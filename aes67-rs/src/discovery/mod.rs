mod sap;

pub use sap::*;

use crate::serde::SdpWrapper;
use serde::{Deserialize, Serialize};
use std::{hash::Hash, time::SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub description: SdpWrapper,
    pub timestamp: SystemTime,
}

impl Hash for Session {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.description.origin.session_id.hash(state);
        self.description.origin.session_version.hash(state);
    }
}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.description.origin.session_id == other.description.origin.session_id
            && self.description.origin.session_version == other.description.origin.session_version
            && self.timestamp == other.timestamp
    }
}

impl Eq for Session {}

impl PartialOrd for Session {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(other.timestamp.cmp(&self.timestamp))
    }
}

impl Ord for Session {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.timestamp.cmp(&self.timestamp)
    }
}
