use std::{fs, path::PathBuf};

use anyhow::{Result, anyhow, bail};
use directories::{ProjectDirs, UserDirs};
use rfd::AsyncFileDialog;

use crate::PKG_NAME;

pub async fn pick_folder() -> Result<Option<PathBuf>> {
    let pick_path = AsyncFileDialog::new().set_directory(get_home_directory()?).pick_folder().await.map(|file_handle| file_handle.path().to_owned());
    Ok(pick_path)
}

pub async fn pick_folders() -> Result<Option<Vec<PathBuf>>> {
    let pick_path = AsyncFileDialog::new()
        .set_directory(get_home_directory()?)
        .pick_folders()
        .await
        .map(|file_handles| file_handles.iter().map(|file_handle| file_handle.path().to_owned()).collect());
    Ok(pick_path)
}

pub async fn pick_files() -> Result<Option<Vec<PathBuf>>> {
    let pick_path = AsyncFileDialog::new()
        .set_directory(get_home_directory()?)
        .pick_files()
        .await
        .map(|file_handles| file_handles.iter().map(|file_handle| file_handle.path().to_owned()).collect());
    Ok(pick_path)
}

pub fn get_or_create_config_path() -> Result<PathBuf> {
    let Some(project_dirs) = ProjectDirs::from("com", "yunisdu", PKG_NAME) else {
        bail!("project directories not found");
    };

    let config_dir = project_dirs.config_dir();

    if !config_dir.exists() {
        fs::create_dir_all(config_dir)?;
    }

    let config_path = config_dir.join("flashcat.toml");
    if config_path.exists() {
        return Ok(config_path);
    }
    std::fs::write(&config_path, "")?;

    Ok(config_path)
}

pub fn get_user_download_dir() -> String {
    UserDirs::new().unwrap().download_dir().unwrap().to_string_lossy().to_string()
}

pub fn get_home_directory() -> Result<PathBuf> {
    let user_dirs = UserDirs::new().ok_or(anyhow!("could not get user directory"))?;
    Ok(user_dirs.home_dir().to_path_buf())
}
