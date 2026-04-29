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

use crate::{
    error::{ClockCreationError, ClockCreationResult, ClockError, ClockResult},
    formats::FramesPerSecond,
    time::{
        MediaClock, PtpTimestamp, SystemTimestamp, Time, Timestamp, get_time, to_media_time,
        to_nanos,
    },
};
use libc::{CLOCK_TAI, clockid_t};
use std::{
    os::fd::{IntoRawFd, RawFd},
    path::Path,
    sync::{LazyLock, OnceLock, atomic::Ordering},
    time::Duration,
};
use std::{sync::atomic::AtomicI64, time::Instant};
use tokio::{select, time::MissedTickBehavior};
use tosub::{SubsystemError, SubsystemHandle};
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct PhcClock {
    sample_rate: FramesPerSecond,
}

type CLockId = (clockid_t, RawFd);

static LAST_OFFSET: LazyLock<AtomicI64> = LazyLock::new(|| AtomicI64::new(0));
static CLOCK_ID: OnceLock<ClockCreationResult<CLockId>> = OnceLock::new();

impl PhcClock {
    pub fn open(
        subsys: &SubsystemHandle,
        path: impl AsRef<Path>,
        sample_rate: FramesPerSecond,
    ) -> ClockCreationResult<Self> {
        let (clock, _) = CLOCK_ID
            .get_or_init(|| init_phc_clock(subsys, path.as_ref()))
            .to_owned()?;

        let offset = get_current_offset(clock).map_err(|e| {
            ClockCreationError::GetTime(e.to_string(), path.as_ref().display().to_string())
        })?;

        LAST_OFFSET.store(offset, Ordering::Release);

        Ok(Self { sample_rate })
    }

    fn now(&mut self) -> ClockResult<(SystemTimestamp, PtpTimestamp)> {
        let tp = get_time(CLOCK_TAI)?;

        let offset = LAST_OFFSET.load(Ordering::Acquire);

        let compensated = Duration::from_nanos((to_nanos(tp) + offset as i128) as u64);

        let system_timestamp = Timestamp {
            seconds: tp.tv_sec as u64,
            nanos: tp.tv_nsec as u32,
        };
        let ptp_timestamp = Timestamp {
            seconds: compensated.as_secs(),
            nanos: compensated.subsec_nanos(),
        };

        Ok((system_timestamp, ptp_timestamp))
    }
}

fn init_phc_clock(
    subsys: &SubsystemHandle,
    path: impl AsRef<Path>,
) -> ClockCreationResult<CLockId> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .read(true)
        .open(path.as_ref())
        .map_err(|e| {
            ClockCreationError::Open(path.as_ref().display().to_string(), e.to_string())
        })?;
    let fd = file.into_raw_fd();
    let clock_id = ((!(fd as libc::clockid_t)) << 3) | 3;
    let clock = (clock_id, fd);

    info!("Starting PHC clock sync task …");
    subsys.spawn(
        format!("phc-clock-sync-{}", path.as_ref().display()),
        move |s| async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                let offset = match get_current_offset(clock_id) {
                    Ok(value) => value,
                    Err(e) => {
                        error!("Failed to get PHC offset: {e}");
                        continue;
                    }
                };

                LAST_OFFSET.store(offset, Ordering::Release);

                select! {
                    _ = s.shutdown_requested() => break,
                    _ = interval.tick() => continue,
                }
            }

            Ok::<(), SubsystemError>(())
        },
    );

    Ok(clock)
}

fn get_current_offset(clock: i32) -> ClockResult<i64> {
    let tai1 = get_time(CLOCK_TAI)?;
    let phc = get_time(clock)?;
    let tai2 = get_time(CLOCK_TAI)?;

    let tai1_nanos = to_nanos(tai1);
    let phc_nanos = to_nanos(phc);
    let tai2_nanos = to_nanos(tai2);

    let offset = (phc_nanos - (tai1_nanos + tai2_nanos) / 2) as i64;

    Ok(offset)
}

impl MediaClock for PhcClock {
    fn current_time(&mut self) -> ClockResult<Time> {
        #[cfg(debug_assertions)]
        let start = Instant::now();

        let now = self.now();

        #[cfg(debug_assertions)]
        let end = Instant::now();

        #[cfg(debug_assertions)]
        {
            let time = (end - start).as_micros();
            if time > 500 {
                warn!("Getting time took {time} µs",);
            }
        }

        let (system_time, ptp_time) = match now {
            Ok(it) => it,
            Err(e) => return Err(ClockError::other(e)),
        };

        let media_time = to_media_time(ptp_time, self.sample_rate);

        Ok(Time {
            media_time,
            ptp_time,
            system_time,
        })
    }
}
