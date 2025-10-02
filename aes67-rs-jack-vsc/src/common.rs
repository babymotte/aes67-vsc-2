use aes67_rs::{
    error::ClockResult, formats::Frames, time::MediaClock, utils::AverageCalculationBuffer,
};
use jack::ProcessScope;
use tracing::{debug, warn};

pub struct JackClock {
    ptp_clock: Box<dyn MediaClock>,
    jack_clock_offset: Option<i64>,
    drift_buffer: AverageCalculationBuffer<i64>,
    slew: i64,
}

pub enum ClockOffset {
    Stable(i64),
    Unstable,
}

pub enum ClockState {
    Stable(Frames),
    Unstable,
}

impl JackClock {
    pub fn new(ptp_clock: Box<dyn MediaClock>) -> Self {
        JackClock {
            ptp_clock,
            jack_clock_offset: None,
            drift_buffer: AverageCalculationBuffer::new([0i64; 1024].into()),
            slew: 0,
        }
    }

    pub fn update_clock(&mut self, ps: &ProcessScope) -> ClockResult<ClockState> {
        let state = match self.jack_clock_offset {
            Some(offset) => self.compensate_drift(offset, ps)?,
            None => self.init_clock(ps)?,
        };

        Ok(match state {
            ClockOffset::Stable(offset) => {
                // TODO this might wrap, wrap needs to be detected and handled!
                ClockState::Stable((ps.last_frame_time() as i64 + offset) as Frames)
            }
            ClockOffset::Unstable => ClockState::Unstable,
        })
    }

    fn init_clock(&mut self, ps: &ProcessScope) -> ClockResult<ClockOffset> {
        let t1 = ps.frames_since_cycle_start();
        let ptp_time = self.ptp_clock.current_media_time()? as i64;
        let t3 = ps.frames_since_cycle_start();
        let jack_time = (ps.last_frame_time() + (t1 + t3) / 2) as i64;

        let diff = ptp_time - jack_time;

        self.jack_clock_offset = Some(diff);

        Ok(ClockOffset::Unstable)
    }

    fn compensate_drift(&mut self, offset: i64, ps: &ProcessScope) -> ClockResult<ClockOffset> {
        let t1 = ps.frames_since_cycle_start();
        let ptp_time = self.ptp_clock.current_media_time()? as i64;
        let t3 = ps.frames_since_cycle_start();
        let jack_time = (ps.last_frame_time() + (t1 + t3) / 2) as i64;

        let diff = ptp_time - jack_time;
        let drift = diff - offset;

        let drift_abs = drift.abs();
        if drift_abs > ps.n_frames() as i64 {
            warn!("JACK clock is off by {drift} frames, resetting JACK clock.");
            self.jack_clock_offset = Some(diff);
            return Ok(ClockOffset::Unstable);
        }

        if let Some(drift) = self.drift_buffer.update(drift) {
            self.slew -= drift;
        }

        let signum = self.slew.signum();

        if signum != 0 {
            debug!(
                "JACK clock drift: {}; slewing jack clock by {}",
                -self.slew, signum
            );
        }

        self.slew -= signum;
        let updated_offset = offset - signum;

        self.jack_clock_offset = Some(updated_offset);

        Ok(ClockOffset::Stable(updated_offset))
    }
}
