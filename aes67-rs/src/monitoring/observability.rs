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
    formats::{Frames, MilliSeconds},
    monitoring::{
        HealthReport, ReceiverHealthReport, ReceiverState, ReceiverStatsReport, Report,
        SenderHealthReport, SenderState, SenderStatsReport, StateEvent, StatsReport,
        VscHealthReport, VscState, VscStatsReport,
    },
    receiver::config::RxDescriptor,
    sender::config::TxDescriptor,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use tokio::{
    select, spawn,
    sync::{broadcast, mpsc},
    time::{sleep, timeout},
};
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::{info, instrument, warn};
use worterbuch_client::{KeyValuePair, Worterbuch, topic};

const ROOT_KEY: &str = "aes67-vsc";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LostPackets {
    count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "serde_millis")]
    last: Option<SystemTime>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SenderStats {}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverStats {
    clock_offset: u64,
    network_delay_frames: i64,
    network_delay_millis: f32,
    measured_link_offset_frames: Frames,
    measured_link_offset_millis: f32,
    lost_packets: LostPackets,
    late_packets: LostPackets,
    muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SenderData {
    config: TxDescriptor,
    stats: SenderStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverData {
    config: RxDescriptor,
    stats: ReceiverStats,
}

pub async fn observability(
    subsys: SubsystemHandle,
    client_name: String,
    rx: broadcast::Receiver<Report>,
) -> Result<(), &'static str> {
    ObservabilityActor::new(subsys, client_name, rx).run().await;
    Ok(())
}

struct ObservabilityActor {
    subsys: SubsystemHandle,
    client_name: String,
    rx: broadcast::Receiver<Report>,
    wb: Option<Worterbuch>,
    senders: HashMap<String, SenderData>,
    receivers: HashMap<String, ReceiverData>,
    running: bool,
}

impl ObservabilityActor {
    fn new(subsys: SubsystemHandle, client_name: String, rx: broadcast::Receiver<Report>) -> Self {
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
                Ok(evt) = self.rx.recv() => self.process_event(evt).await,
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
        for (name, data) in &self.senders {
            self.publish_sender(name, data.clone()).await;
        }
        for (name, data) in &self.receivers {
            self.publish_receiver(name, data.clone()).await;
        }
    }

    async fn process_event(&mut self, evt: Report) {
        match evt {
            Report::State(state) => self.process_state(state).await,
            Report::Stats(stats) => self.process_stats_report(stats).await,
            Report::Health(health) => self.process_health_report(health).await,
        }
    }

    async fn process_state(&mut self, state: StateEvent) {
        match state {
            StateEvent::Vsc(e) => self.process_vsc_event(e).await,
            StateEvent::Sender(e) => self.process_sender_event(e).await,
            StateEvent::Receiver(e) => self.process_receiver_event(e).await,
        }
    }

    async fn process_vsc_event(&mut self, e: VscState) {
        match e {
            VscState::VscCreated => self.vsc_created().await,
        }
    }

    async fn process_sender_event(&mut self, e: SenderState) {
        match e {
            SenderState::SenderCreated { name, descriptor } => {
                self.sender_created(name, descriptor).await
            }
            SenderState::SenderDestroyed { name } => self.sender_destroyed(name).await,
        }
    }

    async fn process_receiver_event(&mut self, e: ReceiverState) {
        match e {
            ReceiverState::ReceiverCreated { name, descriptor } => {
                self.receiver_created(name, descriptor).await
            }
            ReceiverState::ReceiverDestroyed { name } => self.receiver_destroyed(name).await,
        }
    }

    async fn process_stats_report(&mut self, e: StatsReport) {
        match e {
            StatsReport::Vsc(r) => self.process_vsc_stats_report(r).await,
            StatsReport::Sender(r) => self.process_sender_stats_report(r).await,
            StatsReport::Receiver(r) => self.process_receiver_stats_report(r).await,
        }
    }

    async fn process_health_report(&mut self, e: HealthReport) {
        match e {
            HealthReport::Vsc(r) => self.process_vsc_health_report(r).await,
            HealthReport::Sender(r) => self.process_sender_health_report(r).await,
            HealthReport::Receiver(r) => self.process_receiver_health_report(r).await,
        }
    }

    async fn vsc_created(&mut self) {
        info!("VSC created");
        self.running = true;
        self.publish_vsc().await;
    }

    async fn sender_created(&mut self, name: String, descriptor: TxDescriptor) {
        let data = SenderData {
            config: descriptor.clone(),
            stats: SenderStats::default(),
        };
        self.senders.insert(name.clone(), data.clone());
        self.publish_sender(&name, data).await;
    }

    async fn sender_destroyed(&mut self, name: String) {
        self.senders.remove(&name);
        self.unpublish_sender(&name).await;
    }

    async fn receiver_created(&mut self, name: String, descriptor: RxDescriptor) {
        let data = ReceiverData {
            config: descriptor.clone(),
            stats: ReceiverStats::default(),
        };
        self.receivers.insert(name.clone(), data.clone());
        self.publish_receiver_config(&name, data.config).await;
    }

    async fn receiver_destroyed(&mut self, name: String) {
        self.receivers.remove(&name);
        self.unpublish_receiver(&name).await;
    }

    async fn process_vsc_stats_report(&mut self, report: VscStatsReport) {
        match report {}
    }

    async fn process_sender_stats_report(&mut self, report: SenderStatsReport) {
        match report {}
    }

    async fn process_receiver_stats_report(&mut self, report: ReceiverStatsReport) {
        match report {
            ReceiverStatsReport::MediaClockOffsetChanged { receiver, offset } => {
                self.receiver_clock_offset_changed(receiver, offset).await;
            }
            ReceiverStatsReport::NetworkDelay {
                receiver,
                delay_frames,
                delay_millis,
            } => {
                self.receiver_network_delay_changed(receiver, delay_frames, delay_millis)
                    .await;
            }
            ReceiverStatsReport::MeasuredLinkOffset {
                receiver,
                link_offset_frames,
                link_offset_ms,
            } => {
                self.receiver_measured_link_offset_changed(
                    receiver,
                    link_offset_frames,
                    link_offset_ms,
                )
                .await;
            }
            ReceiverStatsReport::LostPackets {
                receiver,
                lost_packets,
                timestamp,
            } => {
                self.receiver_lost_packets_changed(receiver, lost_packets, timestamp)
                    .await;
            }
            ReceiverStatsReport::LatePackets {
                receiver,
                late_packets,
                timestamp,
            } => {
                self.receiver_late_packets_changed(receiver, late_packets, timestamp)
                    .await;
            }
            ReceiverStatsReport::Muted { receiver, muted } => {
                self.receiver_muted_changed(receiver, muted).await
            }
        }
    }

    async fn receiver_clock_offset_changed(&mut self, name: String, offset: u64) {
        let Some(receiver) = self.receivers.get_mut(&name) else {
            return;
        };
        receiver.stats.clock_offset = offset;
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&name, stats).await;
    }

    async fn receiver_network_delay_changed(
        &mut self,
        name: String,
        delay_frames: i64,
        delay_millis: f32,
    ) {
        let Some(receiver) = self.receivers.get_mut(&name) else {
            return;
        };
        receiver.stats.network_delay_frames = delay_frames;
        receiver.stats.network_delay_millis = delay_millis;
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&name, stats).await;
    }

    async fn receiver_measured_link_offset_changed(
        &mut self,
        name: String,
        link_offset_frames: Frames,
        link_offset_ms: MilliSeconds,
    ) {
        let Some(receiver) = self.receivers.get_mut(&name) else {
            return;
        };
        receiver.stats.measured_link_offset_frames = link_offset_frames;
        receiver.stats.measured_link_offset_millis = link_offset_ms;
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&name, stats).await;
    }

    async fn receiver_lost_packets_changed(
        &mut self,
        name: String,
        lost_packets: usize,
        timestamp: SystemTime,
    ) {
        let Some(receiver) = self.receivers.get_mut(&name) else {
            return;
        };
        receiver.stats.lost_packets = LostPackets {
            count: lost_packets,
            last: Some(timestamp),
        };
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&name, stats).await;
    }

    async fn receiver_late_packets_changed(
        &mut self,
        name: String,
        late_packets: usize,
        timestamp: SystemTime,
    ) {
        let Some(receiver) = self.receivers.get_mut(&name) else {
            return;
        };
        receiver.stats.late_packets = LostPackets {
            count: late_packets,
            last: Some(timestamp),
        };
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&name, stats).await;
    }

    async fn receiver_muted_changed(&mut self, name: String, muted: bool) {
        let Some(receiver) = self.receivers.get_mut(&name) else {
            return;
        };
        receiver.stats.muted = muted;
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&name, stats).await;
    }

    async fn process_vsc_health_report(&mut self, report: VscHealthReport) {
        match report {}
    }

    async fn process_sender_health_report(&mut self, report: SenderHealthReport) {
        match report {}
    }

    async fn process_receiver_health_report(&mut self, report: ReceiverHealthReport) {
        match report {}
    }

    async fn publish_vsc(&self) {
        if let Some(wb) = &self.wb {
            wb.set_async(topic!(ROOT_KEY, self.client_name, "running"), true)
                .await
                .ok();
        };
    }

    async fn publish_sender(&self, name: &str, data: SenderData) {
        if let Some(wb) = &self.wb {
            publish_individual(wb, topic!(ROOT_KEY, name), data).await;
        };
    }

    async fn unpublish_sender(&mut self, name: &str) {
        if let Some(wb) = &self.wb {
            wb.delete_async(topic!(ROOT_KEY, name)).await.ok();
        };
    }

    async fn publish_receiver(&self, name: &str, data: ReceiverData) {
        if let Some(wb) = &self.wb {
            publish_individual(wb, topic!(ROOT_KEY, name), data).await;
        };
    }

    async fn publish_receiver_config(&self, name: &str, config: RxDescriptor) {
        if let Some(wb) = &self.wb {
            publish_individual(wb, topic!(ROOT_KEY, name, "config"), config).await;
        };
    }

    async fn publish_receiver_stats(&self, name: &str, stats: ReceiverStats) {
        if let Some(wb) = &self.wb {
            publish_individual(wb, topic!(ROOT_KEY, name, "stats"), stats).await;
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
