use crate::{
    error::{Aes67Vsc2Error, Aes67Vsc2Result},
    receiver::config::RxDescriptor,
};
use pnet::datalink::{self, NetworkInterface};
use statime::time::Time;
use std::{any::Any, net::IpAddr};

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

pub fn media_time_from_ptp(ptp_time: Time, desc: &RxDescriptor) -> u64 {
    let ptp_nanos = (ptp_time.secs() as u128) * 1_000_000_000 + ptp_time.subsec_nanos() as u128;
    let total_frames = (ptp_nanos * desc.audio_format.sample_rate as u128) / 1_000_000_000;
    total_frames as u64
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
