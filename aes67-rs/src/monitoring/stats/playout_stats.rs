use crate::monitoring::{PoStats, Report};
use tokio::sync::mpsc;

pub struct PlayoutStats {
    id: String,
    tx: mpsc::Sender<Report>,
}

impl PlayoutStats {
    pub fn new(id: String, tx: mpsc::Sender<Report>) -> Self {
        Self { id, tx }
    }

    pub(crate) async fn process(&self, stats: PoStats) {
        match stats {
            PoStats::BufferUnderrun => {
                // TODO
            }
        }
    }
}
