use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use tokio::sync::mpsc;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Intermediate UI updates must never apply backpressure to file data. Callers
/// send final positions and lifecycle events reliably through the normal channel.
#[derive(Default)]
pub(crate) struct ProgressThrottle {
    last_sent: Option<Instant>,
}

impl ProgressThrottle {
    pub(crate) fn report<T>(
        &mut self,
        tx: &mpsc::Sender<T>,
        message: T,
    ) -> Result<()> {
        if tx.is_closed() {
            bail!("progress consumer closed");
        }
        let now = Instant::now();
        if self.last_sent.is_some_and(|last| now.duration_since(last) < PROGRESS_INTERVAL) {
            return Ok(());
        }
        match tx.try_send(message) {
            Ok(()) => self.last_sent = Some(now),
            Err(mpsc::error::TrySendError::Full(_)) => (),
            Err(mpsc::error::TrySendError::Closed(_)) => bail!("progress consumer closed"),
        }
        Ok(())
    }
}
