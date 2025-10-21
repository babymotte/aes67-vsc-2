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

pub mod app;
pub mod buffer;
pub mod config;
pub mod discovery;
pub mod error;
pub mod formats;
pub mod monitoring;
pub mod nic;
pub mod receiver;
pub mod sender;
pub mod serde;
pub mod socket;
pub mod telemetry;
pub mod time;
pub mod utils;
pub mod vsc;

#[cfg(feature = "c")]
pub mod c;
#[cfg(feature = "c")]
pub use c::*;
