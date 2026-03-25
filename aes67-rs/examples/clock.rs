use aes67_rs::time::{MediaClock, UnixMediaClock};
use miette::IntoDiagnostic;
use std::time::Duration;
use thread_priority::{ThreadPriority, set_current_thread_priority};
use timerfd::{SetTimeFlags, TimerFd, TimerState};

pub fn main() -> miette::Result<()> {
    set_current_thread_priority(ThreadPriority::Max).into_diagnostic()?;

    let sr = 48_000;
    let ptime_micros = 125;
    let delay = Duration::from_micros(ptime_micros);

    let mut timer = TimerFd::new().into_diagnostic()?;
    timer.set_state(
        TimerState::Periodic {
            current: delay,
            interval: delay,
        },
        SetTimeFlags::Default,
    );

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(thing) = rx.recv() {
            println!("{thing}");
        }
    });

    let mut clock = UnixMediaClock::system_clock(sr as u32);

    let start = clock.current_media_time().into_diagnostic()?;
    let mut last_time = start;

    loop {
        timer.read();

        let mt = clock.current_media_time().into_diagnostic()?;
        let diff = mt - last_time;
        last_time = mt;
        tx.send(diff).ok();
    }
}
