/*
 *  Copyright (C) 2025 Michael Bachmann
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    collections::HashMap,
    fmt::{self, Debug, Display},
    process,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    select,
    signal::unix::{Signal, SignalKind, signal},
    spawn,
    sync::watch,
    time::timeout,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub enum SubsysResult {
    Pending,
    Ok,
    Error(SubsystemError),
}

#[derive(Debug, Clone)]
pub enum SubsystemError {
    Error(String),
    ChildErrors(HashMap<String, SubsysResult>),
}

impl SubsysResult {
    pub fn is_done(&self) -> bool {
        match self {
            SubsysResult::Pending => false,
            _ => true,
        }
    }
}

pub struct RootBuilder {
    name: String,
    catch_signals: bool,
    shutdown_timeout: Option<std::time::Duration>,
}

impl RootBuilder {
    pub fn start<E, F>(self, subsys: impl FnOnce(Subsystem) -> F + Send + 'static) -> Subsystem
    where
        F: std::future::Future<Output = Result<(), E>> + Send + 'static,
        E: Display + Debug + Send + 'static,
    {
        let global = CancellationToken::new();
        let local = global.child_token();

        if self.catch_signals {
            self.register_signal_handlers(&global);
        }

        let join = watch::channel(false);
        let join_tx = join.0.clone();

        let cancel_clean_shutdown = CancellationToken::new();

        let system = Subsystem {
            name: self.name,
            global: global.clone(),
            local: local.clone(),
            join,
            cancel_clean_shutdown: cancel_clean_shutdown.clone(),
            children: Arc::new(Mutex::new(Some(HashMap::new()))),
        };

        let children = system.children.clone();

        let sys = system.clone();
        let glob = global.clone();
        if let Some(to) = self.shutdown_timeout {
            let cancel_clean_shutdown = cancel_clean_shutdown.clone();

            spawn(async move {
                match subsys(sys).await {
                    Ok(_) => info!("Root system terminated normally."),
                    Err(e) => error!("Root system terminated with error: {e}"),
                }

                glob.cancel();

                info!(
                    "Shutdown initiated, waiting up to {:?} for clean shutdown.",
                    to
                );

                let Some(children) = children.lock().expect("mutex is poisoned").take() else {
                    error!("Root system has already been shut down.");
                    return;
                };

                let children_shutdown = Subsystem::wait_for_children_shutdown(children);

                match timeout(to, children_shutdown).await {
                    Ok(_) => {
                        info!("All subsystems have shut down cleanly.");
                        join_tx.send(true).ok();
                    }
                    Err(_) => {
                        error!("Shutdown timeout reached, forcing shutdown …");
                        cancel_clean_shutdown.cancel();
                        join_tx.send(true).ok();
                    }
                }
            });
        } else {
            spawn(async move {
                match subsys(sys).await {
                    Ok(_) => info!("Root system terminated normally."),
                    Err(e) => error!("Root system terminated with error: {e}"),
                }

                glob.cancel();

                info!("Shutdown initiated, waiting for clean shutdown.");

                let Some(children) = children.lock().expect("mutex is poisoned").take() else {
                    error!("Root system has already been shut down.");
                    return;
                };

                Subsystem::wait_for_children_shutdown(children).await;
                info!("All subsystems have shut down cleanly.");
                join_tx.send(true).ok();
            });
        }

        system
    }

    pub fn catch_signals(mut self) -> Self {
        self.catch_signals = true;
        self
    }

    pub fn with_timeout(mut self, shutdown_timeout: Duration) -> Self {
        self.shutdown_timeout = Some(shutdown_timeout);
        self
    }

    fn register_signal_handlers(&self, global: &CancellationToken) {
        if let Ok(signal) = signal(SignalKind::hangup()) {
            handle_signal(
                global,
                signal,
                "SIGHUP",
                SignalKind::hangup().as_raw_value(),
            );
        } else {
            error!("Failed to register SIGHUP handler");
        }

        if let Ok(signal) = signal(SignalKind::interrupt()) {
            handle_signal(
                global,
                signal,
                "SIGINT",
                SignalKind::interrupt().as_raw_value(),
            );
        } else {
            error!("Failed to register SIGINT handler");
        }

        if let Ok(signal) = signal(SignalKind::quit()) {
            handle_signal(global, signal, "SIGQUIT", SignalKind::quit().as_raw_value());
        } else {
            error!("Failed to register SIGQUIT handler");
        }

        if let Ok(signal) = signal(SignalKind::terminate()) {
            handle_signal(
                global,
                signal,
                "SIGTERM",
                SignalKind::terminate().as_raw_value(),
            );
        } else {
            error!("Failed to register SIGTERM handler");
        }
    }
}

fn handle_signal(
    global: &CancellationToken,
    mut signal: Signal,
    signal_name: &'static str,
    code: i32,
) {
    let global = global.clone();
    spawn(async move {
        let mut counter = 0;
        loop {
            signal.recv().await;
            counter += 1;
            if counter > 1 {
                break;
            }
            info!("Received {signal_name} signal, initiating shutdown.");
            global.cancel();
        }
        process::exit(128 + code);
    });
}

#[derive(Clone)]
pub struct Subsystem {
    name: String,
    local: CancellationToken,
    global: CancellationToken,
    cancel_clean_shutdown: CancellationToken,
    join: (watch::Sender<bool>, watch::Receiver<bool>),
    children: Arc<Mutex<Option<HashMap<String, Box<Subsystem>>>>>,
}

impl fmt::Debug for Subsystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subsystem")
            .field("name", &self.name)
            .finish()
    }
}

impl Subsystem {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn spawn<E, F>(
        &self,
        name: impl AsRef<str>,
        subsys: impl FnOnce(Subsystem) -> F + Send + 'static,
    ) -> Subsystem
    where
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Display + Debug + Send + 'static,
    {
        let join = watch::channel(false);
        let join_tx = join.0.clone();
        let cancel_clean_shutdown = self.cancel_clean_shutdown.clone();

        let child = self.create_child(name, join, cancel_clean_shutdown.clone());
        let full_name = child.name().to_owned();
        let s = child.clone();

        let fname = full_name.clone();
        info!("Spawning subsystem '{}' …", fname);
        let mut join_handle = tokio::spawn(async move {
            info!("Subsystem '{}' started.", fname);
            subsys(s).await
        });

        let global = self.global.clone();

        let child_name = full_name.clone();
        tokio::spawn(async move {
            select! {
                res = &mut join_handle => {
                    match res {
                        Ok(Ok(_)) => {
                            info!("Subsystem '{}' terminated normally.", child_name);
                        }
                        Ok(Err(e)) => {
                            error!("Subsystem '{}' terminated with error: {}", child_name, e);
                            global.cancel();
                        }
                        Err(e) => {
                            if e.is_panic() {
                                error!("Subsystem '{}' panicked: {}", child_name, e);
                                global.cancel();
                            } else {
                                warn!("Subsystem '{}' was shut down forcefully.", child_name);
                            }
                        }
                    }
                    join_tx.send(true).ok();
                },
                _ = cancel_clean_shutdown.cancelled() => {
                    warn!("Subsystem '{}' is being shut down forcefully.", child_name);
                    join_handle.abort();
                },
            }
        });

        {
            let mut children = self.children.lock().expect("mutex is poisoned");
            if let Some(children) = children.as_mut() {
                children.insert(full_name, Box::new(child.clone()));
            }
        }

        child
    }

    pub fn request_global_shutdown(&self) {
        self.global.cancel();
    }

    pub fn request_local_shutdown(&self) {
        self.local.cancel();
    }

    pub fn shutdown_requested(&self) -> impl Future<Output = ()> {
        self.local.cancelled()
    }

    pub fn is_shut_down(&self) -> bool {
        self.local.is_cancelled()
    }

    pub async fn join(&mut self) {
        self.join.1.wait_for(|it| *it).await.ok();
        let children = self.children.lock().expect("mutex is poisoned").take();
        if let Some(children) = children {
            Subsystem::wait_for_children_shutdown(children).await;
        }
    }

    pub fn build_root(name: impl Into<String>) -> RootBuilder {
        RootBuilder {
            name: name.into(),
            catch_signals: false,
            shutdown_timeout: None,
        }
    }

    fn create_child(
        &self,
        name: impl AsRef<str>,
        join: (watch::Sender<bool>, watch::Receiver<bool>),
        cancel_clean_shutdown: CancellationToken,
    ) -> Self {
        let name = format!("{}/{}", self.name, name.as_ref());
        let global = self.global.clone();
        let local = self.local.child_token();
        let children = Arc::new(Mutex::new(Some(HashMap::new())));

        Self {
            name,
            global,
            local,
            join,
            cancel_clean_shutdown,
            children,
        }
    }

    async fn wait_for_children_shutdown(children: HashMap<String, Box<Subsystem>>) {
        for (_, mut child) in children {
            Box::pin(child.join()).await;
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use miette::Report;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    #[tokio::test]
    async fn root_is_created() {
        let subsystem = Subsystem::build_root("test_subsystem").start(|s| async move {
            s.shutdown_requested().await;
            Ok::<(), Report>(())
        });
        assert_eq!(subsystem.name(), "test_subsystem");
    }

    #[tokio::test]
    async fn child_is_created() {
        let root_subsystem = Subsystem::build_root("root").start(|s| async move {
            s.shutdown_requested().await;
            Ok::<(), Report>(())
        });
        let child_subsystem =
            root_subsystem.spawn("child", |_| async move { Ok::<(), Report>(()) });
        assert_eq!(child_subsystem.name(), "root/child");
    }

    #[tokio::test]
    async fn child_global_shutdown_shuts_down_root() {
        let root_subsystem = Subsystem::build_root("root").start(|s| async move {
            s.shutdown_requested().await;
            Ok::<(), Report>(())
        });
        let child_subsystem = root_subsystem.spawn("child", |_| async move {
            sleep(Duration::from_hours(1)).await;
            Ok::<(), Report>(())
        });

        child_subsystem.request_global_shutdown();

        timeout(Duration::from_secs(1), root_subsystem.shutdown_requested())
            .await
            .expect("Root subsystem did not shutdown in time");
    }

    #[tokio::test]
    async fn grandchild_global_shutdown_shuts_down_root() {
        let mut root_subsystem = Subsystem::build_root("root").start(|s| async move {
            s.shutdown_requested().await;
            Ok::<(), Report>(())
        });
        let child_system = root_subsystem.spawn("child", |s| async move {
            s.shutdown_requested().await;
            Ok::<(), Report>(())
        });
        let grandchild_system = child_system.spawn("grandchild", |s| async move {
            s.shutdown_requested().await;
            Ok::<(), Report>(())
        });

        assert!(!root_subsystem.is_shut_down());

        sleep(Duration::from_millis(100)).await;

        grandchild_system.request_global_shutdown();

        timeout(Duration::from_secs(1), root_subsystem.join())
            .await
            .expect("Root subsystem did not shutdown in time");
    }
}
