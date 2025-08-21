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

use aes67_vsc_2::{
    buffer::AudioBufferPointer,
    error::Aes67Vsc2Result,
    formats::{AudioFormat, FrameFormat, MilliSeconds, SampleFormat, SampleWriter},
    sender::start_sender,
    time::{MediaClock, SystemMediaClock},
    utils::{AverageCalculationBuffer, RequestResponseServerChannel, request_response_channel},
};
use miette::Result;
use rtp_rs::Seq;
use std::{
    f32::consts::PI,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};
use tokio::{
    select,
    time::{Interval, MissedTickBehavior, interval},
};
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let target_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(239, 69, 232, 56)), 5004);
    let local_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 178, 39));
    let audio_format = AudioFormat {
        sample_rate: 48_000,
        frame_format: FrameFormat {
            channels: 2,
            sample_format: SampleFormat::L24,
        },
    };
    let ptime = 1.0;

    let clock = SystemMediaClock::new(audio_format.clone());

    Toplevel::new(move |s| async move {
        s.start(SubsystemBuilder::new("aes67-vsc-2", move |s| async move {
            run(s, clock, local_ip, target_address, audio_format, ptime).await
        }));
    })
    .catch_signals()
    .handle_shutdown_requests(Duration::from_secs(1))
    .await?;

    Ok(())
}

async fn run<C: MediaClock>(
    subsys: SubsystemHandle,
    clock: C,
    local_ip: IpAddr,
    target_address: SocketAddr,
    audio_format: AudioFormat,
    ptime: MilliSeconds,
) -> Aes67Vsc2Result<()> {
    let (rrs, rrc) = request_response_channel();

    start_sender(
        &subsys,
        rrc,
        local_ip,
        0,
        target_address,
        audio_format,
        ptime,
    )
    .await?;

    let mut player = Player::new(clock, audio_format, rrs, ptime);

    loop {
        select! {
            Some(buf)  = player.rrs.on_request() => if !player.process_data_request(buf).await? {
                break;
            },
            _ = subsys.on_shutdown_requested() => break,
        }
    }

    Ok(())
}

struct Player<C: MediaClock> {
    clock: C,
    time: u64,
    audio_format: AudioFormat,
    rrs: RequestResponseServerChannel<AudioBufferPointer, (Seq, u64)>,
    interval: Interval,
    pos: u64,
    clock_drift_calculator: AverageCalculationBuffer<i64>,
}

impl<C: MediaClock> Player<C> {
    fn new(
        clock: C,
        audio_format: AudioFormat,
        rrs: RequestResponseServerChannel<AudioBufferPointer, (Seq, u64)>,
        ptime: MilliSeconds,
    ) -> Self {
        let mut interval = interval(Duration::from_nanos((ptime * 1_000_000.0) as u64));
        interval.set_missed_tick_behavior(MissedTickBehavior::Burst);
        Self {
            clock,
            time: 0,
            audio_format,
            rrs,
            interval,
            pos: 0,
            clock_drift_calculator: AverageCalculationBuffer::new(
                vec![0i64; (1000.0 / ptime).round() as usize].into(),
            ),
        }
    }

    async fn process_data_request(
        &mut self,
        audio_buffer: AudioBufferPointer,
    ) -> Aes67Vsc2Result<bool> {
        self.interval.tick().await;
        let seq_ts = self.write_audio_data(audio_buffer)?;
        Ok(self.rrs.respond(seq_ts))
    }

    fn write_audio_data(
        &mut self,
        audio_buffer: AudioBufferPointer,
    ) -> Aes67Vsc2Result<(Seq, u64)> {
        if self.time == 0 {
            self.time = self.clock.current_media_time()?;
        } else {
            if let Some(delay) = self
                .clock_drift_calculator
                .update(self.clock.current_media_time()? as i64 - self.time as i64)
            {
                eprintln!("ingress delay: {delay}");
                if delay < 0 {
                    self.time = (self.time as i64 + delay).max(0) as u64
                }
            }
        }

        let buffer = audio_buffer.buffer_mut();
        let frames = buffer.len()
            / self.audio_format.frame_format.channels
            / self
                .audio_format
                .frame_format
                .sample_format
                .bytes_per_sample();

        let timestamp = self.time;
        self.time += frames as u64;
        let chunk_size = buffer.len() / frames;

        for frame_buf in buffer.chunks_mut(chunk_size) {
            // TODO parameterize frequencey
            let frequency = 440.0;
            // TODO parameterize volume
            let vol = 0.5;

            let val = vol
                * (self.pos as f32 * (frequency / self.audio_format.sample_rate as f32) * 2.0 * PI)
                    .sin();
            self.pos += 1;

            let chunk_size = frame_buf.len() / self.audio_format.frame_format.channels;
            for ch_buf in frame_buf.chunks_mut(chunk_size) {
                self.audio_format
                    .frame_format
                    .sample_format
                    .write_sample(val, ch_buf);
            }
        }

        let seq = Seq::from((self.pos / frames as u64) as u16);

        Ok((seq, timestamp))
    }
}
