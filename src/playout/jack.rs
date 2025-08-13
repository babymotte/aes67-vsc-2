use crate::{
    config::Config,
    error::Aes67Vsc2Result,
    playout::{
        api::{PlayoutApi, PlayoutApiMessage},
        webserver::start_webserver,
    },
    worterbuch::start_worterbuch,
};
use jack::{
    AudioOut, Client, ClientOptions, Control, NotificationHandler, Port, ProcessScope,
    contrib::ClosureProcessHandler,
};
use std::net::SocketAddr;
use tokio::{
    select,
    sync::{mpsc, oneshot},
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tracing::{error, info, instrument, warn};
use worterbuch_client::Worterbuch;

#[instrument(skip(subsys))]
pub async fn start_jack_playout(
    subsys: &SubsystemHandle,
    config: Config,
    use_tls: bool,
) -> Aes67Vsc2Result<PlayoutApi> {
    let id = config.app.instance.name.clone();
    let (ready_tx, ready_rx) = oneshot::channel();
    subsys.start(SubsystemBuilder::new(format!("receiver-{id}"), |s| {
        run(s, config, ready_tx)
    }));
    let api_address = ready_rx.await?;
    info!("Receiver '{id}' started successfully.");
    Ok(PlayoutApi::new(api_address, use_tls))
}

async fn run(
    subsys: SubsystemHandle,
    config: Config,
    ready_tx: oneshot::Sender<SocketAddr>,
) -> Aes67Vsc2Result<()> {
    let wb = start_worterbuch(&subsys, config.clone()).await?;
    let (api_tx, api_rx) = mpsc::channel(1024);
    PlayoutActor::start(&subsys, api_rx, config.clone(), wb).await?;
    start_webserver(&subsys, config, api_tx, ready_tx);

    Ok(())
}

struct PlayoutActor {
    subsys: SubsystemHandle,
    config: Config,
    api_rx: mpsc::Receiver<PlayoutApiMessage>,
}

impl PlayoutActor {
    #[instrument(skip(subsys, api_rx, _wb))]
    async fn start(
        subsys: &SubsystemHandle,
        api_rx: mpsc::Receiver<PlayoutApiMessage>,
        config: Config,
        _wb: Worterbuch,
    ) -> Aes67Vsc2Result<()> {
        let playout_config = config.playout_config.clone().expect("no playout config");
        let id: String = config.app.instance.name.clone();

        info!("Starting JACK playout {id} with config {playout_config:?}");

        subsys.start(SubsystemBuilder::new("actor", |s| async move {
            PlayoutActor {
                subsys: s,
                api_rx,
                config,
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

        let playout_config = self
            .config
            .playout_config
            .as_ref()
            .expect("no playout config");

        // TODO evaluate client status
        // TODO share client with receiver?
        let (client, _status) =
            Client::new(&self.config.instance_name(), ClientOptions::default())?;

        let mut out_ports = vec![];
        for i in 0..playout_config.channels {
            out_ports.push(client.register_port(&format!("out{}", i + 1), AudioOut::default())?);
        }

        let notification_handler = TracingNotificationHandler;
        let process_handler_state = ProcessHandlerState { out_ports };
        let process_handler =
            ClosureProcessHandler::with_state(process_handler_state, process, buffer_change);

        // TODO set buffer size?
        let active_client = client.activate_async(notification_handler, process_handler)?;

        // TODO connect ports

        // TODO get initial routing
        // TODO get API instance for used receivers
        // TODO get shared memory pointers from used receivers
        // TODO get media clock
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

        self.stop();

        info!(
            "Receiver actor '{}' stopped.",
            self.config.app.instance.name
        );

        Ok(())
    }

    #[instrument(skip(self))]
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

struct ProcessHandlerState {
    out_ports: Vec<Port<AudioOut>>,
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

fn buffer_change(
    state: &mut ProcessHandlerState,
    client: &Client,
    buffer_len: jack::Frames,
) -> Control {
    info!("JACK buffer size changed to {buffer_len} frames");
    Control::Continue
}

fn process(state: &mut ProcessHandlerState, client: &Client, ps: &ProcessScope) -> Control {
    for out in state.out_ports.iter_mut() {
        for v in out.as_mut_slice(ps) {
            *v = rand::random();
        }
    }

    Control::Continue
}
