use crate::{
    error::{ClockError, ClockResult},
    formats::AudioFormat,
    time::{MediaClock, get_time, timestamp_to_duration, to_media_time, to_nanos},
};
use clock_steering::Timestamp;
use lazy_static::lazy_static;
use libc::{CLOCK_TAI, clockid_t};
use std::{
    os::fd::{IntoRawFd, RawFd},
    path::Path,
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::Duration,
};
use std::{sync::atomic::AtomicI64, time::Instant};
use tracing::{info, warn};

pub struct PhcClock {
    audio_format: AudioFormat,
}

lazy_static! {
    static ref LAST_OFFSET: AtomicI64 = AtomicI64::new(0);
    static ref CLOCK_ID: Arc<Mutex<Option<(clockid_t, RawFd)>>> = Arc::new(Mutex::new(None));
}

impl PhcClock {
    pub fn open(path: impl AsRef<Path>, audio_format: AudioFormat) -> ClockResult<Self> {
        let clock = {
            let mut guard = CLOCK_ID.lock().expect("mutex poisoned");
            if let Some((clock_id, _)) = guard.as_ref() {
                *clock_id
            } else {
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .read(true)
                    .open(path)?;
                let fd = file.into_raw_fd();
                let clock = ((!(fd as libc::clockid_t)) << 3) | 3;
                *guard = Some((clock, fd));

                info!("Starting PHC clock sync thread …");
                thread::spawn(move || {
                    loop {
                        let offset = match get_current_offset(clock) {
                            Ok(value) => value,
                            _ => return,
                        };

                        LAST_OFFSET.store(offset, Ordering::Release);

                        thread::sleep(Duration::from_secs(1));
                    }
                });

                clock
            }
        };

        let offset = get_current_offset(clock)?;

        LAST_OFFSET.store(offset, Ordering::Release);

        Ok(Self { audio_format })
    }

    fn now(&mut self) -> ClockResult<Timestamp> {
        let tp = get_time(CLOCK_TAI)?;

        let offset = LAST_OFFSET.load(Ordering::Acquire);

        let compensated = Duration::from_nanos((to_nanos(tp) + offset as i128) as u64);

        Ok(Timestamp {
            seconds: compensated.as_secs() as i64,
            nanos: compensated.subsec_nanos(),
        })
    }
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
    fn current_media_time(&mut self) -> ClockResult<crate::formats::Frames> {
        let start = Instant::now();
        let now = self.now();
        let end = Instant::now();

        let time = (end - start).as_micros();
        if time > 500 {
            warn!("Getting time took {time} µs",);
        }

        let ptp_time = match now {
            Ok(it) => it,
            Err(e) => return Err(ClockError::other(e)),
        };
        Ok(to_media_time(ptp_time, &self.audio_format))
    }

    fn current_ptp_time_millis(&mut self) -> ClockResult<u64> {
        let ptp_time = match self.now() {
            Ok(it) => it,
            Err(e) => return Err(ClockError::other(e)),
        };
        Ok(timestamp_to_duration(ptp_time).as_millis() as u64)
    }
}
