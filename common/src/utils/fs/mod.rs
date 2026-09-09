pub mod identity;
pub mod receive_file;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    cmp::max,
    fs::{self, File},
    io,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
#[cfg(feature = "progress")]
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::Shutdown;

use super::human_bytes;

#[derive(Debug, Default, Clone)]
pub struct FileInfo {
    pub file_id: u64,
    pub name: String,
    pub access_path: String,
    pub relative_path: String,
    #[cfg(unix)]
    pub mode: u32,
    pub size: u64,
    pub empty_dir: bool,
    pub source_identity: String,
}

#[derive(Debug, Default, Clone)]
pub struct FileCollector {
    pub files: Vec<FileInfo>,
    pub total_size: u64,
    pub num_files: u64,
    pub num_folders: u64,
    pub max_file_name_length: usize,
}

impl FileCollector {
    fn add_total_size(
        &mut self,
        total_size: u64,
    ) {
        self.total_size += total_size;
    }

    fn count_num_files(&mut self) {
        self.num_files += 1;
    }

    fn count_num_folders(&mut self) {
        self.num_folders += 1;
    }

    fn add_file(
        &mut self,
        file: FileInfo,
    ) {
        self.files.push(file);
    }

    fn calc_max_file_name_length(
        &mut self,
        curr_file_name_legnth: usize,
    ) {
        self.max_file_name_length = max(self.max_file_name_length, curr_file_name_legnth);
    }

    pub fn total_size_to_human_readable(&self) -> String {
        human_bytes(self.total_size)
    }

    pub fn file_count(&self) -> u64 {
        self.num_files
    }

    pub fn folder_count(&self) -> u64 {
        self.num_folders
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScanProgress {
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
}

/// Scan fails explicitly on inaccessible entries; a partial selection must never
/// silently become a successful transfer. Callers may cancel between entries.
pub fn collect_files<P: AsRef<Path>>(paths: &[P]) -> Result<FileCollector> {
    collect_files_with_progress(paths, &Shutdown::new(), |_| {})
}

pub fn collect_files_with_progress<P: AsRef<Path>>(
    paths: &[P],
    shutdown: &Shutdown,
    mut report: impl FnMut(ScanProgress),
) -> Result<FileCollector> {
    let mut collector = FileCollector::default();
    let mut seen = std::collections::HashSet::new();
    let mut last_report = std::time::Instant::now();
    report(ScanProgress::default());
    let mut roots = paths
        .iter()
        .map(|selected| std::path::absolute(selected.as_ref()).with_context(|| format!("Cannot scan {}", selected.as_ref().display())))
        .collect::<Result<Vec<_>>>()?;
    // Prefer the parent selection so a separately selected child keeps its
    // directory layout and is not accidentally sent under a different root.
    roots.sort_by_key(|root| root.components().count());
    let mut scanned_roots = std::collections::HashSet::new();
    for root in roots {
        if shutdown.is_terminated() {
            bail!("File scan cancelled");
        }
        if root.ancestors().any(|ancestor| scanned_roots.contains(ancestor)) {
            continue;
        }
        scanned_roots.insert(root.clone());
        let parent = root.parent().unwrap_or(Path::new("/"));
        for entry in WalkDir::new(&root).follow_links(true) {
            if shutdown.is_terminated() {
                bail!("File scan cancelled");
            }
            let entry = entry.with_context(|| format!("Cannot scan {}", root.display()))?;
            let path = entry.path();
            // Selecting a folder and a file inside it should only enqueue that path once.
            if !seen.insert(path.to_path_buf()) {
                continue;
            }
            let metadata = entry.metadata().with_context(|| format!("Cannot inspect {}", path.display()))?;
            let empty_dir = if metadata.is_dir() {
                collector.count_num_folders();
                fs::read_dir(path)
                    .with_context(|| format!("Cannot read folder {}", path.display()))?
                    .next()
                    .transpose()
                    .with_context(|| format!("Cannot read folder {}", path.display()))?
                    .is_none()
            } else {
                false
            };
            if metadata.is_file() || empty_dir {
                if metadata.is_file() {
                    File::open(path).with_context(|| format!("Cannot read file {}", path.display()))?;
                    collector.count_num_files();
                    collector.add_total_size(metadata.len());
                }
                let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                collector.calc_max_file_name_length(name.chars().count());
                collector.add_file(FileInfo {
                    file_id: collector.files.len() as u64 + 1,
                    name,
                    access_path: path.to_string_lossy().into_owned(),
                    relative_path: path.strip_prefix(parent)?.to_string_lossy().into_owned(),
                    #[cfg(unix)]
                    mode: metadata.mode(),
                    size: if empty_dir {
                        0
                    } else {
                        metadata.len()
                    },
                    empty_dir,
                    source_identity: if empty_dir {
                        String::new()
                    } else {
                        identity::file_identity(&metadata)?
                    },
                });
            } else if !metadata.is_dir() {
                bail!("Unsupported file type: {}", path.display());
            }
            if last_report.elapsed() >= std::time::Duration::from_millis(100) {
                report(ScanProgress {
                    files: collector.num_files,
                    folders: collector.num_folders,
                    bytes: collector.total_size,
                });
                last_report = std::time::Instant::now();
            }
        }
    }
    if shutdown.is_terminated() {
        bail!("File scan cancelled");
    }
    if collector.files.is_empty() {
        bail!("No readable files or empty folders were selected");
    }
    report(ScanProgress {
        files: collector.num_files,
        folders: collector.num_folders,
        bytes: collector.total_size,
    });
    Ok(collector)
}

/// Check whether the paths exists.
pub fn paths_exist<P: AsRef<Path>>(paths: &[P]) -> Result<()> {
    for path in paths {
        let path = path.as_ref();
        if !path.exists() {
            bail!(format!("{}: no such file or directory", path.to_string_lossy()));
        }
    }
    Ok(())
}

/// Pick up folder path from given paths
pub fn pick_up_folder<P: AsRef<Path>>(paths: &[P]) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter_map(|p| Some(p.as_ref()))
        .filter_map(|p| {
            if let Ok(m) = p.metadata() {
                if m.is_dir() {
                    Some(p.to_owned())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
}

/// Zip folder, print compress progress if show_progress is true.
pub fn zip_folder<P: AsRef<Path>>(
    file_name: String,
    path: P,
    shutdown: Shutdown,
) -> Result<()> {
    let path = path.as_ref();
    if path.is_file() {
        bail!("{:?} is file.", path.as_os_str());
    }

    if file_name.is_empty() {
        bail!("file name is empty.");
    }

    let file_name = if file_name.ends_with(".zip") {
        file_name
    } else {
        format!("{}.zip", file_name)
    };

    if Path::new(&file_name).exists() {
        bail!("{} already exist.", file_name);
    }

    let path = fs::canonicalize(path)?;
    let file = File::create(&file_name)?;
    // The output archive can be located inside the source directory, most
    // notably for `flash-cat send --zip .`. Exclude it from both walks so the
    // archive never tries to compress its own continuously growing contents.
    let output_path = fs::canonicalize(&file_name)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).unix_permissions(0o755);

    let archive_name = Path::new(&file_name).file_name().unwrap_or_default().to_string_lossy();
    let root_dir = archive_name.strip_suffix(".zip").unwrap_or(&archive_name);

    #[cfg(feature = "progress")]
    let (total_size, total_files) = {
        let mut total_size = 0u64;
        let mut total_files = 0u64;
        for entry in WalkDir::new(&path) {
            if shutdown.is_terminated() {
                bail!("Folder compression cancelled");
            }
            let entry = entry.with_context(|| format!("Cannot scan {}", path.display()))?;
            if entry.path() == output_path {
                continue;
            }
            if entry.path().is_file() {
                total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
                total_files += 1;
            }
        }
        (total_size, total_files)
    };

    #[cfg(feature = "progress")]
    let (multi, spinner, progress_bar, counter_width) = {
        use std::fmt::Write;

        let multi = MultiProgress::new();

        // Calculate counter width for alignment (e.g., "(999/999)" = 9 chars)
        let counter_width = format!("({}/{})", total_files, total_files).len();

        // Line 1: spinner with file info, counter fixed at right
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(ProgressStyle::with_template("{spinner:.green} {prefix:.bold.green} {msg}").unwrap());
        let spinner = multi.add(spinner);

        // Line 2: progress bar
        let progress_bar = ProgressBar::new(total_size);
        progress_bar.set_style(
            ProgressStyle::with_template("[{bar:50.cyan/blue}] {bytes}/{total_bytes} ({percent}%)")
                .unwrap()
                .with_key("percent", |state: &ProgressState, w: &mut dyn Write| {
                    write!(w, "{:.1}", state.fraction() * 100.0).unwrap()
                })
                .progress_chars("#>-"),
        );
        let progress_bar = multi.add(progress_bar);

        (multi, spinner, progress_bar, counter_width)
    };

    #[cfg(feature = "progress")]
    let mut processed_files = 0u64;

    for entry in WalkDir::new(&path).contents_first(true) {
        let entry = entry.with_context(|| format!("Cannot scan {}", path.display()))?;
        if shutdown.is_terminated() {
            bail!("Folder compression cancelled");
        }

        let entry_path = entry.path();
        if entry_path == output_path {
            continue;
        }
        let relative_path = entry_path.strip_prefix(&path).unwrap_or(entry_path).to_string_lossy();
        // The ZIP specification requires the use of a forward slash '/' as the path separator.
        // On Windows, a backslash needs to be replaced.
        let path_in_zip = format!("{}/{}", root_dir, relative_path.replace('\\', "/"));

        if entry_path.is_dir() {
            // Check for empty directory
            if fs::read_dir(entry_path).map(|mut d| d.next().is_none()).unwrap_or(false) {
                let dir_path = format!("{}/", path_in_zip);
                zip.add_directory(&dir_path, options)?;
            }
        } else if entry_path.is_file() {
            #[cfg(feature = "progress")]
            {
                processed_files += 1;

                let display_path = entry_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

                let counter = format!("({}/{})", processed_files, total_files);
                spinner.set_message(format!("{:>width$}", counter, width = counter_width));
                spinner.set_prefix(format!("Compressing {:<width$}", display_path, width = 50));
            }

            zip.start_file(&path_in_zip, options)?;
            let file = File::open(entry_path)?;
            #[cfg(feature = "progress")]
            let mut file = progress_bar.wrap_read(file);
            #[cfg(not(feature = "progress"))]
            let mut file = file;
            let mut buffer = vec![0; 1024 * 1024];
            loop {
                if shutdown.is_terminated() {
                    bail!("Folder compression cancelled");
                }
                let read = io::Read::read(&mut file, &mut buffer)?;
                if read == 0 {
                    break;
                }
                io::Write::write_all(&mut zip, &buffer[..read])?;
            }

            #[cfg(feature = "progress")]
            {
                spinner.inc(1);
            }
        }
    }

    #[cfg(feature = "progress")]
    {
        spinner.finish_and_clear();
        progress_bar.finish_and_clear();
        let _ = multi.clear();
    }

    zip.finish()?;
    Ok(())
}

/// Unzip given zip file.
pub fn unzip<P: AsRef<Path>>(zip_files: &[P]) -> Result<()> {
    for zip_file in zip_files {
        let zip_file_path = zip_file.as_ref();
        let root_dir = match zip_file_path.file_name() {
            Some(file_name) => {
                let mut root_dir = "";
                let file_name = file_name.to_str().unwrap_or_default().split(".").collect::<Vec<_>>();
                if file_name.len() > 0 {
                    root_dir = file_name.get(0).unwrap();
                }
                root_dir
            }
            None => "",
        };
        // create root directory
        fs::create_dir(root_dir)?;
        let root_path = Path::new(root_dir);

        let file = File::open(zip_file_path)?;
        let mut archive = ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;

            // create the decompressed file path
            let outpath = root_path.join(file.mangled_name());
            println!("outpath: {outpath:?}");

            // if the extracted file is a directory, create the corresponding directory
            if (&*file.name()).ends_with('/') {
                std::fs::create_dir_all(&outpath)?;
            } else {
                // create the decompressed file
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        std::fs::create_dir_all(&p)?;
                    }
                }
                let mut outfile = File::create(&outpath)?;
                io::copy(&mut file, &mut outfile)?;
            }
        }
    }
    Ok(())
}

pub fn remove_files<P: AsRef<Path>>(files: &[P]) -> Result<()> {
    for path in files {
        let path = path.as_ref();
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn is_idr<P: AsRef<Path>>(path: P) -> bool {
    if let Ok(m) = path.as_ref().metadata() {
        m.is_dir()
    } else {
        false
    }
}

pub fn is_file<P: AsRef<Path>>(path: P) -> bool {
    if let Ok(m) = path.as_ref().metadata() {
        m.is_file()
    } else {
        false
    }
}

pub fn reset_path(path: &str) -> String {
    #[cfg(unix)]
    {
        path.replace("\\", "/")
    }
    #[cfg(windows)]
    {
        path.replace("/", "\\")
    }
}

pub fn safe_join_relative_path(
    base: impl AsRef<Path>,
    relative_path: &str,
) -> Result<PathBuf> {
    let normalized = reset_path(relative_path);
    let mut safe_relative = PathBuf::new();

    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => safe_relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("unsafe relative path: {relative_path}");
            }
        }
    }

    if safe_relative.as_os_str().is_empty() {
        bail!("empty relative path");
    }

    Ok(base.as_ref().join(safe_relative))
}
