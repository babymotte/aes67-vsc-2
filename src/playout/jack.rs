/*
 *  Copyright (C) 2025 Michael Bachmann
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use crate::{
    buffer::{AudioBufferRef, open_audio_buffer},
    config::Config,
    error::Aes67Vsc2Result,
    formats::SampleReader,
    playout::{
        api::{PlayoutApi, PlayoutApiMessage},
        webserver::start_webserver,
    },
    receiver::{api::ReceiverApi, config::RxDescriptor},
    time::MediaClock,
    utils::{AverageCalculationBuffer, set_realtime_priority},
};
use jack::{
    AudioOut, Client, ClientOptions, Control, NotificationHandler, Port, ProcessScope,
    contrib::ClosureProcessHandler,
};
use std::{net::SocketAddr, thread};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tracing::{error, info, instrument, warn};
use worterbuch_client::Worterbuch;

#[instrument(skip(subsys, wb, clock))]
pub async fn start_jack_playout<C: MediaClock>(
    subsys: &SubsystemHandle,
    config: Config,
    use_tls: bool,
    wb: Worterbuch,
    clock: C,
    compensate_clock_drift: bool,
) -> Aes67Vsc2Result<PlayoutApi> {
    let id = config.app.instance.name.clone();
    let (ready_tx, ready_rx) = oneshot::channel();
    subsys.start(SubsystemBuilder::new(format!("receiver-{id}"), move |s| {
        run(s, config, ready_tx, wb, clock, compensate_clock_drift)
    }));
    let api_address = ready_rx.await?;
    info!("Receiver '{id}' started successfully.");
    Ok(PlayoutApi::new(api_address, use_tls))
}

async fn run<C: MediaClock>(
    subsys: SubsystemHandle,
    config: Config,
    ready_tx: oneshot::Sender<SocketAddr>,
    wb: Worterbuch,
    clock: C,
    compensate_clock_drift: bool,
) -> Aes67Vsc2Result<()> {
    let (api_tx, api_rx) = mpsc::channel(1024);
    PlayoutActor::start(
        &subsys,
        api_rx,
        config.clone(),
        wb,
        clock,
        compensate_clock_drift,
    )
    .await?;
    start_webserver(&subsys, config, api_tx, ready_tx);

    Ok(())
}

struct PlayoutActor {
    subsys: SubsystemHandle,
    config: Config,
    api_rx: mpsc::Receiver<PlayoutApiMessage>,
}

impl PlayoutActor {
    #[instrument(skip(subsys, api_rx, _wb, clock))]
    async fn start<C: MediaClock>(
        subsys: &SubsystemHandle,
        api_rx: mpsc::Receiver<PlayoutApiMessage>,
        config: Config,
        _wb: Worterbuch,
        clock: C,
        compensate_clock_drift: bool,
    ) -> Aes67Vsc2Result<()> {
        let playout_config = config.playout_config.clone().expect("no playout config");
        let id: String = config.app.instance.name.clone();

        info!("Starting JACK playout {id} with config {playout_config:?}");

        subsys.start(SubsystemBuilder::new("actor", move |s| async move {
            PlayoutActor {
                subsys: s,
                api_rx,
                config,
            }
            .run(clock, compensate_clock_drift)
            .await
        }));

        Ok(())
    }

    async fn run<C: MediaClock>(
        mut self,
        clock: C,
        compensate_clock_drift: bool,
    ) -> Aes67Vsc2Result<()> {
        info!(
            "Receiver actor '{}' started.",
            self.config.app.instance.name
        );

        let playout_config = self
            .config
            .playout_config
            .as_ref()
            .expect("no playout config");

        let receiver_api = ReceiverApi::with_url(playout_config.receiver.to_owned());
        let receiver_info = receiver_api.info().await?;
        info!("Got receiver info:\n{receiver_info}");

        let (buffer_ref_tx, buffer_ref_rx) = oneshot::channel();
        let (buffer_ref_drop_tx, buffer_ref_drop_rx) = oneshot::channel();
        let rinf = receiver_info.clone();
        thread::spawn(move || match open_audio_buffer(rinf) {
            Ok(buffer) => {
                let buffer_ref = buffer.get_ref();
                buffer_ref_tx.send(Ok(buffer_ref)).ok();
                buffer_ref_drop_rx.blocking_recv().ok();
                drop(buffer);
            }
            Err(e) => _ = buffer_ref_tx.send(Err(e)),
        });

        let audio_buffer_ref = buffer_ref_rx.await??;

        // TODO evaluate client status
        let (client, _status) =
            Client::new(&self.config.instance_name(), ClientOptions::default())?;

        let mut out_ports = vec![];

        for label in receiver_info
            .descriptor
            .channel_labels
            .iter()
            .enumerate()
            .map(|(i, l)| l.to_owned().unwrap_or_else(|| format!("out{}", i + 1)))
        {
            out_ports.push(client.register_port(&label, AudioOut::default())?);
        }

        let desc = receiver_info.descriptor;
        let drift_calculator_buffer_len = playout_config.clock_drift_compensation_interval as usize
            * desc.audio_format.sample_rate
            / client.buffer_size() as usize;
        let drift_calculator_buffer = vec![0; drift_calculator_buffer_len];

        let notification_handler = TracingNotificationHandler;
        let process_handler_state = ProcessHandlerState {
            out_ports,
            audio_buffer_ref,
            desc,
            clock,
            jack_media_clock: None,
            drift_calculator: AverageCalculationBuffer::new(drift_calculator_buffer.into()),
            drift_slew: 0,
            thread_prio_set: false,
            compensate_clock_drift,
        };
        let process_handler =
            ClosureProcessHandler::with_state(process_handler_state, process, buffer_change);

        // TODO set buffer size?
        let active_client = client.activate_async(notification_handler, process_handler)?;

        // TODO connect ports

        // TODO get shared memory pointers from used receivers
        // TODO get media clock
        // TODO check if JACK and receiver audio format are compatible
        // TODO play out audio from shared memory

        loop {
            select! {
                Some(api_msg) = self.api_rx.recv() => if self.process_api_message(api_msg).await.is_err() {
                    break;
                },
                _ = self.subsys.on_shutdown_requested() => break,
                else => break,
            }
        }
        if let Err(e) = active_client.deactivate() {
            error!("Error deactivating JACK client: {e}");
        }

        buffer_ref_drop_tx.send(()).ok();
        self.stop();

        info!(
            "Receiver actor '{}' stopped.",
            self.config.app.instance.name
        );

        Ok(())
    }

    #[instrument(skip(self), err)]
    async fn process_api_message(&mut self, api_msg: PlayoutApiMessage) -> Aes67Vsc2Result<()> {
        info!("Received API message: {api_msg:?}");

        match api_msg {
            PlayoutApiMessage::Stop => self.stop(),
        }

        Ok(())
    }

    #[instrument(skip(self))]
    fn stop(&mut self) {
        self.subsys.request_shutdown();
    }
}

struct ProcessHandlerState<C: MediaClock> {
    out_ports: Vec<Port<AudioOut>>,
    audio_buffer_ref: AudioBufferRef,
    desc: RxDescriptor,
    clock: C,
    jack_media_clock: Option<u64>,
    drift_calculator: AverageCalculationBuffer,
    drift_slew: i64,
    thread_prio_set: bool,
    compensate_clock_drift: bool,
}

impl<C: MediaClock> ProcessHandlerState<C> {
    pub fn slew(&mut self, jack_media_time: u64) -> u64 {
        if self.drift_slew != 0 {
            info!("JACK clock slew {} frames", self.drift_slew);
        }
        self.drift_slew -= self.drift_slew.signum();
        (jack_media_time as i64 + self.drift_slew.signum()) as u64
    }
}

struct TracingNotificationHandler;

impl NotificationHandler for TracingNotificationHandler {
    fn thread_init(&self, _: &Client) {
        info!("JACK thread initialized");
    }

    unsafe fn shutdown(&mut self, _status: jack::ClientStatus, reason: &str) {
        info!("JACK thread shutting down: {reason}");
    }

    fn sample_rate(&mut self, _: &Client, srate: jack::Frames) -> Control {
        info!("JACK sample rate changed to {srate} Hz");
        Control::Continue
    }

    fn client_registration(&mut self, _: &Client, name: &str, is_registered: bool) {
        info!("JACK client '{name}' registered: {is_registered}")
    }

    fn port_registration(&mut self, _: &Client, _port_id: jack::PortId, _is_registered: bool) {
        // TODO check if we should be sending to a newly connected port and establish a connection if necessary
    }

    fn port_rename(
        &mut self,
        _: &Client,
        _port_id: jack::PortId,
        _old_name: &str,
        _new_name: &str,
    ) -> Control {
        Control::Continue
    }

    fn ports_connected(
        &mut self,
        client: &Client,
        port_id_a: jack::PortId,
        port_id_b: jack::PortId,
        are_connected: bool,
    ) {
        if let Some(port) = client.port_by_id(port_id_a) {
            if client.is_mine(&port) {
                if are_connected {
                    info!("JACK ports connected: {port_id_a} -> {port_id_b}")
                } else {
                    info!("JACK ports disconnected: {port_id_a} -/> {port_id_b}")
                }
                // TODO store ports connections and restore them on startup
            }
        }
    }

    fn graph_reorder(&mut self, _: &Client) -> Control {
        Control::Continue
    }

    fn xrun(&mut self, _: &Client) -> Control {
        warn!("JACK buffer over-/underrun");
        Control::Continue
    }
}

fn buffer_change<C: MediaClock>(
    _: &mut ProcessHandlerState<C>,
    _: &Client,
    buffer_len: jack::Frames,
) -> Control {
    info!("JACK buffer size changed to {buffer_len} frames");
    Control::Continue
}

fn process<C: MediaClock>(
    state: &mut ProcessHandlerState<C>,
    _: &Client,
    ps: &ProcessScope,
) -> Control {
    if !state.thread_prio_set {
        set_realtime_priority();
        state.thread_prio_set = true;
    }

    let jack_buffer_len = state
        .out_ports
        .iter_mut()
        .next()
        .map(|p| p.as_mut_slice(ps).len())
        .unwrap_or(0) as u64;

    let current_media_time = state.clock.current_media_time();
    let jack_media_time = state.jack_media_clock.unwrap_or(current_media_time);

    let link_offset = state.desc.link_offset;
    let link_offset_frames = f32::floor(link_offset * state.desc.frames_per_ms() as f32) as u64;

    let current_drift = jack_media_time as i64 - current_media_time as i64;

    let next_media_time = if state.compensate_clock_drift {
        if let Some(drift) = state.drift_calculator.update(current_drift) {
            // we got a new average drift, let's see if we need to compensate
            if drift.unsigned_abs() > link_offset_frames / 2 {
                warn!(
                    "JACK media clock if too far off ({drift} frames), resetting it to ptp media clock"
                );
                state.drift_slew = 0;
                current_media_time
            } else {
                if drift != 0 {
                    warn!(
                        "Current JACK clock drift: {} frames / {} µs",
                        drift,
                        (drift * 1_000_000) / state.desc.audio_format.sample_rate as i64
                    );
                }
                state.drift_slew = -drift;
                state.slew(jack_media_time)
            }
        } else {
            state.slew(jack_media_time)
        }
    } else {
        jack_media_time
    } + jack_buffer_len;

    state.jack_media_clock = Some(next_media_time);

    let ingress_time = jack_media_time - link_offset_frames;
    let playout_time = ingress_time - jack_buffer_len;

    let buffer = state.audio_buffer_ref.buffer();

    for (port_nr, port) in state.out_ports.iter_mut().enumerate() {
        let output_buffer = port.as_mut_slice(ps);

        let bytes_per_buffer_sample = state
            .desc
            .audio_format
            .frame_format
            .sample_format
            .bytes_per_sample();
        let bytes_per_buffer_frame = state.desc.audio_format.frame_format.bytes_per_frame();
        let sample_format = state.desc.audio_format.frame_format.sample_format;
        let frames_per_buffer = buffer.len() / bytes_per_buffer_frame;

        let frame_start = (playout_time % frames_per_buffer as u64) as usize;

        for (frame, sample) in output_buffer.iter_mut().enumerate() {
            let buffer_frame_index = (frame_start + frame) % frames_per_buffer;

            let sample_index_in_frame = port_nr * bytes_per_buffer_sample;
            let sample_start = buffer_frame_index * bytes_per_buffer_frame + sample_index_in_frame;
            let sample_end = sample_start + bytes_per_buffer_sample;
            let buf = &buffer[sample_start..sample_end];

            *sample = sample_format.read_sample(buf);
        }
    }

    Control::Continue
}
