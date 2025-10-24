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

use sdp::SessionDescription;
use serde::{Deserialize, Deserializer, Serializer};
use std::io::Cursor;
use tracing::instrument;

#[instrument(skip(deserializer))]
pub fn deserialize_sdp<'de, D>(deserializer: D) -> Result<SessionDescription, D::Error>
where
    D: Deserializer<'de>,
{
    let sdp = String::deserialize(deserializer)?;
    SessionDescription::unmarshal(&mut Cursor::new(&sdp)).map_err(serde::de::Error::custom)
}

// fn fetch_sdp<'de, D>(s: String) -> Result<String, D::Error>
// where
//     D: Deserializer<'de>,
// {
//     let sdp = reqwest::blocking::get(s)
//         .map_err(serde::de::Error::custom)?
//         .text()
//         .map_err(serde::de::Error::custom)?;
//     Ok(sdp)
// }

#[instrument(skip(serializer))]
pub fn serialize_sdp<S>(value: &SessionDescription, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.marshal())
}
