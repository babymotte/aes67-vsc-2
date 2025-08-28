use crate::{monitoring::ObservabilityEvent, receiver::config::RxDescriptor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, time::Duration};
use tokio::{
    select, spawn,
    sync::mpsc,
    time::{sleep, timeout},
};
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::{info, instrument, warn};
use worterbuch_client::{KeyValuePair, Worterbuch, topic};

const ROOT_KEY: &str = "aes67-vsc";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverData {
    config: RxDescriptor,
    clock_offset: u64,
}

pub async fn observability(
    subsys: SubsystemHandle,
    client_name: String,
    rx: mpsc::Receiver<ObservabilityEvent>,
) -> Result<(), &'static str> {
    ObservabilityActor::new(subsys, client_name, rx).run().await;
    Ok(())
}

struct ObservabilityActor {
    subsys: SubsystemHandle,
    client_name: String,
    rx: mpsc::Receiver<ObservabilityEvent>,
    wb: Option<Worterbuch>,
    senders: HashMap<String, String>,
    receivers: HashMap<String, ReceiverData>,
    running: bool,
}

impl ObservabilityActor {
    fn new(
        subsys: SubsystemHandle,
        client_name: String,
        rx: mpsc::Receiver<ObservabilityEvent>,
    ) -> Self {
        Self {
            subsys,
            client_name,
            rx,
            wb: None,
            senders: HashMap::new(),
            receivers: HashMap::new(),
            running: false,
        }
    }

    async fn run(mut self) {
        let (wb_disco_tx, mut wb_disco_rx) = mpsc::channel(1);
        self.restart_wb(wb_disco_tx.clone(), self.client_name.clone())
            .await;

        info!("Observability subsystem started.");
        loop {
            select! {
                Some(evt) = self.rx.recv() => self.process_event(evt).await,
                recv = wb_disco_rx.recv() => if recv.is_some() { self.restart_wb(wb_disco_tx.clone(), self.client_name.clone()).await },
                _ = self.subsys.on_shutdown_requested() => break,
                else => {
                    self.subsys.request_shutdown();
                    break;
                },
            }
        }
        info!("Observability subsystem stopped.");
    }

    #[instrument(skip(self, disco))]
    async fn restart_wb(&mut self, disco: mpsc::Sender<()>, client_name: String) {
        warn!("(Re-)connecting to worterbuch …");
        drop(self.wb.take());

        let Ok(Ok((wb, on_disconnect, _))) = timeout(
            Duration::from_secs(1),
            worterbuch_client::connect_with_default_config(),
        )
        .await
        else {
            warn!("Could not connect to worterbuch, trying again in 5 seconds …");
            spawn(async move {
                sleep(Duration::from_secs(5)).await;
                disco.send(()).await.ok();
            });
            return;
        };

        spawn(async move {
            on_disconnect.await;
            sleep(Duration::from_secs(5)).await;
            disco.send(()).await.ok();
        });

        wb.set_client_name(topic!(ROOT_KEY, client_name)).await.ok();
        wb.set_grave_goods(&[&topic!(ROOT_KEY, client_name, "#")])
            .await
            .ok();
        wb.set_last_will(&[KeyValuePair::of(
            topic!(ROOT_KEY, client_name, "running"),
            false,
        )])
        .await
        .ok();

        self.wb = Some(wb);
        self.re_publish().await;
    }

    async fn re_publish(&self) {
        if self.running {
            self.publish_vsc().await;
        }
        for (name, sdp) in &self.senders {
            self.publish_sender(name, sdp.to_owned()).await;
        }
        for (name, data) in &self.receivers {
            self.publish_receiver(name, data.clone()).await;
        }
    }

    async fn process_event(&mut self, evt: ObservabilityEvent) {
        match evt {
            ObservabilityEvent::VscEvent(e) => self.process_vsc_event(e).await,
            ObservabilityEvent::SenderEvent(e) => self.process_sender_event(e).await,
            ObservabilityEvent::ReceiverEvent(e) => self.process_receiver_event(e).await,
            ObservabilityEvent::Stats(e) => self.process_stats_report(e).await,
        }
    }

    async fn process_vsc_event(&mut self, e: super::VscEvent) {
        match e {
            super::VscEvent::VscCreated => self.vsc_created().await,
        }
    }

    async fn process_sender_event(&mut self, e: super::SenderEvent) {
        match e {
            super::SenderEvent::SenderCreated { name, sdp } => self.sender_created(name, sdp).await,
            super::SenderEvent::SenderDestroyed { name } => self.sender_destroyed(name).await,
        }
    }

    async fn process_receiver_event(&mut self, e: super::ReceiverEvent) {
        match e {
            super::ReceiverEvent::ReceiverCreated { name, descriptor } => {
                self.receiver_created(name, descriptor).await
            }
            super::ReceiverEvent::ReceiverDestroyed { name } => self.receiver_destroyed(name).await,
        }
    }

    async fn process_stats_report(&mut self, e: super::StatsReport) {
        match e {
            super::StatsReport::Vsc(r) => self.process_vsc_stats_report(r).await,
            super::StatsReport::Sender(r) => self.process_sender_stats_report(r).await,
            super::StatsReport::Receiver(r) => self.process_receiver_stats_report(r).await,
        }
    }

    async fn vsc_created(&mut self) {
        self.running = true;
        self.publish_vsc().await;
    }

    async fn sender_created(&mut self, name: String, sdp: String) {
        self.senders.insert(name.clone(), sdp.clone());
        self.publish_sender(&name, sdp).await;
    }

    async fn sender_destroyed(&mut self, name: String) {
        self.senders.remove(&name);
        self.unpublish_sender(&name).await;
    }

    async fn receiver_created(&mut self, name: String, descriptor: RxDescriptor) {
        let data = ReceiverData {
            clock_offset: 0,
            config: descriptor.clone(),
        };
        self.receivers.insert(name.clone(), data.clone());
        self.publish_receiver(&name, data).await;
    }

    async fn receiver_destroyed(&mut self, name: String) {
        self.receivers.remove(&name);
        self.unpublish_receiver(&name).await;
    }

    async fn process_vsc_stats_report(&mut self, report: super::VscStatsReport) {
        match report {}
    }

    async fn process_sender_stats_report(&mut self, report: super::SenderStatsReport) {
        match report {}
    }

    async fn process_receiver_stats_report(&mut self, report: super::ReceiverStatsReport) {
        match report {
            super::ReceiverStatsReport::MediaClockOffsetChanged { receiver, offset } => {
                self.receiver_clock_offset_changed(receiver, offset).await;
            }
        }
    }

    async fn receiver_clock_offset_changed(&mut self, name: String, offset: u64) {
        let Some(receiver) = self.receivers.get_mut(&name) else {
            return;
        };
        receiver.clock_offset = offset;
        let data = receiver.clone();
        self.publish_receiver(&name, data).await;
    }

    async fn publish_vsc(&self) {
        if let Some(wb) = &self.wb {
            wb.set_async(topic!(ROOT_KEY, self.client_name, "running"), true)
                .await
                .ok();
        };
    }

    async fn publish_sender(&self, name: &str, sdp: String) {
        if let Some(wb) = &self.wb {
            wb.set_async(topic!(ROOT_KEY, name, "sdp"), sdp).await.ok();
        };
    }

    async fn unpublish_sender(&mut self, name: &str) {
        if let Some(wb) = &self.wb {
            wb.delete_async(topic!(ROOT_KEY, name, "sdp")).await.ok();
        };
    }

    async fn publish_receiver(&self, name: &str, data: ReceiverData) {
        if let Some(wb) = &self.wb {
            publish_individual(wb, topic!(ROOT_KEY, name, "config"), data).await;
        };
    }

    async fn unpublish_receiver(&self, name: &str) {
        if let Some(wb) = &self.wb {
            wb.delete_async(topic!(ROOT_KEY, name)).await.ok();
        };
    }
}

async fn publish_individual(wb: &Worterbuch, key: String, object: impl Serialize) {
    let Ok(json) = serde_json::to_value(object) else {
        return;
    };

    publish_individual_values(wb, key, json).await;
}

async fn publish_individual_values(wb: &Worterbuch, key: String, object: Value) {
    if let Value::Object(map) = object {
        for (k, v) in map {
            Box::pin(publish_individual_values(wb, topic!(key, k), v)).await;
        }
    } else {
        wb.set_async(key, object).await.ok();
    }
}
