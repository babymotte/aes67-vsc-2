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

use crate::c::r#impl::{try_create_receiver, try_destroy_receiver, try_receive};
use ::safer_ffi::prelude::*;

#[cfg(feature = "headers")]
pub use r#impl::generate_headers;

pub const AES_VSC_OK: u8 = 0x00;
pub const AES_VSC_ERROR_NOT_INITIALIZED: u8 = 0x01;
pub const AES_VSC_ERROR_ALREADY_INITIALIZED: u8 = 0x02;
pub const AES_VSC_ERROR_UNSUPPORTED_BIT_DEPTH: u8 = 0x03;
pub const AES_VSC_ERROR_UNSUPPORTED_SAMPLE_RATE: u8 = 0x04;
pub const AES_VSC_ERROR_VSC_NOT_CREATED: u8 = 0x05;
pub const AES_VSC_ERROR_RECEIVER_NOT_FOUND: u8 = 0x06;
pub const AES_VSC_ERROR_SENDER_NOT_FOUND: u8 = 0x07;
pub const AES_VSC_ERROR_INVALID_CHANNEL: u8 = 0x08;
pub const AES_VSC_ERROR_RECEIVER_BUFFER_UNDERRUN: u8 = 0x09;
pub const AES_VSC_ERROR_CLOCK_SYNC_ERROR: u8 = 0x0A;
pub const AES_VSC_ERROR_RECEIVER_NOT_READY_YET: u8 = 0x0B;

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

/// Create a new AES67 receiver
/// * `id` - A string pointer to the receiver ID, which must be unique within the process (not within the virtual sound card!)
/// * `audio_format` - The receiver's audio format
#[ffi_export]
fn aes67_vsc_create_receiver<'a>(
    receiver_name: char_p::Ref<'_>,
    config: &'a Aes67VscReceiverConfig<'a>,
) -> i32 {
    match try_create_receiver(receiver_name, config) {
        Ok(it) => it,
        Err(err) => -(err.error_code() as i32),
    }
}

#[ffi_export]
fn aes67_vsc_receive(
    receiver_id: u32,
    playout_time: u64,
    buffer_ptr: usize,
    buffer_len: usize,
) -> u8 {
    match try_receive(receiver_id, playout_time, buffer_ptr, buffer_len) {
        Ok(it) => it,
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
fn aes67_vsc_destroy_receiver(receiver_id: u32) -> u8 {
    match try_destroy_receiver(receiver_id) {
        Ok(it) => it,
        Err(err) => err.error_code(),
    }
}
