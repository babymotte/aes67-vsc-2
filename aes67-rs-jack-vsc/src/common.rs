use aes67_rs::{error::ClockResult, formats::Frames, time::MediaClock};
use jack::ProcessScope;

pub struct JackClock {
    ptp_clock: Box<dyn MediaClock>,
    jack_clock_offset: Option<i64>,
}

impl JackClock {
    pub fn new(ptp_clock: Box<dyn MediaClock>) -> Self {
        JackClock {
            ptp_clock,
            jack_clock_offset: None,
        }
    }

    pub fn update_clock(&mut self, ps: &ProcessScope) -> ClockResult<Frames> {
        let offset = match self.jack_clock_offset {
            Some(it) => it,
            None => self.init_clock(ps)?,
        };

        // TODO detect and compensate drift

        // TODO this might wrap, wrap needs to be detected and handled!
        Ok((ps.last_frame_time() as i64 + offset) as Frames)
    }

    fn init_clock(&mut self, ps: &ProcessScope) -> ClockResult<i64> {
        let t1 = ps.frames_since_cycle_start();
        let ptp_time = self.ptp_clock.current_media_time()? as i64;
        let t3 = ps.frames_since_cycle_start();
        let jack_time = (ps.last_frame_time() + (t1 + t3) / 2) as i64;

        let diff = ptp_time - jack_time;

        self.jack_clock_offset = Some(diff);

        Ok(diff)
    }
}
