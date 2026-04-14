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
    config::PtpMode,
    error::ConfigResult,
    formats::{AudioFormat, FrameFormat, SampleFormat},
    time::{MediaClock, get_clock},
};
use std::{io, thread, time::Duration};
use supports_color::Stream;
use tracing_subscriber::{
    EnvFilter, Layer, filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

#[tokio::main]
async fn main() -> ConfigResult<()> {
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

    let nic = "enp0s13f0u3";

    let mut phc_clock = get_clock(
        "phc_clock".into(),
        Some(PtpMode::Phc {
            nic: nic.to_owned(),
        }),
        audio_format.sample_rate,
        wb.clone(),
    )
    .await?;

    let mut statime_clock = get_clock(
        "statime_clock".into(),
        Some(PtpMode::Internal {
            nic: nic.to_owned(),
        }),
        audio_format.sample_rate,
        wb.clone(),
    )
    .await?;

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
