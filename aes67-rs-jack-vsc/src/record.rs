use crate::{
    common::{ClockState, JackClock},
    session_manager::{SessionManagerNotificationHandler, start_session_manager},
};
use aes67_rs::{
    buffer::AudioBufferPointer,
    sender::{api::SenderApi, config::TxDescriptor},
    time::{Clock, MILLIS_PER_SEC_F},
};
use futures_lite::future::block_on;
use jack::{
    AudioIn, Client, ClientOptions, Control, Port, ProcessScope, contrib::ClosureProcessHandler,
};
use miette::IntoDiagnostic;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::{error, info};

struct State {
    app_id: String,
    sender: SenderApi,
    ports: Vec<Port<AudioIn>>,
    channel_bufs: Box<[AudioBufferPointer]>,
    clock: JackClock,
    send_buffer: Box<[f32]>,
    #[deprecated = "derive buffer position from media time"]
    send_buf_pos: usize,
    sample_rate: u32,
}

pub async fn start_recording(
    app_id: String,
    subsys: &mut SubsystemHandle,
    sender: SenderApi,
    descriptor: TxDescriptor,
    clock: Clock,
) -> miette::Result<()> {
    // TODO evaluate client status
    let (client, status) =
        Client::new(&descriptor.id, ClientOptions::default()).into_diagnostic()?;

    info!(
        "JACK client '{}' created with status {:?}",
        descriptor.id, status
    );

    let mut ports = vec![];

    for (i, l) in descriptor.channel_labels.iter().enumerate() {
        let label = l.to_owned().unwrap_or(format!("in{}", i + 1));
        ports.push(
            client
                .register_port(&label, AudioIn::default())
                .into_diagnostic()?,
        );
    }

    let send_buffer_len = descriptor.audio_format.sample_rate as usize
        * descriptor.audio_format.frame_format.channels;
    let send_buffer = vec![0.0; send_buffer_len].into();

    let (tx, notifications) = mpsc::channel(1024);
    let client_id = descriptor.id.clone();
    let notification_handler = SessionManagerNotificationHandler { client_id, tx };
    let process_handler_state = State {
        app_id: app_id.clone(),
        sender,
        ports,
        channel_bufs: vec![
            AudioBufferPointer::new(0, 0);
            descriptor.audio_format.frame_format.channels
        ]
        .into(),
        clock: JackClock::new(clock),
        send_buffer,
        send_buf_pos: 0,
        sample_rate: descriptor.audio_format.sample_rate,
    };
    let process_handler =
        ClosureProcessHandler::with_state(process_handler_state, process, buffer_change);

    let active_client = client
        .activate_async(notification_handler, process_handler)
        .into_diagnostic()?;
    start_session_manager(subsys, active_client, notifications, app_id);

    subsys.on_shutdown_requested().await;

    Ok(())
}

fn buffer_change(_: &mut State, client: &Client, buffer_len: jack::Frames) -> Control {
    let buffer_ms = buffer_len as f32 * MILLIS_PER_SEC_F / client.sample_rate() as f32;
    info!("JACK buffer size changed to {buffer_len} frames / {buffer_ms:.1} ms");
    Control::Continue
}

fn process(state: &mut State, _: &Client, ps: &ProcessScope) -> Control {
    let start = Instant::now();

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

    let send_buffers = state.send_buffer.chunks_mut(state.sample_rate as usize);

    for (ch, (port, send_buf)) in state.ports.iter().zip(send_buffers).enumerate() {
        let port_buf = port.as_slice(ps);

        let start_index = state.send_buf_pos;
        let end_index = start_index + ps.n_frames() as usize;

        let send_buf_slice = &mut send_buf[start_index..end_index];
        send_buf_slice.copy_from_slice(port_buf);

        state.channel_bufs[ch] = AudioBufferPointer::from_slice(send_buf_slice);
    }

    let pre_req = Instant::now();

    block_on(state.sender.send(&state.channel_bufs, ingress_time));

    let post_req = Instant::now();

    state.send_buf_pos = (state.send_buf_pos + ps.n_frames() as usize) % state.sample_rate as usize;

    // TODO send to monitoring

    let _total = post_req.duration_since(start).as_micros();
    let _req = post_req.duration_since(pre_req).as_micros();

    // if total > 100 {
    //     eprintln!("latency record req: {req} µs");
    //     eprintln!("latency record total: {total} µs");
    // }

    Control::Continue
}
