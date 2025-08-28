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

use crate::error::{ConfigError, ConfigResult};
use pnet::datalink::{self, NetworkInterface};
use std::{
    any::Any,
    fmt::Debug,
    iter::Sum,
    net::IpAddr,
    ops::{Add, Div},
};
use thread_priority::{
    RealtimeThreadSchedulePolicy, ThreadPriority, ThreadSchedulePolicy,
    set_thread_priority_and_policy, thread_native_id,
};
use tokio::sync::mpsc::{self, error::TryRecvError};
use tracing::{info, warn};

pub struct RequestResponseServerChannel<Req, Resp> {
    requests: mpsc::Receiver<Req>,
    responses: mpsc::Sender<Resp>,
}

impl<Req, Resp> RequestResponseServerChannel<Req, Resp> {
    pub async fn on_request(&mut self) -> Option<Req> {
        self.requests.recv().await
    }

    pub fn try_on_request(&mut self) -> Result<Req, TryRecvError> {
        self.requests.try_recv()
    }

    pub fn respond(&self, resp: Resp) -> bool {
        self.responses.try_send(resp).is_ok()
    }
}

pub struct RequestResponseClientChannel<Req, Resp> {
    requests: mpsc::Sender<Req>,
    responses: mpsc::Receiver<Resp>,
}

impl<Req, Resp> RequestResponseClientChannel<Req, Resp> {
    pub async fn request(&mut self, req: Req) -> Option<Resp> {
        self.requests.send(req).await.ok()?;
        self.responses.recv().await
    }

    pub fn request_blocking(&mut self, req: Req) -> Option<Resp> {
        self.requests.blocking_send(req).ok()?;
        self.responses.blocking_recv()
    }
}

pub fn request_response_channel<Req, Resp>() -> (
    RequestResponseServerChannel<Req, Resp>,
    RequestResponseClientChannel<Req, Resp>,
) {
    let (request_tx, request_rx) = mpsc::channel(1);
    let (response_tx, response_rx) = mpsc::channel(1);
    (
        RequestResponseServerChannel {
            requests: request_rx,
            responses: response_tx,
        },
        RequestResponseClientChannel {
            requests: request_tx,
            responses: response_rx,
        },
    )
}

pub fn panic_to_string(panic: Box<dyn Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&'static str>() {
        s.to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

pub fn find_network_interface(ip: IpAddr) -> ConfigResult<NetworkInterface> {
    for iface in datalink::interfaces() {
        for ipn in &iface.ips {
            if ipn.ip() == ip {
                return Ok(iface);
            }
        }
    }

    Err(ConfigError::NoSuchNIC(ip.to_string()))
}

pub trait GetAverage<T> {
    fn average(&self) -> T;
}

impl<N, S> GetAverage<N> for S
where
    N: Copy + TryFrom<usize, Error: Debug> + Add + Div<Output = N> + Sum<N>,
    S: AsRef<[N]>,
{
    fn average(&self) -> N {
        let slice = self.as_ref();
        slice.iter().map(ToOwned::to_owned).sum::<N>()
            / N::try_from(slice.len()).expect("cannot cast slice length to value type")
    }
}

pub struct AverageCalculationBuffer<N> {
    buffer: Box<[N]>,
    cursor: usize,
}

impl<N> AverageCalculationBuffer<N>
where
    Box<[N]>: GetAverage<N>,
{
    pub fn new(buffer: Box<[N]>) -> Self {
        Self { buffer, cursor: 0 }
    }

    pub fn update(&mut self, value: N) -> Option<N> {
        self.buffer[self.cursor] = value;
        self.cursor += 1;
        if self.cursor >= self.buffer.len() {
            self.cursor = 0;
            let average = self.buffer.average();
            Some(average)
        } else {
            None
        }
    }
}

pub fn set_realtime_priority() {
    let pid = thread_native_id();
    if let Err(e) = set_thread_priority_and_policy(
        pid,
        ThreadPriority::Max,
        ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo),
    ) {
        warn!("Could not set thread priority: {e}");
    } else {
        info!("Successfully set real time priority for thread {pid}.");
    }
}
