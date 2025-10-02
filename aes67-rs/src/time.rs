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

#[cfg(feature = "statime")]
pub mod statime;

use crate::{
    config::PtpMode,
    error::{ClockError, ClockResult, ConfigResult},
    formats::{AudioFormat, Frames},
    nic::find_nic_with_name,
};
use clock_steering::{Clock, Timestamp, unix::UnixClock};
use libc::{clock_gettime, clockid_t, timespec};
use pnet::datalink::NetworkInterface;
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};
use tracing::{error, info};

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

pub trait MediaClock: Send + Sync + 'static {
    fn current_media_time(&self) -> ClockResult<Frames>;

    fn current_ptp_time_millis(&self) -> ClockResult<u64>;
}

pub struct UnixMediaClock {
    unix_clock: UnixClock,
    audio_format: AudioFormat,
}

impl UnixMediaClock {
    fn system_clock(audio_format: AudioFormat) -> Self {
        UnixMediaClock {
            unix_clock: UnixClock::CLOCK_TAI,
            audio_format,
        }
    }

    fn phc_clock(audio_format: AudioFormat, path: impl AsRef<Path>) -> ClockResult<Self> {
        Ok(UnixMediaClock {
            unix_clock: UnixClock::open(path)?,
            audio_format,
        })
    }
}

impl MediaClock for UnixMediaClock {
    fn current_media_time(&self) -> ClockResult<Frames> {
        let ptp_time = match self.unix_clock.now() {
            Ok(it) => it,
            Err(e) => return Err(ClockError::other(e)),
        };
        Ok(to_media_time(ptp_time, &self.audio_format))
    }

    fn current_ptp_time_millis(&self) -> ClockResult<u64> {
        let ptp_time = match self.unix_clock.now() {
            Ok(it) => it,
            Err(e) => return Err(ClockError::other(e)),
        };
        Ok(to_duration(ptp_time).as_millis() as u64)
    }
}

pub fn to_duration(ts: Timestamp) -> Duration {
    Duration::new(ts.seconds as u64, ts.nanos as u32)
}

pub fn to_system_time(ts: Timestamp) -> SystemTime {
    SystemTime::UNIX_EPOCH + to_duration(ts)
}

pub fn to_media_time(ptp_time: Timestamp, audio_format: &AudioFormat) -> u64 {
    let ptp_nanos = to_duration(ptp_time).as_nanos();
    let total_frames = (ptp_nanos * audio_format.sample_rate as u128) / NANOS_PER_SEC;
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

pub fn get_clock(
    ptp_mode: Option<PtpMode>,
    audio_format: AudioFormat,
) -> ConfigResult<Box<dyn MediaClock>> {
    match ptp_mode {
        Some(PtpMode::System) => return Ok(Box::new(UnixMediaClock::system_clock(audio_format))),
        Some(PtpMode::Phc { nic }) => {
            let iface = find_nic_with_name(&nic)?;
            info!("Creating new PHC clock …");
            let Some(path) = phc_device_for_interface_ethtool(&iface)? else {
                return Err(ClockError::PtpNotSupported(iface.name.clone()).into());
            };
            return Ok(Box::new(UnixMediaClock::phc_clock(audio_format, path)?));
        }
        None => return Ok(Box::new(UnixMediaClock::system_clock(audio_format))),
    }
}

/// Returns Some("/dev/ptpX") if the interface has a PHC, None if not
pub fn phc_device_for_interface_ethtool(iface: &NetworkInterface) -> io::Result<Option<PathBuf>> {
    let output = Command::new("ethtool")
        .arg("-T")
        .arg(&iface.name)
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("ethtool failed for {}", iface.name),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(idx_str) = line.strip_prefix("Hardware timestamp provider index:") {
            let idx_str = idx_str.trim();
            if let Ok(idx) = idx_str.parse::<u32>() {
                return Ok(Some(PathBuf::from(format!("/dev/ptp{}", idx))));
            }
        }
    }

    // No PHC index found
    Ok(None)
}
