use std::{env, fs, io::Cursor, path::PathBuf};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use tar::Archive;

use crate::built_info;

const LATEST_VERSION_URL: &str = "https://download.yunisdu.com/flash-cat/latest";
const DOWNLOAD_BASE_URL: &str = "https://download.yunisdu.com/flash-cat";

/// Detect the current OS
fn detect_os() -> Result<&'static str> {
    match env::consts::OS {
        "linux" => Ok("linux"),
        "macos" => Ok("macos"),
        "windows" => Ok("windows"),
        os => bail!("Unsupported OS: {}", os),
    }
}

/// Detect the current architecture
fn detect_arch() -> Result<&'static str> {
    match env::consts::ARCH {
        "x86_64" => Ok("x86_64"),
        "aarch64" => Ok("aarch64"),
        "x86" => Ok("i686"),
        arch => bail!("Unsupported architecture: {}", arch),
    }
}

/// Get the path of the current executable
fn get_current_exe() -> Result<PathBuf> {
    env::current_exe().context("Failed to get current executable path")
}

/// Fetch the latest version from the server
async fn fetch_latest_version(client: &reqwest::Client) -> Result<String> {
    let version = client
        .get(LATEST_VERSION_URL)
        .send()
        .await
        .context("Failed to fetch latest version")?
        .text()
        .await
        .context("Failed to read version response")?
        .trim()
        .to_string();
    Ok(version)
}

/// Download the binary archive
async fn download_archive(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<u8>> {
    println!("Downloading from: {}", url);
    let response = client.get(url).send().await.context("Failed to download archive")?;

    if !response.status().is_success() {
        bail!("Download failed with status: {}", response.status());
    }

    let bytes = response.bytes().await.context("Failed to read archive bytes")?;
    Ok(bytes.to_vec())
}

/// Install the new binary to the target path
fn install_binary(
    temp_path: &PathBuf,
    target_path: &PathBuf,
) -> Result<()> {
    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(temp_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(temp_path, perms)?;
    }

    // Rename old binary to .old (backup)
    let backup_path = target_path.with_extension("old");
    if target_path.exists() {
        let _ = fs::remove_file(&backup_path); // Remove old backup if exists
        fs::rename(target_path, &backup_path).context("Failed to backup current binary")?;
    }

    // Move new binary to target path
    fs::rename(temp_path, target_path).context("Failed to install new binary")?;

    // Remove backup
    let _ = fs::remove_file(&backup_path);

    Ok(())
}

/// Extract the binary from tar.gz archive (Linux/macOS)
fn extract_from_tar_gz(
    archive_data: &[u8],
    target_path: &PathBuf,
) -> Result<()> {
    let decoder = GzDecoder::new(Cursor::new(archive_data));
    let mut archive = Archive::new(decoder);

    for entry in archive.entries().context("Failed to read archive entries")? {
        let mut entry = entry.context("Failed to read archive entry")?;
        let path = entry.path().context("Failed to get entry path")?;

        // Look for the flash-cat binary
        if path.file_name().map(|n| n == "flash-cat" || n == "flash-cat.exe").unwrap_or(false) {
            // Create a temporary file for extraction
            let temp_path = target_path.with_extension("new");
            entry.unpack(&temp_path).context("Failed to extract binary")?;

            return install_binary(&temp_path, target_path);
        }
    }

    bail!("Binary not found in archive")
}

/// Extract the binary from zip archive (Windows)
fn extract_from_zip(
    archive_data: &[u8],
    target_path: &PathBuf,
) -> Result<()> {
    use std::io::Read;

    let cursor = Cursor::new(archive_data);
    let mut archive = zip::ZipArchive::new(cursor).context("Failed to open zip archive")?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).context("Failed to read zip entry")?;
        let name = file.name().to_string();

        // Look for the flash-cat binary
        if name == "flash-cat.exe" || name == "flash-cat" {
            let temp_path = target_path.with_extension("new");

            let mut content = Vec::new();
            file.read_to_end(&mut content).context("Failed to read binary from zip")?;
            fs::write(&temp_path, content).context("Failed to write binary")?;

            return install_binary(&temp_path, target_path);
        }
    }

    bail!("Binary not found in archive")
}

/// Extract and install the binary based on archive type
fn extract_binary(
    archive_data: &[u8],
    target_path: &PathBuf,
    is_zip: bool,
) -> Result<()> {
    if is_zip {
        extract_from_zip(archive_data, target_path)
    } else {
        extract_from_tar_gz(archive_data, target_path)
    }
}

/// Perform the update
pub async fn update() -> Result<()> {
    let current_version = built_info::PKG_VERSION;
    println!("Current version: {}", current_version);

    let client = reqwest::Client::new();

    // 1. Fetch latest version
    println!("Checking for updates...");
    let latest_version = fetch_latest_version(&client).await?;
    println!("Latest version: {}", latest_version);

    if current_version == latest_version {
        println!("You are already using the latest version!");
        return Ok(());
    }

    // 2. Detect platform
    let os = detect_os()?;
    let arch = detect_arch()?;
    println!("Platform: {}-{}", os, arch);

    // 3. Construct download URL
    let is_windows = os == "windows";
    let ext = if is_windows {
        "zip"
    } else {
        "tar.gz"
    };
    let binary_name = format!("flash-cat-cli-{}-{}-{}.{}", os, latest_version, arch, ext);
    let download_url = format!("{}/{}/{}", DOWNLOAD_BASE_URL, latest_version, binary_name);

    // 4. Download archive
    let archive_data = download_archive(&client, &download_url).await?;
    println!("Download complete ({} bytes)", archive_data.len());

    // 5. Extract and replace binary
    let current_exe = get_current_exe()?;
    println!("Installing to: {}", current_exe.display());

    extract_binary(&archive_data, &current_exe, is_windows)?;

    println!("Update to {} completed successfully!", latest_version);
    Ok(())
}
