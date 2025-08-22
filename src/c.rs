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

//! This module implements a public C API that can be loaded as a shared object / dynamically linked library

mod r#impl;

use crate::c::r#impl::{
    try_create_receiver, try_create_vsc, try_destroy_receiver, try_destroy_vsc, try_init,
};
use ::safer_ffi::prelude::*;

#[cfg(feature = "headers")]
pub use r#impl::generate_headers;

pub const AES_VSC_OK: u8 = 0x00;
pub const AES_VSC_ERROR_ALREADY_INITIALIZED: u8 = 0x01;
pub const AES_VSC_ERROR_UNSUPPORTED_BIT_DEPTH: u8 = 0x02;
pub const AES_VSC_ERROR_UNSUPPORTED_SAMPLE_RATE: u8 = 0x03;
pub const AES_VSC_ERROR_VSC_NOT_FOUND: u8 = 0x04;
pub const AES_VSC_ERROR_MUTEX_POISONED: u8 = 0x05;

/// A unique reference to a virtual sound card
#[derive_ReprC]
#[repr(C)]
pub struct Aes67VscVirtualSoundCard {
    id: u32,
}

#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Aes67VscReceiverConfig<'a> {
    id: char_p::Ref<'a>,
    sdp: char_p::Ref<'a>,
    link_offset: f32,
    buffer_time: f32,
    delay_calculation_interval: Option<&'a u32>,
    interface_ip: char_p::Ref<'a>,
}

/// Initialize the VSC subsystem. This only needs to be called once, any subsequent calls will be ignored.
#[ffi_export]
fn aes67_vsc_init() -> u8 {
    match try_init() {
        Ok(_) => AES_VSC_OK,
        Err(err) => err.error_code(),
    }
}

/// Create a new virtual sound card.
/// The sound card can then be used to create senders and receivers and get stats and monitoring.
///
/// While technically possible, it is generally not recommended to create more than one sound card
/// at the same time to avoid allocating resources unnecessarily.
#[ffi_export]
fn aes67_vsc_create_vsc() -> i32 {
    match try_create_vsc() {
        Ok(it) => it,
        Err(err) => -(err.error_code() as i32),
    }
}

/// Destroy a virtual sound card. This will stop all senders and receivers that were created on
/// this sound card and de-allocate all memory that was allocated by it.
#[ffi_export]
fn aes67_vsc_destroy_vsc(vsc: &i32) -> u8 {
    match try_destroy_vsc(vsc) {
        Ok(_) => AES_VSC_OK,
        Err(err) => err.error_code(),
    }
}

/// Create a new AES67 receiver
/// * `id` - A string pointer to the receiver ID, which must be unique within this process
/// * `audio_format` - The receiver's audio format
#[ffi_export]
fn aes67_vsc_create_receiver<'a>(
    vsc: &i32,
    id: char_p::Ref<'_>,
    config: &'a Aes67VscReceiverConfig<'a>,
) -> u8 {
    match try_create_receiver(vsc, id, config) {
        Ok(_) => AES_VSC_OK,
        Err(err) => err.error_code(),
    }
}

/// Destroy an existing AES67 receiver. Destroying a receiver will stop it from receiving any
/// more audio packets and filling the assigned buffer. It will also de-allocate any memory the
/// receiver has allocated during its creation.
///
/// * `vsc` - the virtual soundcard on which to destroy the receiver
/// * `id` - the ID of the receiver to be destroyed
#[ffi_export]
fn aes67_vsc_destroy_receiver(vsc: &i32, id: char_p::Ref<'_>) -> u8 {
    match try_destroy_receiver(vsc, id) {
        Ok(_) => AES_VSC_OK,
        Err(err) => err.error_code(),
    }
}
