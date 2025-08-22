use crate::{
    AES_VSC_ERROR_ALREADY_INITIALIZED, AES_VSC_ERROR_MUTEX_POISONED, AES_VSC_ERROR_VSC_NOT_FOUND,
    AES_VSC_OK, Aes67VscReceiverConfig,
    config::Config,
    error::{Aes67Vsc2Error, Aes67Vsc2Result},
    receiver::config::ReceiverConfig,
    telemetry,
    vsc::VirtualSoundCardApi,
};
use ::safer_ffi::prelude::*;
use lazy_static::lazy_static;
use sdp::SessionDescription;
use std::{
    collections::HashMap,
    io::Cursor,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};
use tokio::runtime;
use tracing::{info, warn};

lazy_static! {
    static ref INITIALIZED: AtomicBool = AtomicBool::new(false);
    static ref VSCS: Arc<Mutex<HashMap<i32, VirtualSoundCardApi>>> = Arc::default();
    static ref VSC_IDS: AtomicI32 = AtomicI32::new(0);
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

pub fn try_init() -> Aes67Vsc2Result<u8> {
    let already_initialized = INITIALIZED.swap(true, Ordering::AcqRel);
    if already_initialized {
        warn!("VSC subsystem is already initialized!");
        return Ok(AES_VSC_ERROR_ALREADY_INITIALIZED);
    }

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

    Ok(AES_VSC_OK)
}

pub fn try_create_vsc() -> Aes67Vsc2Result<i32> {
    let id = get_next_id();
    info!("Creating new VSC with id {id} …");
    let Ok(mut lock) = VSCS.lock() else {
        return Ok(-(AES_VSC_ERROR_MUTEX_POISONED as i32));
    };
    let vsc = match VirtualSoundCardApi::new(id) {
        Ok(it) => it,
        Err(e) => return Ok(-(e.error_code() as i32)),
    };
    lock.insert(id, vsc);
    info!("VSC '{id}' created.");
    Ok(id as i32)
}

pub fn try_destroy_vsc(vsc: &i32) -> Aes67Vsc2Result<u8> {
    let vsc_id = *vsc;
    info!("Destroying VSC '{}' …", vsc_id);

    let mut lock = VSCS.lock().expect("mutex guard on VSCS is poisoned");
    let Some(vsc) = lock.remove(&vsc_id) else {
        return Ok(AES_VSC_ERROR_VSC_NOT_FOUND);
    };

    vsc.close()?;

    info!("VSC '{}' destroyed.", vsc_id);
    Ok(AES_VSC_OK)
}

pub fn try_create_receiver(
    vsc: &i32,
    id: char_p::Ref<'_>,
    config: &Aes67VscReceiverConfig,
) -> Aes67Vsc2Result<u8> {
    let vsc_id = *vsc;
    let receiver_id = id.to_string();
    let config = ReceiverConfig::try_from(config)?;

    let mut lock = VSCS.lock().expect("mutex guard on VSCS is poisoned");
    let Some(vsc) = lock.get_mut(&vsc_id) else {
        return Ok(AES_VSC_ERROR_VSC_NOT_FOUND);
    };

    vsc.create_receiver(receiver_id, config)?;
    Ok(AES_VSC_OK)
}

pub fn try_destroy_receiver(vsc: &i32, id: char_p::Ref<'_>) -> Aes67Vsc2Result<u8> {
    let vsc_id = *vsc;
    let receiver_id = id.to_string();

    let mut lock = VSCS.lock().expect("mutex guard on VSCS is poisoned");
    let Some(vsc) = lock.get_mut(&vsc_id) else {
        return Ok(AES_VSC_ERROR_VSC_NOT_FOUND);
    };

    vsc.destroy_receiver(receiver_id)?;
    Ok(AES_VSC_OK)
}

fn get_next_id() -> i32 {
    VSC_IDS.fetch_add(1, Ordering::SeqCst)
}

// The following function is only necessary for the header generation.
#[cfg(feature = "headers")] // c.f. the `Cargo.toml` section
pub fn generate_headers() -> ::std::io::Result<()> {
    ::safer_ffi::headers::builder()
        .to_file("./include/aes67-vsc-2.h")?
        .generate()
}
