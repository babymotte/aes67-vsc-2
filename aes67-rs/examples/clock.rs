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
    time::{MediaClock, UnixMediaClock},
    utils::set_realtime_priority,
};
use miette::IntoDiagnostic;
use std::time::Duration;
use timerfd::{SetTimeFlags, TimerFd, TimerState};

pub fn main() -> miette::Result<()> {
    set_realtime_priority();

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

    let start = clock.current_time().into_diagnostic()?.media_time;
    let mut last_time = start;

    loop {
        timer.read();

        let mt = clock.current_time().into_diagnostic()?.media_time;
        let diff = mt - last_time;
        last_time = mt;
        tx.send(diff).ok();
    }
}
