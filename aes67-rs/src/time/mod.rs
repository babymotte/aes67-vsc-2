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

//! Available clock sources:
//! - system time: used when the system clock is already synced to a PTP master
//! - passive: used when PTP is not available or required. Time is derived from RTP packet timestamps
//! - phc: used with PTP compatible network interfaces in combination with a ptp daemon like ptp4l
//! - statime: used when PTP is required but there is no external synchronization. Only possible if PTP port is not already used

mod phc;
#[cfg(feature = "statime")]
mod statime;

use crate::{
    config::PtpMode,
    error::{ClockError, ClockResult, ConfigResult},
    formats::{AudioFormat, Frames, FramesPerSecond},
    nic::{find_nic_with_name, phc_device_for_interface_ethtool},
    time::{phc::PhcClock, statime::StatimePtpMediaClock},
};
use clock_steering::{Clock as CSClock, Timestamp, unix::UnixClock};
use libc::{clock_gettime, clockid_t, timespec};
use std::{
    io,
    time::{Duration, Instant, SystemTime},
};
use tracing::{error, info, warn};
#[cfg(feature = "statime")]
use worterbuch_client::Worterbuch;

pub const NANOS_PER_SEC: u128 = 1_000_000_000;
pub const NANOS_PER_MILLI: u64 = 1_000_000;
pub const MICROS_PER_MILLI: u64 = 1_000;
pub const NANOS_PER_MICRO: u64 = 1_000;
pub const MILLIS_PER_SEC: u64 = 1_000;
pub const MICROS_PER_SEC: u64 = 1_000_000;

pub const NANOS_PER_SEC_F: f64 = 1_000_000_000.0;
pub const NANOS_PER_MILLI_F: f64 = 1_000_000.0;
pub const MICROS_PER_MILLI_F: f32 = 1_000.0;
pub const NANOS_PER_MICRO_F: f64 = 1_000.0;
pub const MILLIS_PER_SEC_F: f32 = 1_000.0;
pub const MICROS_PER_SEC_F: f64 = 1_000_000.0;

pub trait MediaClock: Clone + Send + 'static {
    fn current_media_time(&mut self) -> ClockResult<Frames>;

    fn current_ptp_time_millis(&mut self) -> ClockResult<u64>;
}

#[derive(Clone)]
pub enum Clock {
    System(UnixMediaClock),
    Phc(PhcClock),
    #[cfg(feature = "statime")]
    Statime(StatimePtpMediaClock),
}

impl MediaClock for Clock {
    fn current_media_time(&mut self) -> ClockResult<Frames> {
        match self {
            Clock::System(clock) => clock.current_media_time(),
            Clock::Phc(clock) => clock.current_media_time(),
            #[cfg(feature = "statime")]
            Clock::Statime(clock) => clock.current_media_time(),
        }
    }

    fn current_ptp_time_millis(&mut self) -> ClockResult<u64> {
        match self {
            Clock::System(clock) => clock.current_ptp_time_millis(),
            Clock::Phc(clock) => clock.current_ptp_time_millis(),
            #[cfg(feature = "statime")]
            Clock::Statime(clock) => clock.current_ptp_time_millis(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnixMediaClock {
    unix_clock: UnixClock,
    sample_rate: FramesPerSecond,
}

impl UnixMediaClock {
    fn system_clock(sample_rate: FramesPerSecond) -> Self {
        UnixMediaClock {
            unix_clock: UnixClock::CLOCK_TAI,
            sample_rate,
        }
    }
}

impl MediaClock for UnixMediaClock {
    fn current_media_time(&mut self) -> ClockResult<Frames> {
        let start = Instant::now();
        let now = self.unix_clock.now();
        let end = Instant::now();

        let time = (end - start).as_micros();
        if time > 500 {
            warn!("Getting time took {time} µs",);
        }

        let ptp_time = match now {
            Ok(it) => it,
            Err(e) => return Err(ClockError::other(e)),
        };
        Ok(to_media_time(ptp_time, self.sample_rate))
    }

    fn current_ptp_time_millis(&mut self) -> ClockResult<u64> {
        let ptp_time = match self.unix_clock.now() {
            Ok(it) => it,
            Err(e) => return Err(ClockError::other(e)),
        };
        Ok(timestamp_to_duration(ptp_time).as_millis() as u64)
    }
}

pub fn timestamp_to_duration(ts: Timestamp) -> Duration {
    Duration::new(ts.seconds as u64, ts.nanos)
}

pub fn timespec_to_duration(tp: timespec) -> Duration {
    Duration::new(tp.tv_sec as u64, tp.tv_nsec as u32)
}

pub fn to_system_time(ts: Timestamp) -> SystemTime {
    SystemTime::UNIX_EPOCH + timestamp_to_duration(ts)
}

pub fn to_media_time(ptp_time: Timestamp, sample_rate: FramesPerSecond) -> u64 {
    let ptp_nanos = timestamp_to_duration(ptp_time).as_nanos();
    let total_frames = (ptp_nanos * sample_rate as u128) / NANOS_PER_SEC;
    total_frames as u64
}

pub fn get_time(clock_id: clockid_t) -> io::Result<timespec> {
    let mut tp = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { clock_gettime(clock_id, &mut tp) } != 0 {
        let e = io::Error::last_os_error();
        error!("Could not get current time of clock {clock_id}: {e}");
        Err(e)
    } else {
        Ok(tp)
    }
}

pub async fn get_clock(
    app_name: String,
    ptp_mode: Option<PtpMode>,
    sample_rate: FramesPerSecond,
    wb: Worterbuch,
) -> ConfigResult<Clock> {
    match ptp_mode {
        Some(PtpMode::System) => create_system_clock(sample_rate).map(Clock::System),
        Some(PtpMode::Phc { nic }) => create_phc_clock(sample_rate, nic).map(Clock::Phc),
        #[cfg(feature = "statime")]
        Some(PtpMode::Internal { nic }) => create_statime_clock(app_name, sample_rate, wb, nic)
            .await
            .map(Clock::Statime),
        None => create_system_clock(sample_rate).map(Clock::System),
    }
}

fn create_system_clock(sample_rate: FramesPerSecond) -> ConfigResult<UnixMediaClock> {
    info!("Creating new system clock …");
    let clock = UnixMediaClock::system_clock(sample_rate);
    Ok(clock)
}

fn create_phc_clock(sample_rate: FramesPerSecond, nic: String) -> ConfigResult<PhcClock> {
    info!("Creating new PHC clock for on NIC {nic} …");
    let iface = find_nic_with_name(&nic)?;
    let Some(path) = phc_device_for_interface_ethtool(&iface)? else {
        return Err(ClockError::PtpNotSupported(iface.name.clone()).into());
    };
    let clock = PhcClock::open(path, sample_rate)?;
    Ok(clock)
}

#[cfg(feature = "statime")]
async fn create_statime_clock(
    app_name: String,
    sample_rate: FramesPerSecond,
    wb: Worterbuch,
    nic: String,
) -> ConfigResult<StatimePtpMediaClock> {
    info!("Creating new statime clock on NIC {nic} …");
    let iface = find_nic_with_name(&nic)?;
    let clock = StatimePtpMediaClock::new(app_name, iface, sample_rate, wb).await?;
    Ok(clock)
}

pub fn to_nanos(tp: timespec) -> i128 {
    tp.tv_sec as i128 * NANOS_PER_SEC as i128 + tp.tv_nsec as i128
}
