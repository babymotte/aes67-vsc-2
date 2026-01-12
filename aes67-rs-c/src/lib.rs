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

use crate::r#impl::{try_create_receiver, try_destroy_receiver, try_receive};
use ::safer_ffi::prelude::*;
use aes67_rs::error::GetErrorCode;

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
pub const AES_VSC_ERROR_NO_DATA: u8 = 0x0C;
pub const AES_VSC_ERROR_INVALID_PTP_CONFIG: u8 = 0x0D;

/// Configuration for an AES67 receiver
#[derive_ReprC]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Aes67VscReceiverConfig<'a> {
    /// Unique receiver ID
    id: u32,
    /// Name of the receiver. Technically this does not have to be unique but stats are reported by receiver name,
    /// so giving the same name to multiple receivers at the same time will make those hard to interpret.
    name: Option<char_p::Ref<'a>>,
    /// The content of the SDP file of the sender that this receiver should subscribe to.
    sdp: char_p::Ref<'a>,
    /// Link offset in milliseconds
    link_offset: f32,
}

/// Create a new AES67 receiver
/// * `config` - the configuration for the sender
#[ffi_export]
fn aes67_vsc_create_receiver<'a>(config: &'a Aes67VscReceiverConfig<'a>) -> i32 {
    eprintln!("config: {:?}", config);
    match try_create_receiver(config) {
        Ok(it) => it,
        Err(err) => -(err.error_code() as i32),
    }
}

/// Fetch data from the specified receiver
///
/// * `receiver_id` - the receiver id as returned by the `aes67_vsc_create_receiver` function
/// * `playout_time` - the media clock timestamp of the first frame to fetch
/// * `buffer_ptr` - pointer to a float[] to which the fetched audio samples will be written
#[ffi_export]
fn aes67_vsc_receive<'a>(
    receiver_id: u32,
    playout_time: u64,
    buffer_ptr: c_slice::Mut<'a, f32>,
) -> u8 {
    match try_receive(receiver_id, playout_time, buffer_ptr) {
        Ok(it) => it,
        Err(err) => err.error_code(),
    }
}

/// Destroy an existing AES67 receiver. Destroying a receiver will stop it from receiving any
/// more audio packets and filling the assigned buffer. It will also de-allocate any memory the
/// receiver has allocated during its creation.
///
/// * `receiver_id` - the ID of the receiver to be destroyed
#[ffi_export]
fn aes67_vsc_destroy_receiver(receiver_id: u32) -> u8 {
    match try_destroy_receiver(receiver_id) {
        Ok(it) => it,
        Err(err) => err.error_code(),
    }
}
