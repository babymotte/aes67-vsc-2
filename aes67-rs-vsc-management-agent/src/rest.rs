use crate::{ManagementAgentApi, error::ManagementAgentResult, netinf_watcher};
use axum::extract::State;

pub(crate) async fn app_name<'a>(State(app_id): State<String>) -> String {
    app_id.clone()
}

pub(crate) async fn refresh_netinfs(
    State(netinf_watcher): State<netinf_watcher::Handle>,
) -> ManagementAgentResult<&'static str> {
    netinf_watcher.refresh().await;
    Ok("Network interfaces refresh triggered")
}

pub(crate) async fn vsc_start(State(api): State<ManagementAgentApi>) -> ManagementAgentResult<()> {
    api.start_vsc().await?;
    Ok(())
}

pub(crate) async fn vsc_stop(State(api): State<ManagementAgentApi>) -> ManagementAgentResult<()> {
    // api.stop_vsc().await?;
    // TODO due to leaky implementations in statime currently the only clean way to stop the VSC is to stop the whole application
    api.exit().await?;
    Ok(())
}

pub(crate) async fn vsc_tx_create(
    State(api): State<ManagementAgentApi>,
) -> ManagementAgentResult<()> {
    api.create_sender().await?;
    Ok(())
}

pub(crate) async fn vsc_tx_update(
    State(api): State<ManagementAgentApi>,
) -> ManagementAgentResult<()> {
    api.update_sender().await?;
    Ok(())
}

pub(crate) async fn vsc_tx_delete(
    State(api): State<ManagementAgentApi>,
) -> ManagementAgentResult<()> {
    api.delete_sender().await?;
    Ok(())
}

pub(crate) async fn vsc_rx_create(
    State(api): State<ManagementAgentApi>,
) -> ManagementAgentResult<()> {
    api.create_receiver().await?;
    Ok(())
}

pub(crate) async fn vsc_rx_update(
    State(api): State<ManagementAgentApi>,
) -> ManagementAgentResult<()> {
    api.update_receiver().await?;
    Ok(())
}

pub(crate) async fn vsc_rx_delete(
    State(api): State<ManagementAgentApi>,
) -> ManagementAgentResult<()> {
    api.delete_receiver().await?;
    Ok(())
}
