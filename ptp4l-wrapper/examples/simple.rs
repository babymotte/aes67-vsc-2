use miette::IntoDiagnostic;
use ptp4l_wrapper::config::{DelayMechanism, NetworkTransport, TimeStamping};
use std::{env, time::Duration};
use tracing::{error, info};

#[tokio::main]
async fn main() -> miette::Result<()> {
    tracing_subscriber::fmt::init();

    let iface_name = env::args()
        .nth(1)
        .expect("Please provide a network interface name as the first argument.");

    if let Err(e) = tosub::build_root("simple_ptp4l")
        .catch_signals()
        .with_timeout(Duration::from_secs(1))
        .start(move |subsys| run(subsys, iface_name))
        .await
        .into_diagnostic()
    {
        error!("{e:?}");
        return Err(e);
    }

    Ok(())
}

async fn run(
    subsys: tosub::SubsystemHandle,
    iface_name: String,
) -> ptp4l_wrapper::error::Result<()> {
    let app_id = subsys.name().to_owned();

    let config_dir = dirs::config_dir()
        .ok_or_else(|| ptp4l_wrapper::error::Error::ConfigDirNotFound)?
        .join(&app_id)
        .join("ptp4l");

    let runtime_dir = dirs::runtime_dir()
        .ok_or_else(|| ptp4l_wrapper::error::Error::RuntimeDirNotFound)?
        .join(&app_id)
        .join("ptp4l");

    let uds_path = runtime_dir.join(format!("{}.sock", iface_name));
    let uds_ro_path = runtime_dir.join(format!("{}-ro.sock", iface_name));

    let config = ptp4l_wrapper::config::Config {
        global: ptp4l_wrapper::config::GlobalConfig {
            // Slave-only: never participate in master election
            two_step_flag: Some(1),
            client_only: Some(1),
            priority1: Some(255),
            priority2: Some(255),
            // Hardware timestamping
            time_stamping: Some(TimeStamping::Hardware),
            network_transport: Some(NetworkTransport::UdpV4),
            delay_mechanism: Some(DelayMechanism::E2E),
            verbose: Some(1),
            logging_level: Some(6),
            uds_address: Some(uds_path.to_owned()),
            uds_ro_address: Some(uds_ro_path.to_owned()),
            ..Default::default()
        },
    };

    ptp4l_wrapper::start_ptpt4l(
        &subsys,
        Some("/usr/bin/ptp4l"),
        config,
        iface_name,
        config_dir,
    )
    .await?
    .shutdown_requested()
    .await;

    info!("ptp4l stopped.");

    Ok(())
}
