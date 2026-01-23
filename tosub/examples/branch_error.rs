use miette::Report;
use std::{io, time::Duration};
use tokio::{select, time::sleep};
use tosub::Subsystem;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            fmt::Layer::new().with_writer(io::stderr).with_filter(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .with_env_var("WORTERBUCH_LOG")
                    .from_env_lossy(),
            ),
        )
        .init();

    Subsystem::build_root("root")
        .catch_signals()
        .with_timeout(Duration::from_secs(5))
        .start(|s| async move {
            s.spawn("branch", |s| async move {
                let mut tick = s.spawn("tick", |s| async move {
                    for i in 0..10 {
                        select! {
                            _ = sleep(Duration::from_millis(950)) => println!("tick {i}"),
                            _ = s.shutdown_requested() => break,
                        }
                    }
                    Ok::<(), miette::Report>(())
                });

                let mut tock = s.spawn("tock", |s| async move {
                    for i in 0..10 {
                        if i == 4 {
                            return Err(miette::miette!("Oh noes!"));
                        }
                        select! {
                            _ = sleep(Duration::from_millis(1000)) => println!("tock {i}"),
                            _ = s.shutdown_requested() => break,
                        }
                    }
                    Ok::<(), miette::Report>(())
                });

                tick.join().await;
                tock.join().await;
                s.request_global_shutdown();

                Ok::<(), miette::Report>(())
            })
            .join()
            .await;

            Ok::<(), Report>(())
        })
        .join()
        .await;
}
