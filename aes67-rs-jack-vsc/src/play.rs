use crate::{
    common::JackClock,
    session_manager::{SessionManagerNotificationHandler, start_session_manager},
};
use aes67_rs::{
    receiver::{api::ReceiverApi, config::RxDescriptor},
    time::{MILLIS_PER_SEC_F, MediaClock},
};
use jack::{
    AudioOut, Client, ClientOptions, Control, Port, ProcessScope, contrib::ClosureProcessHandler,
};
use miette::IntoDiagnostic;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::{error, info};

struct State {
    ports: Vec<Port<AudioOut>>,
    receiver: ReceiverApi,
    clock: JackClock,
    descriptor: RxDescriptor,
}

impl State {}

pub async fn start_playout(
    subsys: SubsystemHandle,
    receiver: ReceiverApi,
    descriptor: RxDescriptor,
    clock: Box<dyn MediaClock>,
) -> miette::Result<()> {
    // TODO evaluate client status
    let (client, status) =
        Client::new(&descriptor.id, ClientOptions::default()).into_diagnostic()?;

    info!(
        "JACK client '{}' created with status {:?}",
        descriptor.id, status
    );

    let mut ports = vec![];

    for (i, l) in descriptor.channel_labels.iter().enumerate() {
        let label = l.to_owned().unwrap_or(format!("out{}", i + 1));
        ports.push(
            client
                .register_port(&label, AudioOut::default())
                .into_diagnostic()?,
        );
    }

    let (tx, notifications) = mpsc::channel(1024);
    let notification_handler = SessionManagerNotificationHandler { tx };
    let process_handler_state = State {
        ports,
        receiver,
        clock: JackClock::new(clock),
        descriptor,
    };
    let process_handler =
        ClosureProcessHandler::with_state(process_handler_state, process, buffer_change);

    let active_client = client
        .activate_async(notification_handler, process_handler)
        .into_diagnostic()?;
    start_session_manager(&subsys, active_client, notifications);

    subsys.on_shutdown_requested().await;

    Ok(())
}

fn buffer_change(_: &mut State, client: &Client, buffer_len: jack::Frames) -> Control {
    let buffer_ms = buffer_len as f32 * MILLIS_PER_SEC_F / client.sample_rate() as f32;
    info!("JACK buffer size changed to {buffer_len} frames / {buffer_ms:.1} ms");
    Control::Continue
}

fn process(state: &mut State, _: &Client, ps: &ProcessScope) -> Control {
    let start = Instant::now();

    let playout_time = match state.clock.update_clock(ps) {
        Ok(it) => it,
        Err(e) => {
            error!("Could not get current media time: {e}");
            return Control::Quit;
        }
    };

    // TODO get link offset from current config
    let link_offset_frames = state.descriptor.frames_in_link_offset() as u64;
    let ingress_time = playout_time - link_offset_frames;

    let buffers = state.ports.iter_mut().map(|p| Some(p.as_mut_slice(ps)));

    let pre_req = Instant::now();

    if let Err(e) = state.receiver.receive_blocking(buffers, ingress_time) {
        error!("Error receiving audio data: {e}");
        return Control::Quit;
    }

    let post_req = Instant::now();

    // TODO send to monitoring

    let total = post_req.duration_since(start).as_micros();
    let req = post_req.duration_since(pre_req).as_micros();

    if total > 100 {
        eprintln!("latency playout req: {req} µs");
        eprintln!("latency playout total: {total} µs");
    }

    Control::Continue
}
