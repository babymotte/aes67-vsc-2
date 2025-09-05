use crate::monitoring::{Report, TxStats};
use tokio::sync::mpsc;

pub struct SenderStats {
    id: String,
    tx: mpsc::Sender<Report>,
}

impl SenderStats {
    pub fn new(id: String, tx: mpsc::Sender<Report>) -> Self {
        Self { id, tx }
    }

    pub(crate) async fn process(&self, stats: TxStats) {
        match stats {
            TxStats::BufferUnderrun => {
                // TODO
            }
        }
    }
}
