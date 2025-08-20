use crate::{error::Aes67Vsc2Result, playout::jack::Notification};
use jack::{AsyncClient, Client, NotificationHandler, ProcessHandler};
use miette::{Context, IntoDiagnostic, Result, miette};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, hash_map::Entry},
    env::home_dir,
    path::PathBuf,
};
use tokio::{fs, select, sync::mpsc};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle};
use tracing::{error, info, instrument, warn};

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
) -> Aes67Vsc2Result<()>
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

async fn handle_notification(client: &Client, notification: Notification) -> Aes67Vsc2Result<()> {
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
            if let Some(port) = client.port_by_id(port_id_a) {
                if client.is_mine(&port) {
                    if are_connected {
                        info!("JACK ports connected: {port_id_a} -> {port_id_b}")
                    } else {
                        info!("JACK ports disconnected: {port_id_a} -/> {port_id_b}")
                    }
                    store_connection(client, port_id_a, port_id_b, are_connected).await;
                }
            }
        }
        Notification::GraphReorder => {
            info!("JACK graph reorder");
        }
        Notification::XRun => {
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
    home_dir().map(|dir| {
        dir.join(".config")
            .join("aes67-vsc")
            .join(format!("{client_name}.yaml"))
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ClientConfig {
    connections: HashMap<String, Vec<String>>,
}
