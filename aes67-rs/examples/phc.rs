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
        "test-clock".into(),
        Some(PtpMode::Phc {
            nic: nic.to_owned(),
        }),
        audio_format.sample_rate,
        wb.clone(),
    )
    .await?;

    let mut statime_clock = get_clock(
        "test-clock".into(),
        Some(PtpMode::Internal {
            nic: nic.to_owned(),
        }),
        audio_format.sample_rate,
        wb.clone(),
    )
    .await?;

    loop {
        let phc_time_1 = phc_clock.current_media_time()?;
        let statime_time = statime_clock.current_media_time()?;
        let phc_time_2 = phc_clock.current_media_time()?;
        let phc_time = (phc_time_1 + phc_time_2) / 2;

        eprintln!("Current phc media time: {}", phc_time);
        eprintln!("Current statime media time: {}", statime_time);
        eprintln!("Diff: {}", phc_time as i64 - statime_time as i64);

        thread::sleep(Duration::from_secs(1));
    }
}
