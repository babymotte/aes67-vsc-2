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

use miette::miette;
use std::{io, time::Duration};
use tokio::time::sleep;
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

    tosub::Subsystem::build_root("hello_world")
        .catch_signals()
        .with_timeout(Duration::from_secs(5))
        .start(|root| async move {
            root.spawn("child 1", |subsystem| async move {
                println!("Hello from {}", subsystem.name());
                sleep(Duration::from_secs(1)).await;
                Err(miette!("Oopsie whoopsie!"))
            });

            root.spawn("child 2", |subsystem| async move {
                println!("Hello from {}", subsystem.name());
                subsystem.shutdown_requested().await;
                println!("{} needs a seconds to shut down ...", subsystem.name());
                sleep(Duration::from_secs(1)).await;
                Ok::<(), miette::ErrReport>(())
            });

            root.spawn("child 3", |subsystem| async move {
                println!("Hello from {}", subsystem.name());
                subsystem.shutdown_requested().await;
                println!("{} needs two seconds to shut down ...", subsystem.name());
                sleep(Duration::from_secs(2)).await;
                Ok::<(), miette::ErrReport>(())
            });

            root.shutdown_requested().await;

            Ok::<(), miette::ErrReport>(())
        })
        .join()
        .await;
}
