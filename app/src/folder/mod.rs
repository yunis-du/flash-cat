use directories::{ProjectDirs, UserDirs};
use std::path::PathBuf;

use rfd::AsyncFileDialog;

pub async fn chosen_path() -> anyhow::Result<Option<PathBuf>> {
    let chosen_path = AsyncFileDialog::new()
        .set_directory(get_home_directory()?)
        .pick_folder()
        .await
        .map(|file_handle| file_handle.path().to_owned());
    Ok(chosen_path)
}

pub fn get_home_directory() -> anyhow::Result<PathBuf> {
    let user_dirs = UserDirs::new().ok_or(anyhow::anyhow!("could not get user directory"))?;
    Ok(user_dirs.home_dir().to_path_buf())
}

fn project_dir() -> ProjectDirs {
    ProjectDirs::from("", "", env!("CARGO_PKG_NAME")).expect("could not get the program paths")
}

pub fn get_config_dir_path() -> PathBuf {
    PathBuf::from(project_dir().config_dir())
}
