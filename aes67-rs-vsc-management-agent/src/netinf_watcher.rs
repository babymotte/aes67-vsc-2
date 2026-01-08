use pnet::datalink::{self, NetworkInterface};
use std::{mem, time::Duration};
use tokio::{select, spawn, sync::mpsc, time::interval};
use tracing::info;
use worterbuch_client::{Worterbuch, topic};

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
        }
    }

    let _ = mem::replace(known_interfaces, interfaces);
}

fn state(netinf: &NetworkInterface) -> (bool, bool) {
    let use_inf = !netinf.is_loopback() && netinf.mac.is_some() && netinf.is_multicast();
    let active = netinf.is_up() && netinf.is_running() && !netinf.ips.is_empty();
    (use_inf, active)
}
