use std::io::Cursor;

use sdp::SessionDescription;
use serde::{Deserialize, Deserializer, Serializer};
use tracing::instrument;

#[instrument(skip(deserializer))]
pub fn deserialize_sdp<'de, D>(deserializer: D) -> Result<SessionDescription, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    SessionDescription::unmarshal(&mut Cursor::new(&s)).map_err(serde::de::Error::custom)
}

#[instrument(skip(serializer))]
pub fn serialize_sdp<S>(value: &SessionDescription, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.marshal())
}
