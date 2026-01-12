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
    buffer::AudioBufferPointer,
    formats::{Frames, MilliSeconds},
    monitoring::{
        HealthReport, ReceiverHealthReport, ReceiverState, ReceiverStatsReport, Report,
        SenderHealthReport, SenderState, SenderStatsReport, StateEvent, StatsReport,
        VscHealthReport, VscState, VscStatsReport,
    },
    receiver::config::RxDescriptor,
    sender::config::TxDescriptor,
    utils::publish_individual,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::SystemTime};
use tokio::{select, sync::broadcast};
use tokio_graceful_shutdown::SubsystemHandle;
use tracing::info;
use worterbuch_client::{Worterbuch, topic};

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
    label: String,
    address: AudioBufferPointer,
    running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverData {
    config: RxDescriptor,
    stats: ReceiverStats,
    label: String,
    address: AudioBufferPointer,
    running: bool,
}

pub async fn observability(
    subsys: &mut SubsystemHandle,
    client_name: String,
    rx: broadcast::Receiver<Report>,
    worterbuch_client: Worterbuch,
) -> Result<(), &'static str> {
    ObservabilityActor::new(subsys, client_name, rx, worterbuch_client)
        .run()
        .await;
    Ok(())
}

struct ObservabilityActor<'a> {
    subsys: &'a mut SubsystemHandle,
    client_name: String,
    rx: broadcast::Receiver<Report>,
    wb: Worterbuch,
    senders: HashMap<String, SenderData>,
    receivers: HashMap<String, ReceiverData>,
    running: bool,
}

impl<'a> ObservabilityActor<'a> {
    fn new(
        subsys: &'a mut SubsystemHandle,
        client_name: String,
        rx: broadcast::Receiver<Report>,
        wb: Worterbuch,
    ) -> Self {
        Self {
            subsys,
            client_name,
            rx,
            wb,
            senders: HashMap::new(),
            receivers: HashMap::new(),
            running: false,
        }
    }

    async fn run(mut self) {
        info!("Observability subsystem started.");
        loop {
            select! {
                Ok(evt) = self.rx.recv() => self.process_event(evt).await,
                _ = self.subsys.on_shutdown_requested() => break,
                else => {
                    self.subsys.request_shutdown();
                    break;
                },
            }
        }
        info!("Observability subsystem stopped.");
    }

    async fn re_publish(&self) {
        if self.running {
            self.publish_vsc().await;
        }
        for (qualified_id, data) in &self.senders {
            self.publish_sender(qualified_id, data.clone()).await;
        }
        for (qualified_id, data) in &self.receivers {
            self.publish_receiver(qualified_id, data.clone()).await;
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
            SenderState::Created {
                id,
                descriptor,
                label,
                address,
            } => self.sender_created(id, label, descriptor, address).await,
            SenderState::Renamed { id, label } => self.sender_renamed(id, label).await,
            SenderState::Destroyed { id } => self.sender_destroyed(id).await,
        }
    }

    async fn process_receiver_event(&mut self, e: ReceiverState) {
        match e {
            ReceiverState::Created {
                id: name,
                descriptor,
                label,
                address,
            } => {
                self.receiver_created(name, label, descriptor, address)
                    .await
            }
            ReceiverState::Renamed { id, label } => self.receiver_renamed(id, label).await,
            ReceiverState::Destroyed { id: name } => self.receiver_destroyed(name).await,
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

    async fn sender_created(
        &mut self,
        name: String,
        label: String,
        descriptor: TxDescriptor,
        address: AudioBufferPointer,
    ) {
        let data = SenderData {
            config: descriptor.clone(),
            stats: SenderStats::default(),
            label,
            address,
            running: true,
        };
        self.senders.insert(name.clone(), data.clone());
        self.publish_sender(&name, data).await;
    }

    async fn sender_renamed(&mut self, qualified_id: String, label: String) {
        let Some(data) = self.senders.get_mut(&qualified_id) else {
            return;
        };
        data.label = label.clone();
        self.publish_sender_label(&qualified_id, label).await;
    }

    async fn sender_destroyed(&mut self, name: String) {
        self.senders.remove(&name);
        self.unpublish_sender(&name).await;
    }

    async fn receiver_created(
        &mut self,
        qualified_id: String,
        label: String,
        descriptor: RxDescriptor,
        address: AudioBufferPointer,
    ) {
        let data = ReceiverData {
            config: descriptor.clone(),
            stats: ReceiverStats::default(),
            label,
            address,
            running: true,
        };
        self.receivers.insert(qualified_id.clone(), data.clone());
        self.publish_receiver(&qualified_id, data).await;
    }

    async fn receiver_renamed(&mut self, qualified_id: String, label: String) {
        let Some(data) = self.receivers.get_mut(&qualified_id) else {
            return;
        };
        data.label = label.clone();
        self.publish_receiver_label(&qualified_id, label).await;
    }

    async fn receiver_destroyed(&mut self, qualified_id: String) {
        self.receivers.remove(&qualified_id);
        self.unpublish_receiver(&qualified_id).await;
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

    async fn receiver_clock_offset_changed(&mut self, qualified_id: String, offset: u64) {
        let Some(receiver) = self.receivers.get_mut(&qualified_id) else {
            return;
        };
        receiver.stats.clock_offset = offset;
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&qualified_id, stats).await;
    }

    async fn receiver_network_delay_changed(
        &mut self,
        qualified_id: String,
        delay_frames: i64,
        delay_millis: f32,
    ) {
        let Some(receiver) = self.receivers.get_mut(&qualified_id) else {
            return;
        };
        receiver.stats.network_delay_frames = delay_frames;
        receiver.stats.network_delay_millis = delay_millis;
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&qualified_id, stats).await;
    }

    async fn receiver_measured_link_offset_changed(
        &mut self,
        qualified_id: String,
        link_offset_frames: Frames,
        link_offset_ms: MilliSeconds,
    ) {
        let Some(receiver) = self.receivers.get_mut(&qualified_id) else {
            return;
        };
        receiver.stats.measured_link_offset_frames = link_offset_frames;
        receiver.stats.measured_link_offset_millis = link_offset_ms;
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&qualified_id, stats).await;
    }

    async fn receiver_lost_packets_changed(
        &mut self,
        qualified_id: String,
        lost_packets: usize,
        timestamp: SystemTime,
    ) {
        let Some(receiver) = self.receivers.get_mut(&qualified_id) else {
            return;
        };
        receiver.stats.lost_packets = LostPackets {
            count: lost_packets,
            last: Some(timestamp),
        };
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&qualified_id, stats).await;
    }

    async fn receiver_late_packets_changed(
        &mut self,
        qualified_id: String,
        late_packets: usize,
        timestamp: SystemTime,
    ) {
        let Some(receiver) = self.receivers.get_mut(&qualified_id) else {
            return;
        };
        receiver.stats.late_packets = LostPackets {
            count: late_packets,
            last: Some(timestamp),
        };
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&qualified_id, stats).await;
    }

    async fn receiver_muted_changed(&mut self, qualified_id: String, muted: bool) {
        let Some(receiver) = self.receivers.get_mut(&qualified_id) else {
            return;
        };
        receiver.stats.muted = muted;
        let stats = receiver.stats.clone();
        self.publish_receiver_stats(&qualified_id, stats).await;
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
        self.wb
            .set_async(topic!(self.client_name, "running"), true)
            .await
            .ok();
    }

    async fn publish_sender(&self, qualified_id: &str, data: SenderData) {
        publish_individual(&self.wb, topic!(qualified_id), data).await;
    }

    async fn publish_sender_config(&self, qualified_id: &str, config: TxDescriptor) {
        publish_individual(&self.wb, topic!(qualified_id, "config"), config).await;
    }

    async fn publish_sender_stats(&self, qualified_id: &str, stats: SenderStats) {
        publish_individual(&self.wb, topic!(qualified_id, "stats"), stats).await;
    }

    async fn publish_sender_label(&self, qualified_id: &str, label: String) {
        publish_individual(&self.wb, topic!(qualified_id, "label"), label).await;
    }

    async fn unpublish_sender(&mut self, qualified_id: &str) {
        self.wb
            .pdelete_async(topic!(qualified_id, "#"), true)
            .await
            .ok();
    }

    async fn publish_receiver(&self, qualified_id: &str, data: ReceiverData) {
        publish_individual(&self.wb, topic!(qualified_id), data).await;
    }

    async fn publish_receiver_config(&self, qualified_id: &str, config: RxDescriptor) {
        publish_individual(&self.wb, topic!(qualified_id, "config"), config).await;
    }

    async fn publish_receiver_stats(&self, qualified_id: &str, stats: ReceiverStats) {
        publish_individual(&self.wb, topic!(qualified_id, "stats"), stats).await;
    }

    async fn publish_receiver_label(&self, qualified_id: &str, label: String) {
        publish_individual(&self.wb, topic!(qualified_id, "label"), label).await;
    }

    async fn unpublish_receiver(&self, qualified_id: &str) {
        self.wb
            .pdelete_async(topic!(qualified_id, "#"), true)
            .await
            .ok();
    }
}
