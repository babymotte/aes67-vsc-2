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
    config::Config,
    error::{ConfigResult, SystemClockResult},
    formats::AudioFormat,
    time::{MediaClock, media_time_from_ptp},
    utils::find_network_interface,
};
use statime::Clock;
pub use statime_linux::*;
use worterbuch_client::Worterbuch;

#[derive(Clone)]
pub struct StatimePtpMediaClock {
    audio_format: AudioFormat,
    statime_ptp_clock: PtpClock,
}

impl StatimePtpMediaClock {
    pub async fn new(
        config: &Config,
        audio_format: AudioFormat,
        wb: Worterbuch,
    ) -> ConfigResult<Self> {
        let iface = find_network_interface(config.interface_ip)?;
        let ip = config.interface_ip;
        let root_key = config.instance_name();
        let statime_ptp_clock = statime_linux(iface, ip, wb, root_key).await;
        Ok(StatimePtpMediaClock {
            audio_format,
            statime_ptp_clock,
        })
    }
}

impl MediaClock for StatimePtpMediaClock {
    fn current_media_time(&self) -> SystemClockResult<u64> {
        let ptp_time = self.statime_ptp_clock.now();
        Ok(media_time_from_ptp(
            ptp_time.secs() as i64,
            ptp_time.subsec_nanos() as i64,
            &self.audio_format,
        ))
    }

    fn current_ptp_time_millis(&self) -> SystemClockResult<u64> {
        let ptp_time = self.statime_ptp_clock.now();
        Ok(to_duration(ptp_time).as_millis() as u64)
    }
}
