use crate::error::{ChildAppError, ChildAppResult};
use std::{error::Error, thread, time::Duration};
use tokio::{runtime, spawn, sync::mpsc};
use tokio_graceful_shutdown::{AsyncSubsysFn, SubsystemBuilder, SubsystemHandle, Toplevel};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
#[cfg(feature = "tokio-metrics")]
use worterbuch_client::Worterbuch;

pub enum AppState {
    Started,
    TerminatedNormally,
    Crashed(Box<dyn Error + Send + Sync>),
}

pub fn spawn_child_app<'a, Err, Subsys>(
    #[cfg(feature = "tokio-metrics")] app_id: String,
    name: String,
    subsystem: Subsys,
    shutdown_token: CancellationToken,
    #[cfg(feature = "tokio-metrics")] wb: Worterbuch,
) -> ChildAppResult<mpsc::Receiver<AppState>>
where
    Subsys: 'static
        + for<'b> AsyncSubsysFn<
            &'b mut SubsystemHandle<Box<dyn std::error::Error + Send + Sync + 'static>>,
            Result<(), Err>,
        >,
    Err: std::error::Error + Send + Sync + 'static,
{
    let (state_tx, state_rx) = mpsc::channel(1);

    let runtime = match runtime::Builder::new_current_thread().enable_all().build() {
        Ok(it) => it,
        Err(e) => {
            return Err(ChildAppError(name, e.to_string()));
        }
    };

    let n = name.clone();
    if let Err(e) = thread::Builder::new().name(name.clone()).spawn(move || {
        start_child_app_runtime(
            #[cfg(feature = "tokio-metrics")]
            app_id,
            n,
            subsystem,
            runtime,
            state_tx,
            shutdown_token,
            #[cfg(feature = "tokio-metrics")]
            wb,
        )
    }) {
        return Err(ChildAppError(name, e.to_string()));
    }

    Ok(state_rx)
}

fn start_child_app_runtime<'a, Err, Subsys>(
    #[cfg(feature = "tokio-metrics")] app_id: String,
    name: String,
    subsystem: Subsys,
    runtime: tokio::runtime::Runtime,
    state_tx: mpsc::Sender<AppState>,
    shutdown_token: CancellationToken,
    #[cfg(feature = "tokio-metrics")] wb: Worterbuch,
) where
    Subsys: 'static
        + for<'b> AsyncSubsysFn<
            &'b mut SubsystemHandle<Box<dyn std::error::Error + Send + Sync + 'static>>,
            Result<(), Err>,
        >,
    Err: std::error::Error + Send + Sync + 'static,
{
    runtime.block_on(async move {
        #[cfg(feature = "tokio-metrics")]
        setup_metrics(app_id, name.clone(), wb).await;

        let n = name.clone();
        let tx = state_tx.clone();
        if let Err(e) = Toplevel::new_with_shutdown_token(
            async move |s: &mut SubsystemHandle| {
                s.start(SubsystemBuilder::new(
                    n,
                    async move |s: &mut SubsystemHandle| {
                        info!("Child app '{}' starting …", name);
                        tx.send(AppState::Started).await.ok();
                        let res = subsystem(s).await;
                        info!("Child app '{}' stopped.", name);
                        tx.send(AppState::TerminatedNormally).await.ok();
                        res
                    },
                ));
            },
            shutdown_token.clone(),
        )
        .handle_shutdown_requests(Duration::from_secs(1))
        .await
        {
            state_tx.send(AppState::Crashed(Box::new(e))).await.ok();
        }
    });
}

#[cfg(feature = "tokio-metrics")]
async fn setup_metrics(app_id: String, subsystem_name: String, wb: Worterbuch) {
    let handle = tokio::runtime::Handle::current();
    let runtime_monitor = tokio_metrics::RuntimeMonitor::new(&handle);
    let frequency = std::time::Duration::from_millis(500);
    tokio::spawn(async move {
        for metrics in runtime_monitor.intervals() {
            use worterbuch_client::topic;

            let root_key = topic!(app_id, "metrics", "tokio", subsystem_name, "metrics");

            wb.set_async(topic!(root_key, "elapsed"), metrics.elapsed)
                .await
                .ok();
            wb.set_async(
                topic!(root_key, "global_queue_depth"),
                metrics.global_queue_depth,
            )
            .await
            .ok();
            wb.set_async(topic!(root_key, "workers_count"), metrics.workers_count)
                .await
                .ok();

            wb.set_async(topic!(root_key, "busy", "ratio"), metrics.busy_ratio())
                .await
                .ok();
            wb.set_async(
                topic!(root_key, "busy", "max_duration"),
                metrics.max_busy_duration,
            )
            .await
            .ok();
            wb.set_async(
                topic!(root_key, "busy", "min_duration"),
                metrics.min_busy_duration,
            )
            .await
            .ok();
            wb.set_async(
                topic!(root_key, "busy", "total_duration"),
                metrics.total_busy_duration,
            )
            .await
            .ok();

            wb.set_async(
                topic!(root_key, "park_count", "max"),
                metrics.max_park_count,
            )
            .await
            .ok();
            wb.set_async(
                topic!(root_key, "park_count", "min"),
                metrics.min_park_count,
            )
            .await
            .ok();
            wb.set_async(
                topic!(root_key, "park_count", "total"),
                metrics.total_park_count,
            )
            .await
            .ok();

            tokio::time::sleep(frequency).await;
        }
    });
}

pub async fn wait_for_start(
    name: String,
    app: &mut mpsc::Receiver<AppState>,
) -> ChildAppResult<()> {
    match app.recv().await {
        Some(AppState::Started) => return Ok(()),
        None | Some(AppState::TerminatedNormally) => {
            let msg = format!("{name} terminated immediately after start.");
            return Err(ChildAppError(name, msg));
        }
        Some(AppState::Crashed(e)) => return Err(ChildAppError(name, e.to_string()).into()),
    }
}

pub fn propagate_exit(mut app: mpsc::Receiver<AppState>, shutdown_token: CancellationToken) {
    spawn(async move {
        while let Some(state) = app.recv().await {
            match state {
                AppState::Started => (),
                AppState::TerminatedNormally => {
                    shutdown_token.cancel();
                    break;
                }
                AppState::Crashed(err) => {
                    error!("Child app crashed with error: {err}");
                    shutdown_token.cancel();
                    break;
                }
            }
        }
    });
}
