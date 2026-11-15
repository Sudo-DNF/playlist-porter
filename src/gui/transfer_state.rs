//! UI state for the YouTube transfer flow.

#[derive(Debug, Clone, Default)]
pub struct TransferView {
    pub running: bool,
}

impl TransferView {
    pub fn is_running(&self) -> bool {
        self.running
    }
}
