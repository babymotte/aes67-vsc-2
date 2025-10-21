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
    AES_VSC_ERROR_RECEIVER_NOT_FOUND, AES_VSC_OK, Aes67VscReceiverConfig,
    config::{Config, PtpMode},
    error::{
        ConfigError, ConfigResult, GetErrorCode, ReceiverApiResult, ReceiverInternalResult,
        ToBoxedResult, VscApiResult, VscInternalError, VscInternalResult,
    },
    receiver::{api::ReceiverApi, config::ReceiverConfig},
    telemetry,
    vsc::VirtualSoundCardApi,
};
use ::safer_ffi::prelude::*;
use dashmap::DashMap;
use futures_lite::future::block_on;
use lazy_static::lazy_static;
use sdp::SessionDescription;
use std::{env, io::Cursor, sync::Arc};
use tokio_util::sync::CancellationToken;
use tracing::info;

lazy_static! {
    static ref VIRTUAL_SOUND_CARD: Arc<VirtualSoundCardApi> =
        init_vsc().expect("failed to initialized AES67 virtual sound card");
    static ref RECEIVERS: DashMap<u32, ReceiverApi> = DashMap::new();
}

fn init_vsc() -> VscApiResult<Arc<VirtualSoundCardApi>> {
    try_init().boxed()?;
    let (wb, _, _) = block_on(worterbuch_client::connect_with_default_config())
        .map_err(VscInternalError::from)
        .boxed()?;
    let vsc_name = env::var("AES67_VSC_NAME").unwrap_or("aes67-virtual-sound-card".to_owned());
    info!("Creating new VSC with name '{vsc_name}' …");
    let shutdown_token = CancellationToken::new();
    let vsc = block_on(VirtualSoundCardApi::new(
        vsc_name.clone(),
        shutdown_token,
        wb,
    ))?;
    info!("VSC '{}' created.", vsc_name);
    Ok(Arc::new(vsc))
}

fn try_init() -> VscInternalResult<()> {
    let config = Config::load()?;
    // let runtime = runtime::Builder::new_current_thread()
    //     .enable_all()
    //     .build()?;
    let init_future = async {
        telemetry::init(&config).await?;
        Ok::<(), VscInternalError>(())
    };
    // runtime.block_on(init_future)?;
    block_on(init_future)?;
    info!("AES67 VSC subsystem initialized successfully.");
    Ok(())
}

impl<'a> TryFrom<&Aes67VscReceiverConfig<'a>> for ReceiverConfig {
    type Error = ConfigError;
    fn try_from(value: &Aes67VscReceiverConfig<'a>) -> ConfigResult<Self> {
        let id = value.name.map(|it| it.to_string());
        let session = SessionDescription::unmarshal(&mut Cursor::new(value.sdp.to_str()))
            .map_err(|e| ConfigError::InvalidSdp(e.to_string()))?;
        let link_offset = value.link_offset;
        let delay_calculation_interval = None;
        let interface_ip = value.interface_ip.to_str().parse()?;

        Ok(ReceiverConfig {
            id,
            session,
            link_offset,
            delay_calculation_interval,
            interface_ip,
        })
    }
}

pub fn try_create_receiver(
    config: &Aes67VscReceiverConfig,
    ptp_mode: Option<PtpMode>,
) -> ReceiverInternalResult<i32> {
    let config = match ReceiverConfig::try_from(config) {
        Ok(it) => it,
        Err(err) => return Ok(-(err.error_code() as i32)),
    };
    let (receiver_api, _, id) = match block_on(VIRTUAL_SOUND_CARD.create_receiver(config, ptp_mode))
    {
        Ok(it) => it,
        Err(err) => return Ok(-(err.error_code() as i32)),
    };
    RECEIVERS.insert(id, receiver_api);
    Ok(id as i32)
}

pub fn try_receive<'a>(
    receiver_id: u32,
    playout_time: u64,
    buffer_ptr: c_slice::Mut<'a, f32>,
) -> ReceiverApiResult<u8> {
    let Some(receiver) = RECEIVERS.get(&receiver_id) else {
        return Ok(AES_VSC_ERROR_RECEIVER_NOT_FOUND);
    };

    // match receiver.receive_all(playout_time, buffer_ptr.as_ptr() as usize, buffer_ptr.len())? {
    //     DataState::Ready => Ok(AES_VSC_OK),
    //     DataState::NotReady => Ok(AES_VSC_ERROR_NO_DATA),
    //     DataState::ReceiverNotReady => Ok(AES_VSC_ERROR_RECEIVER_NOT_READY_YET),
    //     DataState::InvalidChannelNumber => Ok(AES_VSC_ERROR_INVALID_CHANNEL),
    //     DataState::Missed => Ok(AES_VSC_ERROR_RECEIVER_BUFFER_UNDERRUN),
    //     DataState::SyncError => Ok(AES_VSC_ERROR_CLOCK_SYNC_ERROR),
    // }

    // TODO adapt to new receiver API

    Ok(AES_VSC_OK)
}

pub fn try_destroy_receiver(id: u32) -> VscApiResult<u8> {
    block_on(VIRTUAL_SOUND_CARD.destroy_receiver(id))?;
    Ok(AES_VSC_OK)
}

impl TryFrom<(Option<&char_p::Ref<'_>>, Option<&char_p::Ref<'_>>)> for PtpMode {
    type Error = ConfigError;

    fn try_from(
        value: (Option<&char_p::Ref<'_>>, Option<&char_p::Ref<'_>>),
    ) -> Result<Self, Self::Error> {
        todo!()
    }
}

// The following function is only necessary for the header generation.
#[cfg(feature = "headers")] // c.f. the `Cargo.toml` section
pub fn generate_headers() -> ::std::io::Result<()> {
    use safer_ffi::headers::Language;

    ::safer_ffi::headers::builder()
        .with_language(Language::C)
        .with_text_after_guard("#include \"./aes67-vsc-2-constants.h\"")
        .to_file("./include/aes67-vsc-2.h")?
        .generate()
}
