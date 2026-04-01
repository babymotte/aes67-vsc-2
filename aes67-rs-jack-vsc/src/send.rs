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
    monitoring::Monitoring,
    sender::{api::SenderApi, config::SenderConfig},
    time::{Clock, MILLIS_PER_SEC_F},
};
use jack::{
    AudioIn, Client, ClientOptions, Control, Port, ProcessScope, contrib::ClosureProcessHandler,
};
use miette::IntoDiagnostic;
use tokio::sync::mpsc;
use tosub::SubsystemHandle;
use tracing::{error, info};

struct State {
    app_id: String,
    sender: SenderApi,
    ports: Vec<Port<AudioIn>>,
    clock: JackClock,
    subsys: SubsystemHandle,
}

impl Drop for State {
    fn drop(&mut self) {
        info!("JACK recording stopped.");
        self.sender.stop();
    }
}

pub async fn start_recording(
    app_id: String,
    subsys: SubsystemHandle,
    sender: SenderApi,
    config: SenderConfig,
    clock: Clock,
    _monitoring: Monitoring,
) -> miette::Result<SubsystemHandle> {
    // TODO evaluate client status
    let (client, status) =
        Client::new(&config.label, ClientOptions::default()).into_diagnostic()?;

    info!(
        "JACK client '{}' created with status {:?}",
        config.label, status
    );

    let mut ports = vec![];

    for l in config.channel_labels.clone().iter() {
        let label = l.to_owned();
        ports.push(
            client
                .register_port(&label, AudioIn::default())
                .into_diagnostic()?,
        );
    }

    let (tx, notifications) = mpsc::channel(1024);
    let client_id = config.label.clone();
    let notification_handler = SessionManagerNotificationHandler { client_id, tx };
    let process_handler_state = State {
        app_id: app_id.clone(),
        sender,
        ports,
        clock: JackClock::new(clock),
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
        format!("tx/{}", config.id),
    );

    Ok(session_manager)
}

fn buffer_change(_: &mut State, client: &Client, buffer_len: jack::Frames) -> Control {
    let buffer_ms = buffer_len as f32 * MILLIS_PER_SEC_F / client.sample_rate() as f32;
    info!("JACK buffer size changed to {buffer_len} frames / {buffer_ms:.1} ms");
    Control::Continue
}

fn process(state: &mut State, _: &Client, ps: &ProcessScope) -> Control {
    // Check for shutdown early to avoid accessing resources during teardown
    // and prevent logging races that can cause RefCell panics
    if state.subsys.is_shut_down() {
        return Control::Quit;
    }

    let ingress_time = match state.clock.update_clock(ps) {
        Ok(ClockState::Stable(it)) => it,
        Ok(ClockState::Unstable) => {
            // TODO send empty packet
            return Control::Continue;
        }
        Err(e) => {
            error!("Could not get current media time: {e}");
            return Control::Quit;
        }
    };

    state
        .sender
        .start_write(ingress_time, ps.n_frames() as usize);

    for (ch, port) in state.ports.iter().enumerate() {
        state.sender.write_channel(ch, port.as_slice(ps));
    }

    if let Err(e) = state.sender.end_write() {
        // TODO sender was not ready; send to monitoring
    }

    Control::Continue
}
