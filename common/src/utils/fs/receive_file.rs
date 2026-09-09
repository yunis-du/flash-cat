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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("flash-cat-receive-test-{}", rand::random::<u64>()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn destination(&self) -> ReceiveFile {
            ReceiveFile::new(self.0.join("file"), 4, 0, Shutdown::new())
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn bare_filename_uses_current_directory() {
        assert_eq!(parent_directory(Path::new("file")), Path::new("."));
        assert_eq!(parent_directory(Path::new("folder/file")), Path::new("folder"));
        let path = PathBuf::from(format!("flash-cat-relative-test-{}", rand::random::<u64>()));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let mut destination = ReceiveFile::new(path.clone(), 4, 0, Shutdown::new());
        destination.open(false).unwrap();
        destination.guard.as_mut().unwrap().write_all(b"data").unwrap();
        destination.finish().unwrap();
        std::fs::write(&path, b"da").unwrap();
        let mut destination = ReceiveFile::new(path.clone(), 4, 0, Shutdown::new());
        destination.open_resume(2).unwrap();
        use std::io::{Seek, SeekFrom};
        let writer = destination.guard.as_mut().unwrap();
        writer.seek(SeekFrom::Start(2)).unwrap();
        writer.write_all(b"ta").unwrap();
        destination.finish().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"data");
    }

    #[tokio::test]
    async fn resume_preserves_prefix_and_appends_without_sidecars() {
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};
        let dir = TestDir::new();
        std::fs::write(dir.0.join("file"), b"ab").unwrap();
        let mut destination = dir.destination();
        assert!(destination.open_resume(1).is_err());
        destination.open_resume(2).unwrap();
        assert!(dir.destination().open_resume(2).is_err());
        assert_eq!(std::fs::read(dir.0.join("file")).unwrap(), b"ab");
        let mut writer = destination.file().unwrap();
        writer.seek(std::io::SeekFrom::Start(2)).await.unwrap();
        writer.write_all(b"cd").await.unwrap();
        writer.flush().await.unwrap();
        drop(writer);
        destination.finish().unwrap();
        assert_eq!(std::fs::read(dir.0.join("file")).unwrap(), b"abcd");
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn skipped_conflict_does_not_touch_original_or_create_files() {
        let dir = TestDir::new();
        std::fs::write(dir.0.join("file"), b"original").unwrap();
        drop(dir.destination());
        assert_eq!(std::fs::read(dir.0.join("file")).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn data_is_written_directly_and_completion_creates_no_sidecars() {
        let dir = TestDir::new();
        let mut destination = dir.destination();
        destination.open(false).unwrap();
        destination.guard.as_mut().unwrap().write_all(b"data").unwrap();
        assert_eq!(std::fs::read(dir.0.join("file")).unwrap(), b"data");
        destination.finish().unwrap();
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn interrupted_file_is_preserved_and_new_run_requires_conflict_resolution() {
        let dir = TestDir::new();
        let mut destination = dir.destination();
        destination.open(false).unwrap();
        destination.guard.as_mut().unwrap().write_all(b"ab").unwrap();
        assert!(destination.finish().is_err());
        assert_eq!(std::fs::read(dir.0.join("file")).unwrap(), b"ab");
        assert!(dir.destination().open(false).is_err());
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn concurrent_receiver_cannot_truncate_locked_destination() {
        let dir = TestDir::new();
        let mut destination = dir.destination();
        destination.open(false).unwrap();
        destination.guard.as_mut().unwrap().write_all(b"data").unwrap();
        assert!(dir.destination().open(true).is_err());
        assert_eq!(std::fs::read(dir.0.join("file")).unwrap(), b"data");
        destination.finish().unwrap();
    }

    #[test]
    fn keep_both_preserves_original_and_overwrite_truncates_only_on_open() {
        let dir = TestDir::new();
        std::fs::write(dir.0.join("file"), b"original").unwrap();
        let mut destination = dir.destination();
        destination.keep_both().unwrap();
        destination.guard.as_mut().unwrap().write_all(b"data").unwrap();
        assert_eq!(std::fs::read(destination.finish().unwrap()).unwrap(), b"data");
        assert_eq!(std::fs::read(dir.0.join("file")).unwrap(), b"original");
        let mut destination = dir.destination();
        destination.open(true).unwrap();
        assert_eq!(std::fs::metadata(dir.0.join("file")).unwrap().len(), 0);
        destination.guard.as_mut().unwrap().write_all(b"next").unwrap();
        destination.finish().unwrap();
        assert_eq!(std::fs::read(dir.0.join("file")).unwrap(), b"next");
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 2);
    }

    #[test]
    fn empty_file_succeeds_but_cancelled_file_does_not() {
        let dir = TestDir::new();
        let mut destination = dir.destination();
        destination.expected_size = 0;
        destination.open(false).unwrap();
        destination.finish().unwrap();
        let mut destination = dir.destination();
        destination.open(true).unwrap();
        destination.guard.as_mut().unwrap().write_all(b"data").unwrap();
        destination.shutdown.shutdown();
        assert!(destination.finish().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn overwrite_rejects_symlinks_and_hardlinks() {
        let dir = TestDir::new();
        let original = dir.0.join("original");
        let target = dir.0.join("file");
        std::fs::write(&original, b"original").unwrap();
        std::os::unix::fs::symlink(&original, &target).unwrap();
        assert!(dir.destination().open(true).is_err());
        std::fs::remove_file(&target).unwrap();
        std::fs::hard_link(&original, &target).unwrap();
        assert!(dir.destination().open(true).is_err());
        assert_eq!(std::fs::read(original).unwrap(), b"original");
    }
}
