use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
};

use crate::Shutdown;
use anyhow::{Context, Result, bail};

/// Receives directly into the selected destination. Existing files are opened
/// only after conflict confirmation, and locked before any truncation.
pub struct ReceiveFile {
    pub target: PathBuf,
    guard: Option<std::fs::File>,
    pub expected_size: u64,
    mode: u32,
    shutdown: Shutdown,
}

impl ReceiveFile {
    pub fn new(
        target: PathBuf,
        expected_size: u64,
        mode: u32,
        shutdown: Shutdown,
    ) -> Self {
        Self {
            target,
            guard: None,
            expected_size,
            mode,
            shutdown,
        }
    }

    pub fn open(
        &mut self,
        overwrite: bool,
    ) -> Result<()> {
        if self.guard.is_some() {
            bail!("Receive file is already open");
        }
        if self.shutdown.is_terminated() {
            bail!("Transfer cancelled before opening file");
        }
        let parent = parent_directory(&self.target);
        std::fs::create_dir_all(parent)?;
        if overwrite {
            if let Ok(info) = std::fs::symlink_metadata(&self.target) {
                Self::check_regular(&info)?;
            }
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        if overwrite {
            options.create(true).truncate(false);
        } else {
            options.create_new(true);
        }
        let guard = options.open(&self.target).with_context(|| format!("Cannot open {} for receiving", self.target.display()))?;
        Self::check_regular(&guard.metadata()?)?;
        guard.try_lock().with_context(|| format!("Another transfer is already receiving {}", self.target.display()))?;
        // Never truncate before acquiring the lock.
        if overwrite {
            guard.set_len(0)?;
        }
        self.guard = Some(guard);
        Ok(())
    }

    pub fn open_resume(
        &mut self,
        position: u64,
    ) -> Result<()> {
        if self.guard.is_some() || self.shutdown.is_terminated() {
            bail!("Cannot open resume destination");
        }
        Self::check_regular(&std::fs::symlink_metadata(&self.target)?)?;
        let guard = OpenOptions::new().read(true).write(true).open(&self.target)?;
        Self::check_regular(&guard.metadata()?)?;
        guard.try_lock().context("Another transfer is already receiving this file")?;
        if position == 0 || position >= self.expected_size || guard.metadata()?.len() != position {
            bail!("Resume destination size changed; retry receiving");
        }
        self.guard = Some(guard);
        Ok(())
    }

    fn check_regular(info: &std::fs::Metadata) -> Result<()> {
        if !info.is_file() {
            bail!("Receive destination is not a regular file");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if info.nlink() != 1 {
                bail!("Receive destination has multiple links");
            }
        }
        Ok(())
    }

    pub fn file(&self) -> Result<tokio::fs::File> {
        Ok(tokio::fs::File::from_std(
            self.guard.as_ref().context("Receive file is not open")?.try_clone()?,
        ))
    }

    pub fn keep_both(&mut self) -> Result<()> {
        self.target = unused_name(&self.target)?;
        self.open(false)
    }

    pub fn finish(self) -> Result<PathBuf> {
        let guard = self.guard.as_ref().context("Receive file is not open")?;
        if guard.metadata()?.len() != self.expected_size {
            bail!("File size mismatch for {}", self.target.display());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            guard.set_permissions(std::fs::Permissions::from_mode(if self.mode == 0 {
                0o644
            } else {
                self.mode & 0o777
            }))?;
        }
        guard.sync_all().with_context(|| format!("Cannot sync received file {}", self.target.display()))?;
        #[cfg(unix)]
        {
            let parent = parent_directory(&self.target);
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .with_context(|| format!("Cannot sync receive directory {}", parent.display()))?;
        }
        if self.shutdown.is_terminated() {
            bail!("Transfer cancelled before completion");
        }
        Ok(self.target)
    }
}

fn parent_directory(path: &Path) -> &Path {
    path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."))
}

fn unused_name(path: &Path) -> Result<PathBuf> {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    for number in 1..10000 {
        let candidate = path.with_file_name(format!("{stem}({number}){extension}"));
        if !candidate.try_exists()? {
            return Ok(candidate);
        }
    }
    bail!("Cannot find an unused name for {}", path.display());
}
