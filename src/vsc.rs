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
    receiver::{
        config::{ReceiverConfig, RxDescriptor},
        start_receiver,
    },
    time::SystemMediaClock,
};
use std::thread;
use tokio::{
    runtime,
    sync::{mpsc, oneshot},
};
use tracing::info;

enum VscApiMessage {
    CreateReceiver(String, ReceiverConfig, oneshot::Sender<Aes67Vsc2Result<()>>),
    DestroyReceiver(String, oneshot::Sender<Aes67Vsc2Result<()>>),
    Stop(oneshot::Sender<()>),
}

pub struct VirtualSoundCardApi {
    api_tx: mpsc::Sender<VscApiMessage>,
}

impl VirtualSoundCardApi {
    pub fn new(id: i32) -> Aes67Vsc2Result<Self> {
        let (result_tx, result_rx) = oneshot::channel();
        let (api_tx, api_rx) = mpsc::channel(1024);
        thread::Builder::new()
            .name(format!("aes67-vsc-{id}"))
            .spawn(move || {
                let vsc_future = VirtualSoundCard::new(id, api_rx).run();
                let runtime = match runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(it) => it,
                    Err(e) => {
                        result_tx.send(Err(Aes67Vsc2Error::from(e))).ok();
                        return;
                    }
                };
                result_tx.send(Ok(())).ok();
                runtime.block_on(vsc_future);
            })?;
        result_rx.blocking_recv()??;
        Ok(VirtualSoundCardApi { api_tx })
    }

    pub fn create_receiver(&self, id: String, config: ReceiverConfig) -> Aes67Vsc2Result<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .blocking_send(VscApiMessage::CreateReceiver(id, config, tx))
            .ok();
        rx.blocking_recv()?
    }

    pub fn destroy_receiver(&self, id: String) -> Aes67Vsc2Result<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx
            .blocking_send(VscApiMessage::DestroyReceiver(id, tx))
            .ok();
        rx.blocking_recv()?
    }

    pub fn close(self) -> Aes67Vsc2Result<()> {
        let (tx, rx) = oneshot::channel();
        self.api_tx.blocking_send(VscApiMessage::Stop(tx)).ok();
        Ok(rx.blocking_recv()?)
    }
}

struct VirtualSoundCard {
    id: i32,
    api_rx: mpsc::Receiver<VscApiMessage>,
}

impl VirtualSoundCard {
    fn new(id: i32, api_rx: mpsc::Receiver<VscApiMessage>) -> Self {
        VirtualSoundCard { id, api_rx }
    }

    async fn run(mut self) {
        let vsc_id = self.id.clone();

        while let Some(msg) = self.api_rx.recv().await {
            match msg {
                VscApiMessage::CreateReceiver(id, config, tx) => {
                    tx.send(self.create_receiver(id, config).await).ok();
                }
                VscApiMessage::DestroyReceiver(id, tx) => {
                    tx.send(self.destroy_receiver(id).await).ok();
                }
                VscApiMessage::Stop(tx) => {
                    info!("Stopping Virtual sound card '{vsc_id}' …");
                    drop(self);
                    tx.send(()).ok();
                    break;
                }
            }
        }
    }

    async fn create_receiver(&mut self, id: String, config: ReceiverConfig) -> Aes67Vsc2Result<()> {
        info!("Creating receiver '{id}' …");

        let desc = RxDescriptor::try_from(&config)?;
        let clock = SystemMediaClock::new(desc.audio_format.clone());
        start_receiver(format!("receiver-{id}"), config, None, clock).await?;

        info!("Receiver '{id}' successfully created.");
        Ok(())
    }

    async fn destroy_receiver(&mut self, id: String) -> Aes67Vsc2Result<()> {
        info!("Destroying receiver '{id}' …");
        // TODO
        info!("Receiver '{id}' successfully destroyed.");

        Ok(())
    }
}

impl Drop for VirtualSoundCard {
    fn drop(&mut self) {
        info!("Virtual sound card '{}' stopped.", self.id);
    }
}
