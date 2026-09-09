use std::sync::{Arc, Mutex};

use anyhow::Result;
use bytes::Bytes;
use flash_cat_common::consts::{RELAY_CHANNEL_CAPACITY, SEND_BUFF_SIZE};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, BufReader, SeekFrom},
    sync::mpsc,
    task::JoinHandle,
};

const READ_BUFFER_SIZE: usize = 1024 * 1024;
const READ_AHEAD_CHUNKS: usize = 2;
const TAG_SIZE: usize = 16;

/// Buffers return to this bounded pool only after the last protobuf/transport
/// Bytes clone releases them. Reuse therefore cannot mutate in-flight ciphertext.
#[derive(Clone, Default)]
pub(crate) struct ChunkPool(Arc<Mutex<Vec<Vec<u8>>>>);

impl ChunkPool {
    fn take(
        &self,
        size: usize,
    ) -> PooledChunk {
        let mut data = self.0.lock().unwrap_or_else(|e| e.into_inner()).pop().unwrap_or_else(|| Vec::with_capacity(size + TAG_SIZE));
        data.reserve((size + TAG_SIZE).saturating_sub(data.len()));
        data.resize(size, 0);
        PooledChunk {
            data,
            pool: self.clone(),
        }
    }
}

pub(crate) struct PooledChunk {
    pub(crate) data: Vec<u8>,
    pool: ChunkPool,
}

impl PooledChunk {
    pub(crate) fn into_bytes(self) -> Bytes {
        Bytes::from_owner(self)
    }
}

impl AsRef<[u8]> for PooledChunk {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl Drop for PooledChunk {
    fn drop(&mut self) {
        let mut buffers = self.pool.0.lock().unwrap_or_else(|e| e.into_inner());
        if buffers.len() < RELAY_CHANNEL_CAPACITY {
            buffers.push(std::mem::take(&mut self.data));
        }
    }
}

/// Owns the read-ahead task so a cancelled send cannot leave a reader running.
pub(crate) struct FileChunks {
    rx: mpsc::Receiver<Result<PooledChunk>>,
    reader: JoinHandle<()>,
}

impl FileChunks {
    pub(crate) fn new(
        path: String,
        position: u64,
        remaining: u64,
        pool: ChunkPool,
    ) -> Self {
        let (tx, rx) = mpsc::channel(READ_AHEAD_CHUNKS);
        let reader = tokio::spawn(async move {
            let result: Result<()> = async {
                let mut file = File::open(path).await?;
                file.seek(SeekFrom::Start(position)).await?;
                // Small files should not allocate a megabyte of read buffering.
                let read_size = remaining.clamp(1, SEND_BUFF_SIZE as u64) as usize;
                let capacity = remaining.clamp(1, READ_BUFFER_SIZE as u64) as usize;
                let mut file = BufReader::with_capacity(capacity, file);
                loop {
                    // Reserve before reading so read-ahead has a strict bound.
                    let permit = match tx.reserve().await {
                        Ok(permit) => permit,
                        Err(_) => return Ok(()),
                    };
                    let mut chunk = pool.take(read_size);
                    let mut len = 0;
                    while len < read_size {
                        let read = file.read(&mut chunk.data[len..]).await?;
                        if read == 0 {
                            break;
                        }
                        len += read;
                    }
                    if len == 0 {
                        return Ok(());
                    }
                    chunk.data.truncate(len);
                    permit.send(Ok(chunk));
                }
            }
            .await;
            if let Err(error) = result {
                let _ = tx.send(Err(error)).await;
            }
        });
        Self {
            rx,
            reader,
        }
    }

    pub(crate) async fn next(&mut self) -> Result<Option<PooledChunk>> {
        match self.rx.recv().await {
            Some(result) => result.map(Some),
            None => {
                // A panicked reader must not look like a successful EOF.
                (&mut self.reader).await?;
                Ok(None)
            }
        }
    }
}

impl Drop for FileChunks {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
