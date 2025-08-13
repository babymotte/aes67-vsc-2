use std::{any::Any, net::IpAddr};

use pnet::datalink::{self, NetworkInterface};

use crate::error::{Aes67Vsc2Error, Aes67Vsc2Result};

const WRAP: u128 = (1u64 << 32) as u128;

pub fn panic_to_string(panic: Box<dyn Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&'static str>() {
        s.to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn unwrap_rtp(ts32: u32, last_ext: i128) -> i128 {
    // last_low is last_ext mod 2^32 interpreted as u64
    let last_low = (last_ext as u128 & (WRAP - 1)) as u32;
    // compute difference in signed 32-bit space
    let delta = (ts32 as i64) - (last_low as i64);
    // choose delta that is in range [-2^31, 2^31)
    let adj = if delta > (1i64 << 31) {
        delta - (1i64 << 32)
    } else if delta < -(1i64 << 31) {
        delta + (1i64 << 32)
    } else {
        delta
    };
    last_ext + (adj as i128)
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
