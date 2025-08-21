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

mod session_manager;

use crate::{
    buffer::AudioBufferPointer,
    config::Config,
    error::Aes67Vsc2Result,
    playout::{
        api::{PlayoutApi, PlayoutApiMessage},
        jack::session_manager::start_session_manager,
        webserver::start_webserver,
    },
    receiver::{AudioDataRequest, DataState, api::ReceiverApi, config::RxDescriptor},
    time::{MediaClock, wallclock_monotonic_offset_nanos},
    utils::{AverageCalculationBuffer, RequestResponseClientChannel, set_realtime_priority},
};
use jack::{
    AudioOut, Client, ClientOptions, Control, NotificationHandler, Port, ProcessScope,
    contrib::ClosureProcessHandler,
};
use std::{net::SocketAddr, u64};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tracing::{error, info, instrument, warn};
use worterbuch_client::Worterbuch;

#[instrument(skip(subsys, wb, clock, requests))]
pub async fn start_jack_playout<C: MediaClock>(
    subsys: &SubsystemHandle,
    config: Config,
    use_tls: bool,
    wb: Option<Worterbuch>,
    clock: C,
    compensate_clock_drift: bool,
    requests: RequestResponseClientChannel<AudioDataRequest, DataState>,
) -> Aes67Vsc2Result<PlayoutApi> {
    jack::set_logger(jack::LoggerType::Log);
    let id = config.app.instance.name.clone();
    let (ready_tx, ready_rx) = oneshot::channel();
    subsys.start(SubsystemBuilder::new(format!("receiver-{id}"), move |s| {
        run(
            s,
            config,
            ready_tx,
            wb,
            clock,
            compensate_clock_drift,
            requests,
        )
    }));
    let api_address = ready_rx.await?;
    info!("Receiver '{id}' started successfully.");
    Ok(PlayoutApi::new(api_address, use_tls))
}

async fn run<C: MediaClock>(
    subsys: SubsystemHandle,
    config: Config,
    ready_tx: oneshot::Sender<SocketAddr>,
    wb: Option<Worterbuch>,
    clock: C,
    compensate_clock_drift: bool,
    requests: RequestResponseClientChannel<AudioDataRequest, DataState>,
) -> Aes67Vsc2Result<()> {
    let (api_tx, api_rx) = mpsc::channel(1024);
    PlayoutActor::start(
        &subsys,
        api_rx,
        config.clone(),
        wb,
        clock,
        compensate_clock_drift,
        requests,
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
    #[instrument(skip(subsys, api_rx, _wb, clock, requests))]
    async fn start<C: MediaClock>(
        subsys: &SubsystemHandle,
        api_rx: mpsc::Receiver<PlayoutApiMessage>,
        config: Config,
        _wb: Option<Worterbuch>,
        clock: C,
        compensate_clock_drift: bool,
        requests: RequestResponseClientChannel<AudioDataRequest, DataState>,
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
            .run(clock, compensate_clock_drift, requests)
            .await
        }));

        Ok(())
    }

    async fn run<C: MediaClock>(
        mut self,
        clock: C,
        compensate_clock_drift: bool,
        requests: RequestResponseClientChannel<AudioDataRequest, DataState>,
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
        info!(
            "Got receiver info:\n{}",
            serde_yaml::to_string(&receiver_info)?
        );

        // TODO evaluate client status
        let (client, status) = Client::new(&self.config.instance_name(), ClientOptions::default())?;

        info!("JACK client created with status {status:?}");

        let mut out_ports = vec![];

        for label in receiver_info
            .channel_labels
            .iter()
            .enumerate()
            .map(|(i, l)| l.to_owned().unwrap_or_else(|| format!("out{}", i + 1)))
        {
            out_ports.push(client.register_port(&label, AudioOut::default())?);
        }

        let desc = receiver_info;

        let wallclock_offset_usec = (wallclock_monotonic_offset_nanos()? / 1_000) as u64;

        let jack_media_clock_offset_calculator_buffer_len =
            client.sample_rate() / client.buffer_size() as usize;

        let (tx, notifications) = mpsc::channel(1024);
        let notification_handler = SessionManagerNotificationHandler { tx };
        let process_handler_state = ProcessHandlerState {
            out_ports,
            desc,
            clock,
            thread_prio_set: false,
            jack_clock_offset: u64::MAX,
            requests,
            wallclock_offset_usec,
            jack_media_clock_offset_calculator: AverageCalculationBuffer::new(
                vec![0u64; jack_media_clock_offset_calculator_buffer_len].into(),
            ),
            jack_media_clock_offset: 0,
            warmup_counter: 0,
        };
        let process_handler =
            ClosureProcessHandler::with_state(process_handler_state, process, buffer_change);

        let active_client = client.activate_async(notification_handler, process_handler)?;
        start_session_manager(&self.subsys, active_client, notifications);

        loop {
            select! {
                Some(api_msg) = self.api_rx.recv() => if self.process_api_message(api_msg).await.is_err() {
                    break;
                },
                _ = self.subsys.on_shutdown_requested() => break,
                else => break,
            }
        }

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
    desc: RxDescriptor,
    clock: C,
    thread_prio_set: bool,
    jack_clock_offset: u64,
    requests: RequestResponseClientChannel<AudioDataRequest, DataState>,
    wallclock_offset_usec: u64,
    jack_media_clock_offset_calculator: AverageCalculationBuffer<u64>,
    jack_media_clock_offset: u64,
    warmup_counter: u64,
}

pub enum Notification {
    ThreadInit,
    Shutdown(jack::ClientStatus, String),
    SampleRate(jack::Frames),
    ClientRegistration(String, bool),
    PortRegistration(jack::PortId, bool),
    PortRename(jack::PortId, String, String),
    PortConnected(jack::PortId, jack::PortId, bool),
    GraphReorder,
    XRun,
}

struct SessionManagerNotificationHandler {
    tx: mpsc::Sender<Notification>,
}

impl NotificationHandler for SessionManagerNotificationHandler {
    fn thread_init(&self, _: &Client) {
        self.tx.try_send(Notification::ThreadInit).ok();
    }

    unsafe fn shutdown(&mut self, status: jack::ClientStatus, reason: &str) {
        self.tx
            .try_send(Notification::Shutdown(status, reason.to_owned()))
            .ok();
    }

    fn sample_rate(&mut self, _: &Client, srate: jack::Frames) -> Control {
        self.tx.try_send(Notification::SampleRate(srate)).ok();
        Control::Continue
    }

    fn client_registration(&mut self, _: &Client, name: &str, is_registered: bool) {
        self.tx
            .try_send(Notification::ClientRegistration(
                name.to_owned(),
                is_registered,
            ))
            .ok();
    }

    fn port_registration(&mut self, _: &Client, port_id: jack::PortId, is_registered: bool) {
        self.tx
            .try_send(Notification::PortRegistration(port_id, is_registered))
            .ok();
    }

    fn port_rename(
        &mut self,
        _: &Client,
        port_id: jack::PortId,
        old_name: &str,
        new_name: &str,
    ) -> Control {
        self.tx
            .try_send(Notification::PortRename(
                port_id,
                old_name.to_owned(),
                new_name.to_owned(),
            ))
            .ok();
        Control::Continue
    }

    fn ports_connected(
        &mut self,
        _: &Client,
        port_id_a: jack::PortId,
        port_id_b: jack::PortId,
        are_connected: bool,
    ) {
        self.tx
            .try_send(Notification::PortConnected(
                port_id_a,
                port_id_b,
                are_connected,
            ))
            .ok();
    }

    fn graph_reorder(&mut self, _: &Client) -> Control {
        self.tx.try_send(Notification::GraphReorder).ok();
        Control::Continue
    }

    fn xrun(&mut self, _: &Client) -> Control {
        self.tx.try_send(Notification::XRun).ok();
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
    client: &Client,
    ps: &ProcessScope,
) -> Control {
    if state.warmup_counter < 100 {
        state.warmup_counter += 1;
        return Control::Continue;
    }

    if !state.thread_prio_set {
        set_realtime_priority();
        state.thread_prio_set = true;
    }

    if state.jack_media_clock_offset == 0 {
        let cycle_start_monotonic_usec = client.frames_to_time(ps.last_frame_time());
        let cycle_start_wallclock_usec = cycle_start_monotonic_usec + state.wallclock_offset_usec;
        let cycle_start_media_time =
            (cycle_start_wallclock_usec as f64 * client.sample_rate() as f64 / 1_000_000.0).round()
                as u64;
        let offset = cycle_start_media_time - ps.last_frame_time() as u64;
        if let Some(offset) = state.jack_media_clock_offset_calculator.update(offset) {
            info!("Clock calibration done, JACK media clock offset is {offset}");
            state.jack_media_clock_offset = offset;
        }
        return Control::Continue;
    }

    let media_time = state.jack_media_clock_offset + ps.last_frame_time() as u64;

    let Ok(current_media_time) = state.clock.current_media_time() else {
        error!("Could not get system media time!");
        return Control::Quit;
    };
    if current_media_time > media_time {
        let diff = current_media_time - media_time;
        let diff_usec = (diff as f64 * 1_000_000.0 / client.sample_rate() as f64).round() as u64;
        // warn!(
        //     "JACK clock is behind media clock by {diff} frames / {diff_usec} µs (expected to be ahead)!",
        // );
        // return Control::Quit;
    } else {
        let diff = media_time - current_media_time;
        let diff_usec = (diff as f64 * 1_000_000.0 / client.sample_rate() as f64).round() as u64;
        // warn!("JACK clock is ahead of media clock by {diff} frames / {diff_usec} µs (expected).",);
    }

    for (port_nr, port) in state.out_ports.iter_mut().enumerate() {
        let connected = port.connected_count().unwrap_or(0) != 0;

        let output_buffer = port.as_mut_slice(ps);
        if !connected {
            output_buffer.fill(0.0);
            continue;
        }

        'request: loop {
            match state.requests.request_blocking(AudioDataRequest {
                buffer: AudioBufferPointer::from_slice(&output_buffer),
                channel: port_nr,
                playout_time: media_time,
            }) {
                Some(DataState::Missed) => {
                    warn!("Did not get data from receiver in time!");
                    break 'request;
                }
                Some(DataState::Ready) => break 'request,
                Some(DataState::Wait) => continue 'request,
                None => return Control::Quit,
            }
        }
    }

    Control::Continue
}
