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
        Ok(ptp_time.secs() * 1_000 + ptp_time.subsec_nanos() as u64 / 1_000_000)
    }
}
