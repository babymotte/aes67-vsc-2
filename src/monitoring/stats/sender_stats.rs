use crate::monitoring::{ObservabilityEvent, TxStats};
use tokio::sync::mpsc;

pub struct SenderStats {}

impl SenderStats {
    pub fn new(id: String) -> Self {
        Self {}
    }

    pub(crate) async fn process(
        &self,
        stats: TxStats,
        observ_tx: mpsc::Sender<ObservabilityEvent>,
    ) {
        match stats {
            TxStats::BufferUnderrun => {
                // TODO
            }
        }
    }
}
