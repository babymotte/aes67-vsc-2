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

mod statime_linux;
use crate::{
    error::{ClockResult, ConfigError, ConfigResult},
    formats::FramesPerSecond,
    time::{MediaClock, timestamp_to_duration, to_media_time},
};
use clock_steering::Timestamp;
use pnet::datalink::NetworkInterface;
use statime::Clock;
pub use statime_linux::*;
use worterbuch_client::Worterbuch;

#[derive(Clone)]
pub struct StatimePtpMediaClock {
    sample_rate: FramesPerSecond,
    statime_ptp_clock: StatimeClock,
}

impl StatimePtpMediaClock {
    pub async fn new(
        root_key: String,
        iface: NetworkInterface,
        sample_rate: FramesPerSecond,
        wb: Worterbuch,
    ) -> ConfigResult<Self> {
        let ip = iface
            .ips
            .first()
            .ok_or_else(|| ConfigError::NoIPAddressForNIC(iface.name.clone()))?
            .ip();
        let statime_ptp_clock = statime_linux(iface, ip, wb, root_key).await;
        Ok(StatimePtpMediaClock {
            sample_rate,
            statime_ptp_clock,
        })
    }
}

impl MediaClock for StatimePtpMediaClock {
    fn current_media_time(&mut self) -> ClockResult<u64> {
        let ptp_time = self.statime_ptp_clock.now();
        let ptp_time = Timestamp {
            seconds: ptp_time.secs() as i64,
            nanos: ptp_time.subsec_nanos(),
        };
        Ok(to_media_time(ptp_time, self.sample_rate))
    }

    fn current_ptp_time_millis(&mut self) -> ClockResult<u64> {
        let ptp_time = self.statime_ptp_clock.now();
        let ptp_time = Timestamp {
            seconds: ptp_time.secs() as i64,
            nanos: ptp_time.subsec_nanos(),
        };
        Ok(timestamp_to_duration(ptp_time).as_millis() as u64)
    }
}
