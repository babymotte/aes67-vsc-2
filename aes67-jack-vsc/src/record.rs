use std::{
    ptr::slice_from_raw_parts,
    slice::{from_raw_parts, from_raw_parts_mut},
    time::Instant,
};

use crate::session_manager::{SessionManagerNotificationHandler, start_session_manager};
use aes67_rs::{
    buffer::AudioBufferPointer,
    formats::Frames,
    sender::{api::SenderApi, config::TxDescriptor},
    time::MediaClock,
};
use jack::{
    AudioIn, Client, ClientOptions, Control, Port, ProcessScope, contrib::ClosureProcessHandler,
};
use miette::IntoDiagnostic;
use tokio::sync::mpsc;
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::{error, info};

struct State<C: MediaClock> {
    sender: SenderApi,
    ports: Vec<Port<AudioIn>>,
    channel_bufs: Box<[AudioBufferPointer]>,
    clock: C,
    jack_clock: Frames,
    send_buffer: Box<[f32]>,
    send_buf_pos: usize,
    sample_rate: u32,
}

pub async fn start_recording<C: MediaClock>(
    subsys: SubsystemHandle,
    sender: SenderApi,
    descriptor: TxDescriptor,
    clock: C,
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
    let notification_handler = SessionManagerNotificationHandler { tx };
    let process_handler_state = State {
        sender,
        ports,
        channel_bufs: vec![
            AudioBufferPointer::new(0, 1);
            descriptor.audio_format.frame_format.channels
        ]
        .into(),
        clock,
        jack_clock: 0,
        send_buffer,
        send_buf_pos: 0,
        sample_rate: descriptor.audio_format.sample_rate,
    };
    let process_handler =
        ClosureProcessHandler::with_state(process_handler_state, process, buffer_change);

    let active_client = client
        .activate_async(notification_handler, process_handler)
        .into_diagnostic()?;
    start_session_manager(&subsys, active_client, notifications);

    subsys.on_shutdown_requested().await;

    Ok(())
}

fn buffer_change<C: MediaClock>(
    _: &mut State<C>,
    client: &Client,
    buffer_len: jack::Frames,
) -> Control {
    let buffer_ms = buffer_len as f32 * 1_000.0 / client.sample_rate() as f32;
    info!("JACK buffer size changed to {buffer_len} frames / {buffer_ms:.1} ms");
    Control::Continue
}

fn process<C: MediaClock>(state: &mut State<C>, client: &Client, ps: &ProcessScope) -> Control {
    let start = Instant::now();

    // TODO observe and compensate clock drift
    if state.jack_clock == 0 {
        let Ok(now) = state.clock.current_media_time() else {
            error!("Could not get PTP time.");
            return Control::Quit;
        };
        state.jack_clock = now - ps.n_frames() as Frames;
    }

    let send_buffers = state.send_buffer.chunks_mut(state.sample_rate as usize);

    for (ch, (port, send_buf)) in state.ports.iter().zip(send_buffers).enumerate() {
        let port_buf = port.as_slice(ps);

        let start_index = state.send_buf_pos;
        let end_index = start_index + ps.n_frames() as usize;

        let send_buf_slice = &mut send_buf[start_index..end_index];
        send_buf_slice.copy_from_slice(port_buf);

        state.channel_bufs[ch] = AudioBufferPointer::from_slice(send_buf_slice);
    }

    let ingress_time = state.jack_clock;

    let pre_req = Instant::now();

    state
        .sender
        .send_blocking(state.channel_bufs.clone(), ingress_time);

    let post_req = Instant::now();

    state.jack_clock += ps.n_frames() as u64;
    state.send_buf_pos = (state.send_buf_pos + ps.n_frames() as usize) % state.sample_rate as usize;

    // TODO send to monitoring

    let total = post_req.duration_since(start).as_micros();
    let req = post_req.duration_since(pre_req).as_micros();

    if total > 100 {
        eprintln!("req: {req} µs");
        eprintln!("total: {total} µs");
    }

    Control::Continue
}
