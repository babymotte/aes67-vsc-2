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
    error::{Aes67Vsc2Error, Aes67Vsc2Result},
    formats::AudioFormat,
    receiver::config::RxDescriptor,
};
use pnet::datalink::{self, NetworkInterface};
use statime::time::Time;
use std::{any::Any, net::IpAddr};
use thread_priority::{
    RealtimeThreadSchedulePolicy, ThreadPriority, ThreadSchedulePolicy,
    set_thread_priority_and_policy, thread_native_id,
};
use tracing::{info, warn};

pub fn panic_to_string(panic: Box<dyn Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&'static str>() {
        s.to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

pub fn find_network_interface(ip: IpAddr) -> Aes67Vsc2Result<NetworkInterface> {
    for iface in datalink::interfaces() {
        for ipn in &iface.ips {
            if ipn.ip() == ip {
                return Ok(iface);
            }
        }
    }

    Err(Aes67Vsc2Error::Other(format!("no NIC with IP {ip} exists")))
}

pub struct AverageCalculationBuffer {
    buffer: Box<[i64]>,
    cursor: usize,
}

impl AverageCalculationBuffer {
    pub fn new(buffer: Box<[i64]>) -> Self {
        Self { buffer, cursor: 0 }
    }

    pub fn update(&mut self, value: i64) -> Option<i64> {
        self.buffer[self.cursor] = value;
        self.cursor += 1;
        if self.cursor >= self.buffer.len() {
            self.cursor = 0;
            let average = self.buffer.iter().sum::<i64>() / self.buffer.len() as i64;
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
