//! This module implements an AES67 compatible receiver.
//! Once started it uses the provided configuration to open a datagram socket and, if applicable, joins a multicast group tp receive RTP data.
//! RTP data is decoded and written to the appropriate frame of a shared memory buffer based on the receiver's current PTP media clock.

pub mod api;
pub mod config;

use crate::{
    buffer::{AudioBuffer, create_shared_memory_buffer},
    config::Config,
    error::Aes67Vsc2Result,
    receiver::{
        api::{ReceiverApi, ReceiverInfo},
        config::{ReceiverConfig, RxDescriptor},
    },
    socket::create_rx_socket,
    time::statime_linux::statime_linux,
    utils::{find_network_interface, panic_to_string},
    webserver::start_webserver,
    worterbuch::start_worterbuch,
};
use rtp_rs::{RtpReader, Seq};
use statime::{Clock, SharedClock, time::Time};
use std::{
    any::Any,
    io::ErrorKind,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use tokio::{
    select,
    sync::{mpsc, oneshot},
    task::{JoinError, JoinHandle, spawn_blocking},
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tracing::{error, info, instrument, warn};
use worterbuch_client::Worterbuch;

#[instrument(skip(subsys))]
pub async fn start_receiver(
    subsys: &SubsystemHandle,
    config: Config,
    use_tls: bool,
) -> Aes67Vsc2Result<ReceiverApi> {
    let id = config.app.instance.name.clone();
    let (ready_tx, ready_rx) = oneshot::channel();
    subsys.start(SubsystemBuilder::new(format!("receiver-{id}"), |s| {
        run(s, config, ready_tx)
    }));
    let api_address = ready_rx.await?;
    info!("Receiver '{id}' started successfully.");
    Ok(ReceiverApi::new(api_address, use_tls))
}

async fn run(
    subsys: SubsystemHandle,
    config: Config,
    ready_tx: oneshot::Sender<SocketAddr>,
) -> Aes67Vsc2Result<()> {
    let wb = start_worterbuch(&subsys, config.clone()).await?;
    let (api_tx, api_rx) = mpsc::channel(1024);
    ReceiverActor::start(&subsys, api_rx, config.clone(), wb).await?;
    start_webserver(&subsys, config, api_tx, ready_tx);

    Ok(())
}

#[derive(Debug)]
pub enum ReceiverApiMessage {
    Stop,
    GetInfo(oneshot::Sender<ReceiverInfo>),
}

struct ReceiverActor {
    subsys: SubsystemHandle,
    config: Config,
    api_rx: mpsc::Receiver<ReceiverApiMessage>,
    rx_thread: JoinHandle<Result<Aes67Vsc2Result<()>, Box<dyn Any + Send>>>,
    rx_cancellation_token: Arc<AtomicBool>,
    info: ReceiverInfo,
}

impl ReceiverActor {
    #[instrument(skip(subsys, api_rx, wb))]
    async fn start(
        subsys: &SubsystemHandle,
        api_rx: mpsc::Receiver<ReceiverApiMessage>,
        config: Config,
        wb: Worterbuch,
    ) -> Aes67Vsc2Result<()> {
        let rx_cancellation_token = Arc::new(AtomicBool::new(false));
        let rxct = rx_cancellation_token.clone();
        let cfg = config.clone();
        let rx_config = cfg.receiver_config.clone().expect("no receiver config");

        let (shmem_addr_tx, shmem_add_rx) = oneshot::channel();

        let id = config.app.instance.name.clone();
        let link_offset = rx_config.link_offset;
        let descriptor = RxDescriptor::new(id, &rx_config.session, link_offset)?;
        let desc = descriptor.clone();

        let iface = find_network_interface(rx_config.interface_ip)?;

        let clock = statime_linux(iface, rx_config.interface_ip, wb, config.instance_name()).await;

        let rx_thread = thread::Builder::new()
            .name("rx-thread".to_owned())
            .spawn(move || {
                RxThread {
                    config: cfg,
                    rx_cancellation_token: rxct,
                    rx_config,
                    desc,
                    last_sequence_number: None,
                    last_timestamp: None,
                    timestamp_offset: None,
                    clock,
                }
                .run(shmem_addr_tx)
            })?;
        let rx_thread = spawn_blocking(|| rx_thread.join());
        let shmem_address = shmem_add_rx.await?;

        let info = ReceiverInfo {
            descriptor,
            shmem_address,
        };

        subsys.start(SubsystemBuilder::new("actor", |s| async move {
            ReceiverActor {
                subsys: s,
                api_rx,
                config,
                rx_thread,
                rx_cancellation_token,
                info,
            }
            .run()
            .await
        }));

        Ok(())
    }

    async fn run(mut self) -> Aes67Vsc2Result<()> {
        info!(
            "Receiver actor '{}' started.",
            self.config.app.instance.name
        );

        loop {
            select! {
                Some(api_msg) = self.api_rx.recv() => if self.process_api_message(api_msg).await.is_err() {
                    break;
                },
                term = &mut self.rx_thread => {
                    self.rx_thread_terminated(term);
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

    #[instrument(skip(self))]
    async fn process_api_message(&mut self, api_msg: ReceiverApiMessage) -> Aes67Vsc2Result<()> {
        info!("Received API message: {api_msg:?}");

        match api_msg {
            ReceiverApiMessage::Stop => self.stop(),
            ReceiverApiMessage::GetInfo(sender) => _ = sender.send(self.info.clone()),
        }

        Ok(())
    }

    #[instrument(skip(self))]
    fn stop(&mut self) {
        self.rx_cancellation_token.store(true, Ordering::Release);
        self.subsys.request_shutdown();
    }

    #[instrument(skip(self, term))]
    fn rx_thread_terminated(
        &self,
        term: Result<Result<Aes67Vsc2Result<()>, Box<dyn Any + Send>>, JoinError>,
    ) {
        match term {
            Ok(Ok(Ok(_))) => info!("RX thread terminated normally."),
            Ok(Ok(Err(e))) => error!("RX thread terminated with error: {e:?}"),
            Ok(Err(e)) => error!("RX thread paniced: {}", panic_to_string(e)),
            Err(e) => error!("Error waiting for RX thread to terminate: {e}"),
        }
    }
}

struct RxThread<C: Clock> {
    config: Config,
    rx_config: ReceiverConfig,
    rx_cancellation_token: Arc<AtomicBool>,
    desc: RxDescriptor,
    last_timestamp: Option<u32>,
    last_sequence_number: Option<Seq>,
    timestamp_offset: Option<u64>,
    clock: SharedClock<C>,
}

impl<C: Clock> RxThread<C> {
    fn run(mut self, path_tx: oneshot::Sender<String>) -> Aes67Vsc2Result<()> {
        let mut audio_buffer =
            create_shared_memory_buffer(&self.config, path_tx, self.desc.clone())?;
        let socket = create_rx_socket(&self.rx_config.session, self.rx_config.interface_ip)?;

        let mut receive_buffer = [0; 65_535];

        loop {
            match socket.recv_from(&mut receive_buffer) {
                Ok((len, addr)) => {
                    self.rtp_data_received(&receive_buffer[..len], addr, &mut audio_buffer)
                }
                Err(e) => {
                    if e.kind() != ErrorKind::WouldBlock {
                        return Err(e.into());
                    }
                }
            };

            if self.rx_cancellation_token.load(Ordering::Acquire) {
                info!("Cancellation token caused RX thread to stop.");
                break;
            }
        }

        info!("RX thread stoppped.");

        Ok(())
    }

    fn rtp_data_received(&mut self, data: &[u8], addr: SocketAddr, buffer: &mut AudioBuffer) {
        if addr.ip() == self.desc.origin_ip {
            let rtp = match RtpReader::new(data) {
                Ok(it) => it,
                Err(e) => {
                    warn!("received malformed rtp packet: {e:?}");
                    return;
                }
            };

            let seq = rtp.sequence_number();
            let ts = rtp.timestamp();

            let mut ts_wrapped = false;
            let mut seq_wrapped = false;

            if let (Some(last_ts), Some(last_seq)) =
                (self.last_timestamp, self.last_sequence_number)
            {
                if last_seq.next() != seq {
                    warn!(
                        "inconsistent sequence number: {} (last was {})",
                        u16::from(seq),
                        u16::from(last_seq)
                    );
                    // TODO this will never recover from a dropped packet, needs some way to recover
                    // TODO track late packets
                    return;
                }

                if last_ts.wrapping_add(self.desc.frames_per_packet() as u32) != ts {
                    warn!("inconsistent timestamp: {} (last was {})", ts, last_ts);
                    // TODO this will never recover from a dropped packet, needs some way to recover
                    // TODO track late packets
                    return;
                }

                ts_wrapped = ts < last_ts;
                seq_wrapped = u16::from(seq) < u16::from(last_seq);
            }

            // TODO track late packets

            if seq_wrapped || self.timestamp_offset.is_none() {
                self.calibrate_timestamp_offset(ts, ts_wrapped);
            }

            if ts_wrapped {
                info!("RTP timestamp wrapped");
                if let Some(previous_offset) = self.timestamp_offset {
                    let new_offset = previous_offset + 2u64.pow(32);
                    info!("Updating RTP timestamp offset from {previous_offset} to {new_offset}");
                    self.timestamp_offset = Some(new_offset);
                } else {
                    self.calibrate_timestamp_offset(ts, ts_wrapped);
                }
            }

            self.last_sequence_number = Some(seq);
            self.last_timestamp = Some(ts);

            if let &Some(offset) = &self.timestamp_offset {
                buffer.insert(rtp, offset);
            }
        } else {
            warn!("Received packet from wrong sender: {addr}");
        }
    }

    #[instrument(skip(self))]
    fn calibrate_timestamp_offset(&mut self, rtp_timestamp: u32, timestamp_wrapped: bool) {
        info!("Calibrating timestamp offset at RTP timestamp {rtp_timestamp}");
        let ptp_time = self.clock.now();
        let media_time = to_media_time(ptp_time, &self.desc);
        let timestamp_wrap = 2u64.pow(32);
        let timestamp_wraps = media_time / timestamp_wrap;
        let timestamp_modulo = media_time % timestamp_wrap;
        let diff = rtp_timestamp as i128 - timestamp_modulo as i128;
        if diff.abs() >= timestamp_modulo as i128 {
            warn!("calibrating timestamp offset close to wrap, calibration may be inaccurate");
        }
        // the offset is the time of the last wrap in media time,
        // i.e. offset + rtp.timestamp should give us an accurate
        // unwrapped media clock timestamp of an rtp packet
        let offset = timestamp_wraps * timestamp_wrap;

        if let Some(previous_offset) = self.timestamp_offset {
            if previous_offset != offset {
                warn!(
                    "RTP timestamp offset changed from {previous_offset} to {offset}, this may lead to audio interruptions"
                );
            } else {
                info!("Offset did not change ({offset})");
            }
        } else {
            info!("Offset: {offset}");
        }

        self.timestamp_offset = Some(offset);
    }
}

fn to_media_time(ptp_time: Time, desc: &RxDescriptor) -> u64 {
    let ptp_nanos = (ptp_time.secs() as u128) * 1_000_000_000 + ptp_time.subsec_nanos() as u128;
    let total_frames = (ptp_nanos * desc.audio_format.sample_rate as u128) / 1_000_000_000;
    total_frames as u64
}
