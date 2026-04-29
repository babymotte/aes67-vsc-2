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
    time::Clock,
};
use jack::{
    AudioIn, Client, ClientOptions, Control, Port, ProcessScope, contrib::ClosureProcessHandler,
};
use miette::IntoDiagnostic;
use tokio::sync::mpsc;
use tosub::SubsystemHandle;
#[cfg(debug_assertions)]
use tracing::{error, info};

struct State {
    sender: SenderApi,
    ports: Vec<Port<AudioIn>>,
    clock: JackClock,
    subsys: SubsystemHandle,
    config: SenderConfig,
}

impl Drop for State {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
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
                .register_port(&label, AudioIn::default())
                .into_diagnostic()?,
        );
    }

    let (tx, notifications) = mpsc::channel(1024);
    let client_id = config.label.clone();
    let notification_handler = SessionManagerNotificationHandler { client_id, tx };
    let process_handler_state = State {
        sender,
        ports,
        clock: JackClock::new(clock),
        subsys: subsys.clone(),
        config: config.clone(),
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
        return Control::Quit;
    }

    let (ingress_time, compensation) = match state.clock.update_clock(
        client,
        ps,
        state
            .config
            .packet_time
            .frames(state.config.audio_format.sample_rate),
        true,
    ) {
        Ok(ClockState::Stable {
            current_time,
            compensation,
        }) => (current_time, compensation),
        Ok(ClockState::Unstable) => {
            return Control::Continue;
        }
        Err(_e) => {
            #[cfg(debug_assertions)]
            error!("Could not get current media time: {_e}");
            return Control::Quit;
        }
    };

    state
        .sender
        .start_write(ingress_time, ps.n_frames() as usize, compensation);

    for (ch, port) in state.ports.iter().enumerate() {
        state.sender.write_channel(ch, port.as_slice(ps));
    }

    if let Err(_e) = state.sender.end_write() {
        // TODO sender was not ready; send to monitoring
    }

    Control::Continue
}
