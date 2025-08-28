use crate::monitoring::{ObservabilityEvent, PoStats};
use tokio::sync::mpsc;

pub struct PlayoutStats {}

impl PlayoutStats {
    pub fn new(id: String) -> Self {
        Self {}
    }

    pub(crate) async fn process(
        &self,
        stats: PoStats,
        observ_tx: mpsc::Sender<ObservabilityEvent>,
    ) {
        match stats {
            PoStats::BufferUnderrun => {
                // TODO
            }
        }
    }
}
