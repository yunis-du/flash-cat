use std::{fs::Metadata, time::UNIX_EPOCH};

use anyhow::{Context, Result};

/// Resume metadata, obtained with stat only. No file contents are read.
pub fn file_identity(metadata: &Metadata) -> Result<String> {
    let modified = metadata.modified().context("Cannot read file modification time")?;
    let nanos = match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos() as i128,
        Err(error) => -(error.duration().as_nanos() as i128),
    };
    Ok(format!("{}:{nanos}", metadata.len()))
}

/// Hash precisely the saved prefix with bounded memory. The caller must keep the
/// destination locked and seek its writer afterwards if using a cloned handle.
pub async fn prefix_digest(
    mut file: tokio::fs::File,
    length: u64,
) -> Result<String> {
    use sha2::{Digest, Sha256};
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    file.seek(std::io::SeekFrom::Start(0)).await?;
    let mut remaining = length;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 256 * 1024];
    while remaining > 0 {
        let wanted = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..wanted]).await?;
        hash.update(&buffer[..wanted]);
        remaining -= wanted as u64;
    }
    Ok(hex::encode(hash.finalize()))
}
