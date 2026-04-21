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

use aes67_rs::{
    error::ClockResult,
    formats::{Frames, FramesPerSecond},
    time::{Clock, MediaClock},
    utils::AverageCalculationBuffer,
};
use jack::{Client, ProcessScope};
#[cfg(debug_assertions)]
use tracing::{debug, info, warn};

pub struct JackClock {
    ptp_clock: Clock,
    jack_clock_offset: Option<i64>,
    drift_buffer: AverageCalculationBuffer<i64>,
    slew: i64,
}

pub enum ClockOffset {
    Stable { offset: i64, compensation: i64 },
    Unstable,
}

pub enum ClockState {
    Stable {
        current_time: Frames,
        compensation: i64,
    },
    Unstable,
}

impl JackClock {
    pub fn new(ptp_clock: Clock) -> Self {
        JackClock {
            ptp_clock,
            jack_clock_offset: None,
            drift_buffer: AverageCalculationBuffer::new([0i64; 100].into()),
            slew: 0,
        }
    }

    pub fn update_clock(
        &mut self,
        client: &Client,
        ps: &ProcessScope,
        continuously_compensate_drift: bool,
    ) -> ClockResult<ClockState> {
        let drift_buf_len =
            (client.sample_rate() as usize / 48_000) * (10_000 / ps.n_frames() as usize);
        if self.drift_buffer.len() != drift_buf_len {
            info!("Updating drift buffer length to {drift_buf_len}");
            self.drift_buffer = AverageCalculationBuffer::new(vec![0i64; drift_buf_len].into());
        }

        let state = match self.jack_clock_offset {
            Some(offset) => self.compensate_drift(offset, ps, continuously_compensate_drift)?,
            None => self.init_clock(ps)?,
        };

        Ok(match state {
            ClockOffset::Stable {
                offset,
                compensation,
            } => {
                ClockState::Stable {
                    // TODO this might wrap, wrap needs to be detected and handled!
                    current_time: (ps.last_frame_time() as i64 + offset) as Frames,
                    compensation,
                }
            }
            ClockOffset::Unstable => ClockState::Unstable,
        })
    }

    fn init_clock(&mut self, ps: &ProcessScope) -> ClockResult<ClockOffset> {
        let t1 = ps.frames_since_cycle_start();
        let ptp_time = self.ptp_clock.current_time()?.media_time as i64;
        let t3 = ps.frames_since_cycle_start();
        let jack_time = (ps.last_frame_time() + (t1 + t3) / 2) as i64;

        let diff = ptp_time - jack_time;

        self.jack_clock_offset = Some(diff);

        Ok(ClockOffset::Unstable)
    }

    fn compensate_drift(
        &mut self,
        offset: i64,
        ps: &ProcessScope,
        continuously_compensate_drift: bool,
    ) -> ClockResult<ClockOffset> {
        let t1 = ps.frames_since_cycle_start();
        let ptp_time = self.ptp_clock.current_time()?.media_time as i64;
        let t3 = ps.frames_since_cycle_start();
        let jack_time = (ps.last_frame_time() + (t1 + t3) / 2) as i64;

        let jack_ptpt_time = jack_time + offset;

        let diff = ptp_time - jack_time;
        let drift = ptp_time - jack_ptpt_time;

        let drift_abs = drift.abs();
        if drift_abs > ps.n_frames() as i64 {
            #[cfg(debug_assertions)]
            warn!("JACK clock is off by {drift} frames, resetting JACK clock.");
            self.jack_clock_offset = Some(diff);
            return Ok(ClockOffset::Unstable);
        }

        let slew = if let Some(drift) = self.drift_buffer.update(drift) {
            let slew = if continuously_compensate_drift {
                drift.signum()
            } else {
                0
            };

            #[cfg(debug_assertions)]
            if drift != 0 {
                if drift > 0 {
                    debug!(
                        "JACK clock is {} frames BEHIND; slewing jack clock by {}",
                        drift, slew
                    );
                } else {
                    debug!(
                        "JACK clock is {} frames AHEAD; slewing jack clock by {}",
                        -drift, slew
                    );
                }
            }

            slew
        } else {
            0
        };

        let updated_offset = offset + slew;

        self.jack_clock_offset = Some(updated_offset);

        Ok(ClockOffset::Stable {
            offset: updated_offset,
            compensation: slew,
        })
    }
}
