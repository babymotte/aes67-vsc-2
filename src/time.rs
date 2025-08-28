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

#[cfg(feature = "statime")]
mod statime;

use crate::{
    error::{SystemClockError, SystemClockResult},
    formats::AudioFormat,
};
use libc::{CLOCK_MONOTONIC, CLOCK_TAI, clock_gettime, clockid_t, timespec};

pub fn system_time() -> SystemClockResult<timespec> {
    system_time_for_clock_id(CLOCK_TAI)
}

pub fn system_time_monotonic() -> SystemClockResult<timespec> {
    system_time_for_clock_id(CLOCK_MONOTONIC)
}

fn system_time_for_clock_id(clock_id: clockid_t) -> SystemClockResult<timespec> {
    let mut tp = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { clock_gettime(clock_id, &mut tp) } == -1 {
        Err(SystemClockError("could not get system time".to_owned()))
    } else {
        Ok(tp)
    }
}

pub trait MediaClock: Clone + Send + 'static {
    fn current_media_time(&self) -> SystemClockResult<u64>;
    fn current_ptp_time_millis(&self) -> SystemClockResult<u64>;
}

#[derive(Clone)]
pub struct SystemMediaClock {
    audio_format: AudioFormat,
}

impl SystemMediaClock {
    pub fn new(audio_format: AudioFormat) -> Self {
        Self { audio_format }
    }
}

impl MediaClock for SystemMediaClock {
    fn current_media_time(&self) -> SystemClockResult<u64> {
        let ptp_time = system_time()?;
        Ok(media_time_from_ptp(
            ptp_time.tv_sec,
            ptp_time.tv_nsec,
            &self.audio_format,
        ))
    }

    fn current_ptp_time_millis(&self) -> SystemClockResult<u64> {
        let ptp_time = system_time()?;
        Ok(ptp_time.tv_sec as u64 * 1_000 + ptp_time.tv_nsec as u64 / 1_000_000)
    }
}

fn media_time_from_ptp(ptp_time_secs: i64, ptp_time_nanos: i64, audio_format: &AudioFormat) -> u64 {
    let ptp_nanos = (ptp_time_secs as i128) * 1_000_000_000 + ptp_time_nanos as i128;
    let total_frames = (ptp_nanos * audio_format.sample_rate as i128) / 1_000_000_000;
    total_frames as u64
}
