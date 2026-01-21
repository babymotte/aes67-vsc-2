use crate::{
    common::{ClockState, JackClock},
    session_manager::{SessionManagerNotificationHandler, start_session_manager},
};
use aes67_rs::{
    monitoring::Monitoring,
    receiver::{api::ReceiverApi, config::ReceiverConfig},
    time::{Clock, MILLIS_PER_SEC_F},
};
use jack::{
    AudioOut, Client, ClientOptions, Control, Port, ProcessScope, contrib::ClosureProcessHandler,
};
use miette::IntoDiagnostic;
use std::time::{Duration, Instant};
use tokio::{runtime::Handle, sync::mpsc, time::timeout};
use tokio_graceful_shutdown::{NestedSubsystem, SubsystemHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

struct State {
    ports: Vec<Port<AudioOut>>,
    receiver: ReceiverApi,
    clock: JackClock,
    config: ReceiverConfig,
    muted: bool,
    monitoring: Monitoring,
    shutdown_token: CancellationToken,
    async_runtime: Handle,
}

impl State {}

pub async fn start_playout(
    app_id: String,
    subsys: &mut SubsystemHandle,
    receiver: ReceiverApi,
    config: ReceiverConfig,
    clock: Clock,
    monitoring: Monitoring,
) -> miette::Result<NestedSubsystem> {
    // TODO evaluate client status
    let (client, status) =
        Client::new(&config.label, ClientOptions::default()).into_diagnostic()?;

    info!(
        "JACK client '{}' created with status {:?}",
        config.label, status
    );

    let mut ports = vec![];

    for l in config
        .channel_labels
        .clone()
        .unwrap_or_else(|| {
            (0..config.audio_format.frame_format.channels)
                .map(|i| format!("{}", i + 1))
                .collect()
        })
        .iter()
    {
        let label = l.to_owned();
        ports.push(
            client
                .register_port(&label, AudioOut::default())
                .into_diagnostic()?,
        );
    }

    let (tx, notifications) = mpsc::channel(1024);
    let cid = config.label.clone();
    let notification_handler = SessionManagerNotificationHandler {
        client_id: cid.clone(),
        tx,
    };
    let process_handler_state = State {
        ports,
        receiver,
        clock: JackClock::new(clock),
        config,
        muted: false,
        monitoring,
        shutdown_token: subsys.create_cancellation_token(),
        async_runtime: Handle::current(),
    };
    let process_handler =
        ClosureProcessHandler::with_state(process_handler_state, process, buffer_change);

    let active_client = client
        .activate_async(notification_handler, process_handler)
        .into_diagnostic()?;

    let session_manager = start_session_manager(subsys, active_client, notifications, app_id);

    Ok(session_manager)
}

fn buffer_change(_: &mut State, client: &Client, buffer_len: jack::Frames) -> Control {
    let buffer_ms = buffer_len as f32 * MILLIS_PER_SEC_F / client.sample_rate() as f32;
    info!("JACK buffer size changed to {buffer_len} frames / {buffer_ms:.1} ms");
    Control::Continue
}

fn process(state: &mut State, _: &Client, ps: &ProcessScope) -> Control {
    let start = Instant::now();

    let playout_time = match state.clock.update_clock(ps) {
        Ok(ClockState::Stable(it)) => it,
        Ok(ClockState::Unstable) => {
            muted(state, ps);
            return Control::Continue;
        }
        Err(e) => {
            error!("Could not get current media time: {e}");
            return Control::Quit;
        }
    };

    // TODO read current link offset from dynamic config
    let link_offset_frames = state.config.frames_in_link_offset() as u64;
    let ingress_time = playout_time - link_offset_frames;

    let buffers = state.ports.iter_mut().map(|p| Some(p.as_mut_slice(ps)));

    let pre_req = Instant::now();

    match state.async_runtime.block_on(async {
        timeout(
            Duration::from_millis(100),
            state
                .receiver
                .receive(buffers, ingress_time, &state.shutdown_token),
        )
        .await
    }) {
        Ok(Ok(true)) => {
            unmuted(state);
        }
        Ok(Ok(false)) => {
            muted(state, ps);
        }
        Ok(Err(e)) => {
            error!("Error receiving audio data: {e}");
            return Control::Quit;
        }
        Err(_) => {
            if !state.muted {
                error!("Receiving audio data timed out.");
            }
            muted(state, ps);
        }
    }

    let post_req = Instant::now();

    // TODO send to monitoring

    let _total = post_req.duration_since(start).as_micros();
    let _req = post_req.duration_since(pre_req).as_micros();

    // if total > 100 {
    //     eprintln!("latency playout req: {req} µs");
    //     eprintln!("latency playout total: {total} µs");
    // }

    Control::Continue
}

fn muted(state: &mut State, ps: &ProcessScope) {
    if !state.muted {
        state.muted = true;
        state.report_muted(true);
    }

    for port in state.ports.iter_mut() {
        let buf = port.as_mut_slice(ps);
        buf.fill(0.0);
    }
}

fn unmuted(state: &mut State) {
    if state.muted {
        state.muted = false;
        state.report_muted(false);
    }
}

mod monitoring {
    use aes67_rs::monitoring::RxStats;

    use super::*;

    impl State {
        pub fn report_muted(&self, muted: bool) {
            self.monitoring.receiver_stats(RxStats::Muted(muted));
        }
    }
}
