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

use aes67_rs::{
    formats::{AudioFormat, FrameFormat, SampleFormat},
    time::{ClockMode, ClockNic, MediaClock, get_primary_clock},
};
use miette::IntoDiagnostic;
use std::{env, io, thread, time::Duration};
use supports_color::Stream;
use tosub::{SubsystemHandle, SubsystemResult};
use tracing_subscriber::{
    EnvFilter, Layer, filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

#[tokio::main]
async fn main() -> SubsystemResult {
    tracing_subscriber::registry()
        .with(
            fmt::Layer::new()
                .with_ansi(supports_color::on(Stream::Stderr).is_some())
                .with_writer(io::stderr)
                .with_filter(EnvFilter::from_default_env())
                .with_filter(filter_fn(|meta| {
                    !meta.is_span() && meta.fields().iter().any(|f| f.name() == "message")
                })),
        )
        .init();

    tosub::build_root("phc-clock-demo")
        .catch_signals()
        .with_timeout(Duration::from_secs(1))
        .start(run)
        .await
}

async fn run(subsys: SubsystemHandle) -> miette::Result<()> {
    let Some(nic) = env::args().skip(1).next() else {
        return Err(miette::miette!(
            "Please provide a network interface as argument"
        ));
    };

    let audio_format = AudioFormat {
        frame_format: FrameFormat {
            channels: 2,
            sample_format: SampleFormat::L24,
        },
        sample_rate: 48_000,
    };

    let (wb, _, _) = worterbuch_client::connect_with_default_config()
        .await
        .expect("no wb connection");

    let mut phc_clock = get_primary_clock(
        "phc_clock".into(),
        Some(ClockMode::Phc {
            nic: ClockNic::NonRedundant(nic.to_owned()),
            subsys: &subsys,
        }),
        audio_format.sample_rate,
    )
    .into_diagnostic()?;

    let mut statime_clock = get_primary_clock(
        "statime_clock".into(),
        Some(ClockMode::Internal {
            nic: ClockNic::NonRedundant(nic.to_owned()),
            wb: wb.clone(),
        }),
        audio_format.sample_rate,
    )
    .into_diagnostic()?;

    loop {
        let phc_time_1 = phc_clock.current_time()?.media_time as f64;
        let statime_time = statime_clock.current_time()?.media_time as f64;
        let phc_time_2 = phc_clock.current_time()?.media_time as f64;
        let phc_time = (phc_time_1 + phc_time_2) / 2.0;

        eprintln!("Current phc media time: {}", phc_time);
        eprintln!("Current statime media time: {}", statime_time);
        eprintln!("Diff: {}", phc_time - statime_time as f64);

        thread::sleep(Duration::from_secs(1));
    }
}
