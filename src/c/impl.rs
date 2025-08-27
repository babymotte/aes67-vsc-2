use crate::{
    AES_VSC_ERROR_CLOCK_SYNC_ERROR, AES_VSC_ERROR_INVALID_CHANNEL, AES_VSC_ERROR_NO_DATA,
    AES_VSC_ERROR_RECEIVER_BUFFER_UNDERRUN, AES_VSC_ERROR_RECEIVER_NOT_FOUND,
    AES_VSC_ERROR_RECEIVER_NOT_READY_YET, AES_VSC_OK, Aes67VscReceiverConfig,
    config::Config,
    error::{Aes67Vsc2Error, Aes67Vsc2Result},
    receiver::{
        api::{DataState, ReceiverApi},
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

fn init_vsc() -> Aes67Vsc2Result<Arc<VirtualSoundCardApi>> {
    try_init()?;
    let vsc_name = env::var("AES67_VSC_NAME").unwrap_or("aes67-virtual-sound-card".to_owned());
    info!("Creating new VSC with name '{vsc_name}' …");
    let vsc = VirtualSoundCardApi::new(vsc_name.clone())?;
    info!("VSC '{}' created.", vsc_name);
    Ok(Arc::new(vsc))
}

fn try_init() -> Aes67Vsc2Result<()> {
    let runtime = runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let init_future = async {
        let config = Config::load().await?;
        telemetry::init(&config).await?;
        Ok::<(), Aes67Vsc2Error>(())
    };
    runtime.block_on(init_future)?;
    info!("AES67 VSC subsystem initialized successfully.");
    Ok(())
}

impl<'a> TryFrom<&Aes67VscReceiverConfig<'a>> for ReceiverConfig {
    type Error = Aes67Vsc2Error;
    fn try_from(value: &Aes67VscReceiverConfig<'a>) -> Aes67Vsc2Result<Self> {
        let id = value.id.to_string();
        let session = SessionDescription::unmarshal(&mut Cursor::new(value.sdp.to_str()))
            .map_err(|e| Aes67Vsc2Error::InvalidSdp(e.to_string()))?;
        let link_offset = value.link_offset;
        let buffer_time = value.buffer_time;
        let delay_calculation_interval = value.delay_calculation_interval.map(ToOwned::to_owned);
        let interface_ip = value.interface_ip.to_str().parse()?;

        Ok(ReceiverConfig {
            id,
            session,
            link_offset,
            buffer_time,
            delay_calculation_interval,
            interface_ip,
        })
    }
}

pub fn try_create_receiver(
    name: char_p::Ref<'_>,
    config: &Aes67VscReceiverConfig,
) -> Aes67Vsc2Result<i32> {
    let receiver_id = name.to_string();
    let config = match ReceiverConfig::try_from(config) {
        Ok(it) => it,
        Err(err) => return Ok(-(err.error_code() as i32)),
    };
    let (receiver_api, id) = match VIRTUAL_SOUND_CARD.create_receiver(receiver_id, config) {
        Ok(it) => it,
        Err(err) => return Ok(-(err.error_code() as i32)),
    };
    RECEIVERS.insert(id, receiver_api);
    Ok(id as i32)
}

pub fn try_receive(
    receiver_id: u32,
    playout_time: u64,
    buffer_ptr: usize,
    buffer_len: usize,
) -> Aes67Vsc2Result<u8> {
    let Some(receiver) = RECEIVERS.get(&receiver_id) else {
        return Ok(AES_VSC_ERROR_RECEIVER_NOT_FOUND);
    };

    match receiver.receive_all(playout_time, buffer_ptr, buffer_len)? {
        DataState::Ready => Ok(AES_VSC_OK),
        DataState::NotReady => Ok(AES_VSC_ERROR_NO_DATA),
        DataState::ReceiverNotReady => Ok(AES_VSC_ERROR_RECEIVER_NOT_READY_YET),
        DataState::InvalidChannelNumber => Ok(AES_VSC_ERROR_INVALID_CHANNEL),
        DataState::Missed => Ok(AES_VSC_ERROR_RECEIVER_BUFFER_UNDERRUN),
        DataState::SyncError => Ok(AES_VSC_ERROR_CLOCK_SYNC_ERROR),
    }
}

pub fn try_destroy_receiver(id: u32) -> Aes67Vsc2Result<u8> {
    VIRTUAL_SOUND_CARD.destroy_receiver(id)?;
    Ok(AES_VSC_OK)
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
