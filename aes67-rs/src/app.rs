use crate::error::{ChildAppError, ChildAppResult};
use std::{error::Error, thread, time::Duration};
use tokio::{runtime, spawn, sync::mpsc};
use tokio_graceful_shutdown::{ErrTypeTraits, SubsystemBuilder, SubsystemHandle, Toplevel};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub enum AppState {
    Started,
    TerminatedNormally,
    Crashed(Box<dyn Error + Send + Sync>),
}

pub fn spawn_child_app<'a, ErrType, Err, Fut, Subsys>(
    name: String,
    subsystem: Subsys,
    shutdown_token: CancellationToken,
) -> ChildAppResult<mpsc::Receiver<AppState>>
where
    ErrType: ErrTypeTraits,
    Subsys: 'static + FnOnce(SubsystemHandle<ErrType>) -> Fut + Send,
    Fut: 'static + Future<Output = Result<(), Err>> + Send,
    Err: Into<ErrType> + Send,
{
    let (state_tx, state_rx) = mpsc::channel(1);

    let runtime = match runtime::Builder::new_current_thread().enable_all().build() {
        Ok(it) => it,
        Err(e) => {
            return Err(ChildAppError(name, e.to_string()));
        }
    };

    let n = name.clone();
    if let Err(e) = thread::Builder::new()
        .name(name.clone())
        .spawn(move || start_child_app_runtime(n, subsystem, runtime, state_tx, shutdown_token))
    {
        return Err(ChildAppError(name, e.to_string()));
    }

    Ok(state_rx)
}

fn start_child_app_runtime<'a, ErrType, Err, Fut, Subsys>(
    name: String,
    subsystem: Subsys,
    runtime: tokio::runtime::Runtime,
    state_tx: mpsc::Sender<AppState>,
    shutdown_token: CancellationToken,
) where
    ErrType: ErrTypeTraits,
    Subsys: 'static + FnOnce(SubsystemHandle<ErrType>) -> Fut + Send,
    Fut: 'static + Future<Output = Result<(), Err>> + Send,
    Err: Into<ErrType> + Send,
{
    runtime.block_on(async move {
        let n = name.clone();
        let tx = state_tx.clone();
        if let Err(e) = Toplevel::new_with_shutdown_token(
            |s| async move {
                s.start(SubsystemBuilder::new(n, |s| async move {
                    info!("Child app '{}' starting …", name);
                    tx.send(AppState::Started).await.ok();
                    let res = subsystem(s).await;
                    info!("Child app '{}' stopped.", name);
                    tx.send(AppState::TerminatedNormally).await.ok();
                    res
                }));
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
