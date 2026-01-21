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

use ::safer_ffi::prelude::*;
use aes67_rs::{
    config::Config,
    error::{
        ConfigError, ConfigResult, GetErrorCode, ReceiverApiResult, ReceiverInternalResult,
        ToBoxedResult, VscApiResult, VscInternalError, VscInternalResult,
    },
    nic::find_nic_with_name,
    receiver::{api::ReceiverApi, config::ReceiverConfig},
    time::get_clock,
    vsc::VirtualSoundCardApi,
};
use aes67_rs_sdp::SdpWrapper;
use dashmap::DashMap;
use futures_lite::future::block_on;
use lazy_static::lazy_static;
use sdp::SessionDescription;
use std::{env, io::Cursor, sync::Arc};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{AES_VSC_ERROR_RECEIVER_NOT_FOUND, AES_VSC_OK, Aes67VscReceiverConfig};

lazy_static! {
    static ref VIRTUAL_SOUND_CARD: Arc<VirtualSoundCardApi> =
        init_vsc().expect("failed to initialized AES67 virtual sound card");
    static ref RECEIVERS: DashMap<u32, ReceiverApi> = DashMap::new();
}

fn init_vsc() -> VscApiResult<Arc<VirtualSoundCardApi>> {
    let config = try_init().boxed()?;
    let (wb, _, _) = block_on(worterbuch_client::connect_with_default_config())
        .map_err(VscInternalError::from)
        .boxed()?;
    let audio_nic = find_nic_with_name(config.audio.nic)?;
    let vsc_name = env::var("AES67_VSC_NAME").unwrap_or("aes67-virtual-sound-card".to_owned());
    info!("Creating new VSC with name '{vsc_name}' …");
    let shutdown_token = CancellationToken::new();
    let clock = block_on(get_clock(
        vsc_name.clone(),
        config.ptp,
        config.audio.sample_rate,
        wb.clone(),
    ))?;
    let vsc = block_on(VirtualSoundCardApi::new(
        vsc_name.clone(),
        shutdown_token,
        wb,
        clock,
        audio_nic,
    ))?;
    info!("VSC '{}' created.", vsc_name);
    Ok(Arc::new(vsc))
}

fn try_init() -> VscInternalResult<Config> {
    // let config = Config::load()?;
    //
    let config = Config::default();
    // let runtime = runtime::Builder::new_current_thread()
    //     .enable_all()
    //     .build()?;
    let init_future = async {
        // telemetry::init(&config).await?;
        Ok::<(), VscInternalError>(())
    };
    // runtime.block_on(init_future)?;
    block_on(init_future)?;
    info!("AES67 VSC subsystem initialized successfully.");
    Ok(config)
}

impl<'a> TryFrom<&Aes67VscReceiverConfig<'a>> for ReceiverConfig {
    type Error = ConfigError;
    fn try_from(value: &Aes67VscReceiverConfig<'a>) -> ConfigResult<Self> {
        let id = value.id;
        let label = value
            .name
            .map(|it| it.to_string())
            .unwrap_or_else(|| id.to_string());
        let session = SdpWrapper(
            SessionDescription::unmarshal(&mut Cursor::new(value.sdp.to_str()))
                .map_err(|e| ConfigError::InvalidSdp(e.to_string()))?,
        );
        let link_offset = value.link_offset;
        let delay_calculation_interval: Option<()> = None;

        todo!()
        // Ok(ReceiverConfig {
        //     id,
        //     label,
        //     session,
        //     link_offset,
        //     delay_calculation_interval,
        // })
    }
}

pub fn try_create_receiver(config: &Aes67VscReceiverConfig) -> ReceiverInternalResult<i32> {
    let config = match ReceiverConfig::try_from(config) {
        Ok(it) => it,
        Err(err) => return Ok(-(err.error_code() as i32)),
    };
    let (receiver_api, _) = match block_on(VIRTUAL_SOUND_CARD.create_receiver(config)) {
        Ok(it) => it,
        Err(err) => return Ok(-(err.error_code() as i32)),
    };
    RECEIVERS.insert(0, receiver_api);
    Ok(0)
}

pub fn try_receive<'a>(
    receiver_id: u32,
    _playout_time: u64,
    _buffer_ptr: c_slice::Mut<'a, f32>,
) -> ReceiverApiResult<u8> {
    let Some(_receiver) = RECEIVERS.get(&receiver_id) else {
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
