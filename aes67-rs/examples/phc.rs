use aes67_rs::config::PtpMode;
use aes67_rs::error::ConfigResult;
use aes67_rs::formats::{AudioFormat, FrameFormat, SampleFormat};
use aes67_rs::time::get_clock;

fn main() -> ConfigResult<()> {
    let audio_format = AudioFormat {
        frame_format: FrameFormat {
            channels: 2,
            sample_format: SampleFormat::L24,
        },
        sample_rate: 48_000,
    };
    let clock = get_clock(
        Some(PtpMode::Phc {
            nic: "enp0s13f0u1".to_owned(),
        }),
        audio_format,
    )?;

    eprintln!("Current media time: {}", clock.current_media_time()?);

    Ok(())
}
