use dirs::config_local_dir;
use jack::{AsyncClient, Client, Control, NotificationHandler, ProcessHandler};
use miette::{Context, IntoDiagnostic, Result, miette};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, hash_map::Entry},
    path::PathBuf,
};
use tokio::{fs, select, sync::mpsc};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tracing::{error, info, instrument, warn};

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

pub struct SessionManagerNotificationHandler {
    pub tx: mpsc::Sender<Notification>,
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

pub fn start_session_manager<N, P>(
    subsys: &SubsystemHandle,
    client: AsyncClient<N, P>,
    notifications: mpsc::Receiver<Notification>,
) where
    N: 'static + Send + Sync + NotificationHandler,
    P: 'static + Send + ProcessHandler,
{
    subsys.start(SubsystemBuilder::new("session_manager", |s| {
        run(s, client, notifications)
    }));
}

async fn run<N, P>(
    subsys: SubsystemHandle,
    client: AsyncClient<N, P>,
    mut notifications: mpsc::Receiver<Notification>,
) -> miette::Result<()>
where
    N: 'static + Send + Sync + NotificationHandler,
    P: 'static + Send + ProcessHandler,
{
    restore_connections(client.as_client()).await;

    loop {
        select! {
            Some(notification) = notifications.recv() => handle_notification(client.as_client(), notification).await?,
            _ = subsys.on_shutdown_requested() => break,
            else => break,
        }
    }

    if let Err(e) = client.deactivate() {
        error!("Error deactivating JACK client: {e}");
    }

    Ok(())
}

async fn handle_notification(client: &Client, notification: Notification) -> miette::Result<()> {
    match notification {
        Notification::ThreadInit => {
            info!("JACK thread initialized");
        }
        Notification::Shutdown(client_status, reason) => {
            info!(
                "JACK thread shutting down with status {:?}: {}",
                client_status, reason
            );
        }
        Notification::SampleRate(srate) => {
            info!("JACK sample rate changed to {srate} Hz");
        }
        Notification::ClientRegistration(name, is_registered) => {
            info!("JACK client '{name}' registered: {is_registered}");
        }
        Notification::PortRegistration(port_id, is_registered) => {
            info!("JACK port '{port_id}' registered: {is_registered}");
            // TODO check if we should be sending to a newly connected port and establish a connection if necessary
        }
        Notification::PortRename(_port_id, _old_name, _new_name) => {
            // TODO check if this affects our persisted connection table
        }
        Notification::PortConnected(port_id_a, port_id_b, are_connected) => {
            if let Some(port) = client.port_by_id(port_id_b)
                && client.is_mine(&port) {
                    if are_connected {
                        info!("JACK sender ports connected: {port_id_a} -> {port_id_b}")
                    } else {
                        info!("JACK sender ports disconnected: {port_id_a} -/> {port_id_b}")
                    }
                    store_connection(client, port_id_a, port_id_b, are_connected).await;
                }

            if let Some(port) = client.port_by_id(port_id_a)
                && client.is_mine(&port) {
                    if are_connected {
                        info!("JACK receiver ports connected: {port_id_a} -> {port_id_b}")
                    } else {
                        info!("JACK receiver ports disconnected: {port_id_a} -/> {port_id_b}")
                    }
                    store_connection(client, port_id_a, port_id_b, are_connected).await;
                }
        }
        Notification::GraphReorder => {
            info!("JACK graph reorder");
        }
        Notification::XRun => {
            // TODO report playout xrun
            warn!("JACK buffer xrun");
        }
    }

    Ok(())
}

#[instrument(skip(client))]
async fn store_connection(client: &Client, port_id_a: u32, port_id_b: u32, are_connected: bool) {
    info!("Persisting port connections …");

    let Some(port) = client.port_by_id(port_id_a) else {
        warn!("Unknown port: {port_id_a}");
        return;
    };

    let Ok(this_port_name) = port.name() else {
        warn!("Could not get name of port {port_id_a}");
        return;
    };

    let Some(other_port_name) = client.port_by_id(port_id_b).and_then(|p| p.name().ok()) else {
        warn!("Could not get name of port {port_id_b}");
        return;
    };

    let mut config = match load_client_config(client).await {
        Ok(it) => it,
        Err(e) => {
            warn!("Could not load client config: {e}");
            return;
        }
    };

    if are_connected {
        add_connection(&mut config, this_port_name, other_port_name);
    } else {
        remove_connection(&mut config, this_port_name, other_port_name);
    }

    if let Err(e) = save_client_config(client, config).await {
        warn!("Could not write client config to file: {e}");
        return;
    }

    info!("Port connections persisted.");
}

#[instrument(skip(config))]
fn add_connection(config: &mut ClientConfig, port_name: String, other_port: String) {
    match config.connections.entry(port_name) {
        Entry::Occupied(mut e) => {
            let connections = e.get_mut();
            if !connections.iter().any(|it| it == &other_port) {
                connections.push(other_port);
            }
        }
        Entry::Vacant(e) => {
            e.insert(vec![other_port]);
        }
    }
}

#[instrument(skip(config))]
fn remove_connection(config: &mut ClientConfig, port_name: String, other_port: String) {
    if let Entry::Occupied(mut e) = config.connections.entry(port_name) {
        let connections = e.get_mut();
        connections.retain(|it| it != &other_port);
        if connections.is_empty() {
            e.remove();
        }
    }
}

#[instrument(skip(client))]
async fn restore_connections(client: &Client) {
    info!("Restoring port connections from persistence …");

    let config = match load_client_config(client).await {
        Ok(it) => it,
        Err(e) => {
            warn!("Could not load client config: {e}");
            return;
        }
    };

    for (port_name, connections) in config.connections {
        if let Some(port) = client.port_by_name(&port_name) {
            for other_port_name in connections {
                if let Some(other_port) = client.port_by_name(&other_port_name) {
                    if let Err(e) = client.connect_ports(&port, &other_port) {
                        warn!(
                            "Could not connect ports {} -> {}: {}",
                            port_name, other_port_name, e
                        );
                    }
                } else {
                    warn!("Port {} not found.", other_port_name);
                }
            }
        } else {
            warn!("Port {} not found.", port_name);
        }
    }

    info!("Port connections restored from persistence.");
}

#[instrument(skip(client), err)]
async fn load_client_config(client: &Client) -> Result<ClientConfig> {
    let config_file_path =
        client_config_file_path(client).ok_or_else(|| miette!("Could not get user dir."))?;

    if let Some(parent) = config_file_path.parent() {
        fs::create_dir_all(parent)
            .await
            .into_diagnostic()
            .wrap_err("Could not create config dir")?;
    }

    let file_content = fs::read_to_string(&config_file_path).await.ok();
    let config = file_content
        .and_then(|s| serde_yaml::from_str(&s).ok())
        .unwrap_or_else(ClientConfig::default);

    Ok(config)
}

#[instrument(skip(client, config), err)]
async fn save_client_config(client: &Client, config: ClientConfig) -> Result<()> {
    let config_file_path =
        client_config_file_path(client).ok_or_else(|| miette!("Could not get user dir."))?;

    let new_yaml = serde_yaml::to_string(&config)
        .into_diagnostic()
        .wrap_err("Could not serialize new client config")?;

    fs::write(&config_file_path, new_yaml)
        .await
        .into_diagnostic()
        .wrap_err_with(|| format!("Could not write client config to {:?}", config_file_path))?;

    Ok(())
}

#[instrument(skip(client))]
fn client_config_file_path(client: &Client) -> Option<PathBuf> {
    let client_name = client.name();
    config_local_dir().map(|dir| dir.join("aes67-vsc").join(format!("{client_name}.yaml")))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ClientConfig {
    connections: HashMap<String, Vec<String>>,
}
