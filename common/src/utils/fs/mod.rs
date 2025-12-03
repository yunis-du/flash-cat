#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    cmp::max,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
#[cfg(feature = "progress")]
use indicatif::{ProgressBar, ProgressStyle};
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
    fn acc(
        &mut self,
        mut pc: Self,
    ) {
        self.total_size += pc.total_size;
        self.num_files += pc.num_files;
        self.num_folders += pc.num_folders;
        self.calc_max_file_name_length(pc.max_file_name_length);
        self.files.append(&mut pc.files);
    }

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
}

/// Collect how many files exist in the paths, how many folders, and the total size.
pub fn collect_files<P: AsRef<Path>>(paths: &[P]) -> FileCollector {
    let mut file_id = 1;
    paths
        .into_iter()
        .map(|path| {
            WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| match entry.metadata() {
                    Ok(metadata) => Some((metadata, entry.path().to_owned(), path.as_ref().to_owned())),
                    Err(_) => None,
                })
                .fold(FileCollector::default(), |mut fc: FileCollector, (metadata, path, root)| {
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let mut root_clone = root.clone();
                    root_clone.pop();
                    let relative_path = path.strip_prefix(root_clone.as_path()).unwrap();
                    if metadata.is_file() {
                        let file_name_legnth = file_name.len();
                        let file_size = metadata.len();
                        let file_info = FileInfo {
                            file_id,
                            name: file_name,
                            access_path: path.to_string_lossy().to_string(),
                            relative_path: relative_path.to_string_lossy().to_string(),
                            #[cfg(unix)]
                            mode: metadata.mode(),
                            size: file_size,
                            empty_dir: false,
                        };
                        file_id += 1;
                        fc.add_file(file_info);
                        fc.calc_max_file_name_length(file_name_legnth);
                        fc.add_total_size(file_size);
                        fc.count_num_files();
                        fc
                    } else if metadata.is_dir() {
                        if let Ok(directory) = path.as_path().read_dir() {
                            if directory.count() == 0 {
                                // empty directory
                                let file_info = FileInfo {
                                    file_id: 0,
                                    name: file_name,
                                    access_path: path.to_string_lossy().to_string(),
                                    relative_path: relative_path.to_string_lossy().to_string(),
                                    #[cfg(unix)]
                                    mode: metadata.mode(),
                                    size: 0,
                                    empty_dir: true,
                                };
                                fc.add_file(file_info);
                            }
                        }
                        fc.count_num_folders();
                        fc
                    } else {
                        fc
                    }
                })
        })
        .fold(FileCollector::default(), |mut fc, cur| {
            fc.acc(cur);
            fc
        })
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

    let file = File::create(&file_name)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Bzip2).unix_permissions(0o755);

    let folder_path = path.to_string_lossy();
    let folder_prefix = if folder_path.ends_with('/') {
        folder_path.to_string()
    } else {
        format!("{}/", folder_path)
    };
    let root_dir = file_name.strip_suffix(".zip").unwrap_or(&file_name);

    #[cfg(feature = "progress")]
    let pb = {
        let pb = ProgressBar::new(0);
        pb.set_style(ProgressStyle::with_template("{spinner:.green} {prefix:.bold.green} {wide_msg}").unwrap());
        pb.set_prefix("compressing...");
        pb
    };

    for entry in WalkDir::new(path).contents_first(true).into_iter().filter_map(|e| e.ok()) {
        if shutdown.is_terminated() {
            break;
        }

        let entry_path = entry.path();
        let relative_path = entry_path.to_string_lossy().replace(&folder_prefix, "");
        let path_in_zip = format!("{}/{}", root_dir, relative_path);

        if entry_path.is_dir() {
            // Check for empty directory
            if fs::read_dir(entry_path).map(|mut d| d.next().is_none()).unwrap_or(false) {
                let dir_path = format!("{}/", path_in_zip);
                zip.add_directory(&dir_path, options)?;
            }
        } else if entry_path.is_file() {
            #[cfg(feature = "progress")]
            {
                pb.set_message(path_in_zip.clone());
                pb.inc(1);
            }

            zip.start_file(&path_in_zip, options)?;
            let mut file = File::open(entry_path)?;
            io::copy(&mut file, &mut zip)?;
        }
    }

    #[cfg(feature = "progress")]
    pb.finish_and_clear();

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

/// Check the file for missing (all-zero) chunks and calculate the percentage of saved data.
///
/// # Arguments
/// * `path` - The path to the file to be checked.
/// * `chunk_size` - The size of each chunk (in bytes) to check for missing data.
///
/// # Returns
/// * `Ok((saved_chunks, missing_chunks, percent))`
///     - `saved_chunks`: Number of non-empty (saved) chunks found in the file.
///     - `missing_chunks`: Number of empty (all-zero) chunks found at the end of the file.
///     - `percent`: Percentage of the file that is non-empty (saved), rounded to two decimal places.
///
/// # Errors
/// Returns an error if the file cannot be opened or read.
pub fn missing_chunks(
    path: impl AsRef<Path>,
    chunk_size: usize,
) -> Result<(usize, usize, f64)> {
    let f = File::open(path)?;
    let fsize = f.metadata()?.len();

    let empty_buffer = vec![0u8; chunk_size];
    let mut saved_chunks = 0;
    let mut missing_chunks = 0;

    let mut reader = io::BufReader::new(f);
    let mut buffer = vec![0u8; chunk_size];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        if buffer[..bytes_read] == empty_buffer[..bytes_read] {
            // empty chunk
            missing_chunks += 1;
        } else {
            saved_chunks += 1;
            if missing_chunks > 0 {
                // add missing chunks to saved chunks
                saved_chunks += missing_chunks;
                missing_chunks = 0;
            }
        }
    }

    let percent = ((saved_chunks * chunk_size) as f64 / fsize as f64 * 100.0 * 100.0).round() / 100.0;

    Ok((saved_chunks, missing_chunks, percent))
}
