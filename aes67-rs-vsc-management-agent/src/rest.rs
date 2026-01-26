use crate::{
    ManagementAgentApi, Sdp,
    error::{LogError, ManagementAgentResult},
    netinf_watcher,
};
use axum::{Json, extract::State};

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

#[derive(serde::Deserialize)]
pub(crate) struct TransceiverSpec {
    id: u32,
}

pub(crate) async fn vsc_tx_config_create(
    State(api): State<ManagementAgentApi>,
) -> ManagementAgentResult<()> {
    api.create_sender_config()
        .await
        .log_error("Failed to create sender config")?;
    Ok(())
}

pub(crate) async fn vsc_rx_config_create(
    State(api): State<ManagementAgentApi>,
    Json(sdp): Json<Option<Sdp>>,
) -> ManagementAgentResult<()> {
    api.create_receiver_config(sdp)
        .await
        .log_error("Failed to create receiver config")?;
    Ok(())
}

pub(crate) async fn vsc_tx_create(
    State(api): State<ManagementAgentApi>,
    Json(spec): Json<TransceiverSpec>,
) -> ManagementAgentResult<()> {
    api.create_sender(spec.id)
        .await
        .log_error("Failed to create sender")?;
    Ok(())
}

pub(crate) async fn vsc_tx_update(
    State(api): State<ManagementAgentApi>,
    Json(spec): Json<TransceiverSpec>,
) -> ManagementAgentResult<()> {
    api.update_sender(spec.id)
        .await
        .log_error("Failed to update sender")?;
    Ok(())
}

pub(crate) async fn vsc_tx_delete(
    State(api): State<ManagementAgentApi>,
    Json(spec): Json<TransceiverSpec>,
) -> ManagementAgentResult<()> {
    api.delete_sender(spec.id)
        .await
        .log_error("Failed to delete sender")?;
    Ok(())
}

pub(crate) async fn vsc_rx_create(
    State(api): State<ManagementAgentApi>,
    Json(spec): Json<TransceiverSpec>,
) -> ManagementAgentResult<()> {
    api.create_receiver(spec.id)
        .await
        .log_error("Failed to create receiver")?;
    Ok(())
}

pub(crate) async fn vsc_rx_update(
    State(api): State<ManagementAgentApi>,
    Json(spec): Json<TransceiverSpec>,
) -> ManagementAgentResult<()> {
    api.update_receiver(spec.id)
        .await
        .log_error("Failed to update receiver")?;
    Ok(())
}

pub(crate) async fn vsc_rx_delete(
    State(api): State<ManagementAgentApi>,
    Json(spec): Json<TransceiverSpec>,
) -> ManagementAgentResult<()> {
    api.delete_receiver(spec.id)
        .await
        .log_error("Failed to delete receiver")?;
    Ok(())
}
