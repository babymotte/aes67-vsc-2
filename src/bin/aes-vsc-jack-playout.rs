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

use aes67_vsc_2::{
    config::Config,
    error::{Aes67Vsc2Error, Aes67Vsc2Result},
    playout::jack::start_jack_playout,
    telemetry,
};
use miette::Result;
use std::{
    io::{self, BufRead},
    thread,
    time::Duration,
};
use tokio::{spawn, sync::mpsc};
use tokio_graceful_shutdown::{SubsystemBuilder, Toplevel};
use tracing::info;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let config = Config::load().await?;

    telemetry::init(&config).await?;

    info!(
        "Starting {} instance '{}'",
        config.app.name, config.app.instance.name
    );

    let (stdin_tx, mut stdin_rx) = mpsc::channel(1);

    Toplevel::new(|s| async move {
        s.start(SubsystemBuilder::new("aes67-vsc-2", |s| async move {
            let api = start_jack_playout(&s, config, false).await?;
            info!("Playout API running at {}", api.url());

            thread::spawn(move || {
                stdin_api(stdin_tx).ok();
            });

            spawn(async move {
                while let Some(line) = stdin_rx.recv().await {
                    match line.trim() {
                        "stop" => println!("{}", api.stop().await?),
                        _ => eprintln!("unknown command"),
                    }
                }
                Ok::<(), Aes67Vsc2Error>(())
            });

            Ok::<(), Aes67Vsc2Error>(())
        }));
    })
    .catch_signals()
    .handle_shutdown_requests(Duration::from_secs(1))
    .await?;

    Ok(())
}

fn stdin_api(stdin_tx: mpsc::Sender<String>) -> Aes67Vsc2Result<()> {
    for line in io::stdin().lock().lines() {
        if stdin_tx.blocking_send(line?).is_err() {
            break;
        }
    }

    Ok(())
}
