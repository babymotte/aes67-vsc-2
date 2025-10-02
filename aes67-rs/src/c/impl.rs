use crate::{
    AES_VSC_ERROR_RECEIVER_NOT_FOUND, AES_VSC_OK, Aes67VscReceiverConfig,
    config::{Config, PtpMode},
    error::{
        ConfigError, ConfigResult, GetErrorCode, ReceiverApiResult, ReceiverInternalResult,
        ToBoxedResult, VscApiResult, VscInternalError, VscInternalResult,
    },
    receiver::{
        api::ReceiverApi,
        config::ReceiverConfig,
    },
    telemetry,
    vsc::VirtualSoundCardApi,
};
use ::safer_ffi::prelude::*;
use dashmap::DashMap;
use lazy_static::lazy_static;
use sdp::SessionDescription;
use std::{env, io::Cursor, sync::Arc};
use tokio::runtime;
use tracing::info;

lazy_static! {
    static ref VIRTUAL_SOUND_CARD: Arc<VirtualSoundCardApi> =
        init_vsc().expect("failed to initialized AES67 virtual sound card");
    static ref RECEIVERS: DashMap<u32, ReceiverApi> = DashMap::new();
}

fn init_vsc() -> VscApiResult<Arc<VirtualSoundCardApi>> {
    try_init().boxed()?;
    let vsc_name = env::var("AES67_VSC_NAME").unwrap_or("aes67-virtual-sound-card".to_owned());
    info!("Creating new VSC with name '{vsc_name}' …");
    let vsc = VirtualSoundCardApi::new_blocking(vsc_name.clone())?;
    info!("VSC '{}' created.", vsc_name);
    Ok(Arc::new(vsc))
}

fn try_init() -> VscInternalResult<()> {
    let config = Config::load()?;
    let runtime = runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let init_future = async {
        telemetry::init(&config).await?;
        Ok::<(), VscInternalError>(())
    };
    runtime.block_on(init_future)?;
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
    let (receiver_api, id) = match VIRTUAL_SOUND_CARD.create_receiver_blocking(config, ptp_mode) {
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
    VIRTUAL_SOUND_CARD.destroy_receiver_blocking(id)?;
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
    ::safer_ffi::headers::builder()
        .with_text_after_guard(
            "static const unsigned int AES_VSC_OK = 0x00;
static const unsigned int AES_VSC_ERROR_NOT_INITIALIZED = 0x01;
static const unsigned int AES_VSC_ERROR_ALREADY_INITIALIZED = 0x02;
static const unsigned int AES_VSC_ERROR_UNSUPPORTED_BIT_DEPTH = 0x03;
static const unsigned int AES_VSC_ERROR_UNSUPPORTED_SAMPLE_RATE = 0x04;
static const unsigned int AES_VSC_ERROR_VSC_NOT_CREATED = 0x05;
static const unsigned int AES_VSC_ERROR_RECEIVER_NOT_FOUND = 0x06;
static const unsigned int AES_VSC_ERROR_SENDER_NOT_FOUND = 0x07;
static const unsigned int AES_VSC_ERROR_INVALID_CHANNEL = 0x08;
static const unsigned int AES_VSC_ERROR_RECEIVER_BUFFER_UNDERRUN = 0x09;
static const unsigned int AES_VSC_ERROR_CLOCK_SYNC_ERROR = 0x0A;
static const unsigned int AES_VSC_ERROR_RECEIVER_NOT_READY_YET = 0x0B;
static const unsigned int AES_VSC_ERROR_NO_DATA = 0x0C;",
        )
        .to_file("./include/aes67-vsc-2.h")?
        .generate()
}
