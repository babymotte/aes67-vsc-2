use aes67_vsc_2::{
    formats::{AudioFormat, FrameFormat, SampleFormat},
    time::{MediaClock, SystemMediaClock},
};

fn main() {
    let clock = SystemMediaClock::new(AudioFormat {
        frame_format: FrameFormat {
            channels: 2,
            sample_format: SampleFormat::L24,
        },
        sample_rate: 48_000,
    });
    let mut last_media_time = None;

    loop {
        let now = clock.current_media_time().unwrap();
        if let Some(last) = last_media_time {
            eprintln!("now - last = {}", now - last);
        }
        last_media_time = Some(now);
    }
}
