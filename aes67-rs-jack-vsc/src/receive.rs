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
    common::{ClockState, JackClock},
    session_manager::{SessionManagerNotificationHandler, start_session_manager},
};
use aes67_rs::{
    buffer::receiver::ReadResult,
    formats::{Frames, frames_to_duration},
    monitoring::Monitoring,
    receiver::{api::ReceiverApi, config::ReceiverConfig},
    time::Clock,
};
use jack::{
    AudioOut, Client, ClientOptions, Control, Port, ProcessScope, contrib::ClosureProcessHandler,
};
use miette::IntoDiagnostic;
use std::{thread, time::Instant};
use tokio::sync::mpsc;
use tosub::SubsystemHandle;
#[cfg(debug_assertions)]
use tracing::{error, info};

struct State {
    ports: Vec<Port<AudioOut>>,
    receiver: ReceiverApi,
    clock: JackClock,
    config: ReceiverConfig,
    muted: bool,
    monitoring: Monitoring,
    subsys: SubsystemHandle,
}

impl State {}

pub async fn start_playout(
    app_id: String,
    subsys: SubsystemHandle,
    receiver: ReceiverApi,
    config: ReceiverConfig,
    clock: Clock,
    monitoring: Monitoring,
) -> miette::Result<SubsystemHandle> {
    // TODO evaluate client status
    let (client, _status) =
        Client::new(&config.label, ClientOptions::default()).into_diagnostic()?;

    #[cfg(debug_assertions)]
    info!(
        "JACK client '{}' created with status {:?}",
        config.label, _status
    );

    let mut ports = vec![];

    for l in config.channel_labels.clone().iter() {
        let label = l.to_owned();
        ports.push(
            client
                .register_port(&label, AudioOut::default())
                .into_diagnostic()?,
        );
    }

    let (tx, notifications) = mpsc::channel(1024);
    let cid = config.label.clone();
    let notification_handler = SessionManagerNotificationHandler {
        client_id: cid.clone(),
        tx,
    };
    let process_handler_state = State {
        ports,
        receiver,
        clock: JackClock::new(clock),
        config: config.clone(),
        muted: false,
        monitoring,
        subsys: subsys.clone(),
    };
    let process_handler =
        ClosureProcessHandler::with_state(process_handler_state, process, buffer_change);

    let active_client = client
        .activate_async(notification_handler, process_handler)
        .into_diagnostic()?;

    let session_manager = start_session_manager(
        &subsys,
        active_client,
        notifications,
        app_id,
        format!("rx/{}", config.id),
    );

    Ok(session_manager)
}

fn buffer_change(_: &mut State, _client: &Client, _buffer_len: jack::Frames) -> Control {
    #[cfg(debug_assertions)]
    {
        use aes67_rs::time::MILLIS_PER_SEC_F;
        let buffer_ms = _buffer_len as f32 * MILLIS_PER_SEC_F / _client.sample_rate() as f32;
        info!("JACK buffer size changed to {_buffer_len} frames / {buffer_ms:.1} ms");
    }
    Control::Continue
}

fn process(state: &mut State, client: &Client, ps: &ProcessScope) -> Control {
    // Check for shutdown early to avoid accessing resources during teardown
    // and prevent logging races that can cause RefCell panics
    if state.subsys.is_shut_down() {
        muted(state, ps);
        return Control::Quit;
    }

    let start = Instant::now();

    let playout_time = match state.clock.update_clock(client, ps, true) {
        Ok(ClockState::Stable { current_time, .. }) => current_time,
        Ok(ClockState::Unstable) => {
            muted(state, ps);
            return Control::Continue;
        }
        Err(_e) => {
            #[cfg(debug_assertions)]
            error!("Could not get current media time: {_e}");
            return Control::Quit;
        }
    };

    let link_offset_frames = state.config.frames_in_link_offset();
    let ingress_time = playout_time - link_offset_frames;

    let pre_req = Instant::now();

    loop {
        let buffers = state.ports.iter_mut().map(|p| Some(p.as_mut_slice(ps)));

        match state
            .receiver
            .receive(buffers, ingress_time, ps.n_frames() as usize)
        {
            Ok(ReadResult::Ok(_)) => {
                unmuted(state);
                break;
            }
            Ok(ReadResult::NotReady(missing)) => {
                // clock is likely not synced yet
                if missing > ps.n_frames() as usize {
                    muted(state, ps);
                    break;
                }

                // yield thread and re-try

                thread::sleep(frames_to_duration(
                    missing as Frames / 10,
                    state.config.audio_format.sample_rate,
                ));
                continue;
            }
            Ok(ReadResult::TooLate) => {
                muted(state, ps);
                break;
            }
            Err(_) => {
                if !state.muted {
                    #[cfg(debug_assertions)]
                    error!("Receiving audio data timed out.");
                }
                muted(state, ps);
                break;
            }
        }
    }

    let post_req = Instant::now();

    // TODO send timing to monitoring

    let _total = post_req.duration_since(start).as_micros();
    let _req = post_req.duration_since(pre_req).as_micros();

    // if total > 100 {
    //     eprintln!("latency playout req: {req} µs");
    //     eprintln!("latency playout total: {total} µs");
    // }

    Control::Continue
}

fn muted(state: &mut State, ps: &ProcessScope) {
    if !state.muted {
        state.muted = true;
        state.report_muted(true);
    }

    for port in state.ports.iter_mut() {
        let buf = port.as_mut_slice(ps);
        buf.fill(0.0);
    }
}

fn unmuted(state: &mut State) {
    if state.muted {
        state.muted = false;
        state.report_muted(false);
    }
}

mod monitoring {
    use aes67_rs::monitoring::RxStats;

    use super::*;

    impl State {
        pub fn report_muted(&self, muted: bool) {
            self.monitoring.receiver_stats(RxStats::Muted(muted));
        }
    }
}
