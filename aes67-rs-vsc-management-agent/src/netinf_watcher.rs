use pnet::datalink::{self, NetworkInterface};
use std::{mem, time::Duration};
use tokio::{select, spawn, sync::mpsc, time::interval};
use worterbuch_client::{Worterbuch, topic};

// Linux ethtool constants for hardware timestamping queries
const ETHTOOL_GET_TS_INFO: u32 = 0x00000041;
const SIOCETHTOOL: u64 = 0x8946;

#[repr(C)]
struct EthtoolTsInfo {
    cmd: u32,
    so_timestamping: u32,
    phc_index: i32,
    tx_types: u32,
    tx_reserved: [u32; 3],
    rx_filters: u32,
    rx_reserved: [u32; 3],
}

#[repr(C)]
struct Ifreq {
    ifr_name: [u8; 16],
    ifr_data: *mut EthtoolTsInfo,
}

#[derive(Clone)]
pub struct Handle(mpsc::Sender<Option<()>>);

impl Handle {
    pub async fn refresh(&self) {
        self.0.send(Some(())).await.ok();
    }

    pub async fn close(self) {
        self.0.send(None).await.ok();
    }

    pub async fn closed(&self) {
        self.0.closed().await;
    }
}

pub async fn start(app_id: String, scan_period: Duration, wb: Worterbuch) -> Handle {
    let interval = interval(scan_period);

    let (tx, rx) = mpsc::channel(1);

    spawn(watch(app_id, wb, interval, rx));

    Handle(tx)
}

async fn watch(
    app_id: String,
    wb: Worterbuch,
    mut interval: tokio::time::Interval,
    mut rx: mpsc::Receiver<Option<()>>,
) {
    let mut known_interfaces = vec![];

    loop {
        select! {
            _ = interval.tick() => refresh(&app_id, &mut known_interfaces,  &wb).await,
            Some(thing) = rx.recv() => {
                match thing {
                    Some(()) => refresh(&app_id, &mut known_interfaces,  &wb).await,
                    None => break,
                }
            },
            else => break,
        }
    }
}

struct InfWrapper(NetworkInterface);

impl PartialEq<InfWrapper> for InfWrapper {
    fn eq(&self, other: &InfWrapper) -> bool {
        self.0.index == other.0.index
    }
}

async fn refresh(app_id: &str, known_interfaces: &mut Vec<InfWrapper>, wb: &Worterbuch) {
    let interfaces: Vec<InfWrapper> = datalink::interfaces().into_iter().map(InfWrapper).collect();

    for interface in known_interfaces.iter() {
        if !interfaces.contains(interface) {
            wb.pdelete_async(
                topic!(app_id, "networkInterfaces", interface.0.name, "#"),
                true,
            )
            .await
            .ok();
        }
    }

    for interface in interfaces.iter() {
        let (use_inf, active) = state(&interface.0);

        if use_inf {
            wb.set_async(
                topic!(app_id, "networkInterfaces", interface.0.name, "active"),
                active,
            )
            .await
            .ok();

            let ptp_enabled = has_ptpv2_phc(&interface.0).unwrap_or(false);

            wb.set_async(
                topic!(app_id, "networkInterfaces", interface.0.name, "ptp"),
                ptp_enabled,
            )
            .await
            .ok();
        }
    }

    let _ = mem::replace(known_interfaces, interfaces);
}

fn state(netinf: &NetworkInterface) -> (bool, bool) {
    let use_inf = !netinf.is_loopback() && netinf.mac.is_some() && netinf.is_multicast();
    let active = netinf.is_up() && netinf.is_running() && !netinf.ips.is_empty();
    (use_inf, active)
}

/// Checks if a network interface has a PTPv2 compatible PHC (Precision Hardware Clock).
///
/// Returns `Ok(true)` if the interface has a valid PHC, `Ok(false)` if it doesn't,
/// or an error if the check fails.
///
/// # Arguments
/// * `interface` - The network interface to check
///
/// # Returns
/// * `Ok(true)` - Interface has a PTPv2 compatible PHC (phc_index >= 0)
/// * `Ok(false)` - Interface does not have a PHC (phc_index < 0)
/// * `Err` - Failed to query the interface
pub fn has_ptpv2_phc(interface: &NetworkInterface) -> std::io::Result<bool> {
    use std::os::unix::io::RawFd;

    // Create a UDP socket for ioctl operations
    let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if sock < 0 {
        return Err(std::io::Error::last_os_error());
    }

    // Ensure socket is closed when function exits
    let _guard = scopeguard::guard(sock, |fd| unsafe {
        libc::close(fd);
    });

    // Prepare the interface name (max 15 chars + null terminator)
    let mut ifr_name = [0u8; 16];
    let name_bytes = interface.name.as_bytes();
    let copy_len = name_bytes.len().min(15);
    ifr_name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

    // Prepare ethtool timestamp info structure
    let mut ts_info = EthtoolTsInfo {
        cmd: ETHTOOL_GET_TS_INFO,
        so_timestamping: 0,
        phc_index: -1,
        tx_types: 0,
        tx_reserved: [0; 3],
        rx_filters: 0,
        rx_reserved: [0; 3],
    };

    // Prepare ifreq structure
    let mut ifr = Ifreq {
        ifr_name,
        ifr_data: &mut ts_info as *mut EthtoolTsInfo,
    };

    // Perform the ioctl call
    let result = unsafe {
        libc::ioctl(
            sock as RawFd,
            SIOCETHTOOL as libc::c_ulong,
            &mut ifr as *mut Ifreq,
        )
    };

    if result < 0 {
        return Err(std::io::Error::last_os_error());
    }

    // A phc_index >= 0 indicates a valid PHC device
    Ok(ts_info.phc_index >= 0)
}
