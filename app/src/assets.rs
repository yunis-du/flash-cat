use std::borrow::Cow;

use anyhow::anyhow;
use gpui::{AssetSource, Result, SharedString};
use gpui_component::Icon;
use gpui_component_assets::Assets as ComponentAssets;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(
        &self,
        path: &str,
    ) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(f) = ComponentAssets::get(path) {
            return Ok(Some(f.data));
        }

        Self::get(path).map(|f| Some(f.data)).ok_or_else(|| anyhow!(r#"could not find asset at path "{path}""#))
    }

    fn list(
        &self,
        path: &str,
    ) -> Result<Vec<SharedString>> {
        let mut files: Vec<SharedString> = ComponentAssets::iter().filter_map(|p| p.starts_with(path).then(|| p.into())).collect();

        files.extend(Self::iter().filter_map(|p| p.starts_with(path).then(|| p.into())).collect::<Vec<_>>());

        Ok(files)
    }
}

pub enum CustomIconName {
    Logo,
    Copy,
    Sender,
    Receiver,
    Remove,
    Edit,
    Help,
}

impl CustomIconName {
    pub fn path(self) -> SharedString {
        match self {
            CustomIconName::Logo => "icons/flash-cat.svg",
            CustomIconName::Copy => "icons/copy.svg",
            CustomIconName::Sender => "icons/sender.svg",
            CustomIconName::Receiver => "icons/receiver.svg",
            CustomIconName::Remove => "icons/remove.svg",
            CustomIconName::Edit => "icons/edit.svg",
            CustomIconName::Help => "icons/help.svg",
        }
        .into()
    }
}

impl From<CustomIconName> for Icon {
    fn from(val: CustomIconName) -> Self {
        Icon::empty().path(val.path())
    }
}
