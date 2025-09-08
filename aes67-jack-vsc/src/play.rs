use crate::session_manager::{SessionManagerNotificationHandler, start_session_manager};
use aes67_rs::receiver::{api::ReceiverApi, config::RxDescriptor};
use jack::{
    AudioOut, Client, ClientOptions, Control, Port, ProcessScope, contrib::ClosureProcessHandler,
};
use miette::IntoDiagnostic;
use tokio::sync::mpsc;
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::info;

struct State {
    ports: Vec<Port<AudioOut>>,
}

pub async fn start_playout(
    subsys: SubsystemHandle,
    sender: ReceiverApi,
    descriptor: RxDescriptor,
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
    let process_handler_state = State { ports };
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
    let buffer_ms = buffer_len as f32 * 1_000.0 / client.sample_rate() as f32;
    info!("JACK buffer size changed to {buffer_len} frames / {buffer_ms:.1} ms");
    Control::Continue
}

fn process(state: &mut State, client: &Client, ps: &ProcessScope) -> Control {
    // TODO
    Control::Continue
}
