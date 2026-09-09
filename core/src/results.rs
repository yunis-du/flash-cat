use crate::{FileResult, FileStatus};
use std::collections::HashMap;

/// Idempotent result accounting shared by CLI and desktop, including reconnect replay.
#[derive(Default, Clone)]
pub struct TransferResults {
    results: HashMap<u64, FileResult>,
    pub succeeded: u64,
    pub skipped: u64,
    pub failed: u64,
}
impl TransferResults {
    pub fn record(
        &mut self,
        result: FileResult,
    ) -> bool {
        if self.results.contains_key(&result.file_id) {
            return false;
        }
        match result.status() {
            FileStatus::Success => self.succeeded += 1,
            FileStatus::Skipped => self.skipped += 1,
            FileStatus::Failed => self.failed += 1,
        }
        self.results.insert(result.file_id, result);
        true
    }
    pub fn remaining(
        &self,
        total: u64,
    ) -> u64 {
        total.saturating_sub(self.succeeded + self.skipped + self.failed)
    }
    pub fn summary(
        &self,
        total: u64,
    ) -> String {
        format!(
            "{} succeeded, {} skipped, {} failed, {} unfinished",
            self.succeeded,
            self.skipped,
            self.failed,
            self.remaining(total)
        )
    }
}
