#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use flash_cat_app::gui::{assets, FlashCatApp};
use iced::{window, Application, Font, Settings, Size};

fn main() -> Result<()> {
    let icon = window::icon::from_file_data(assets::logos::IMG_LOGO, None).ok();

    FlashCatApp::run(Settings {
        window: iced::window::Settings {
            size: Size::new(480.0, 700.0),
            icon,
            resizable: false,
            ..Default::default()
        },
        default_text_size: 14.0.into(),
        fonts: assets::fonts::FONTS.into(),
        default_font: Font::with_name(assets::fonts::SOURCE_HAN_SANS_CN),
        ..Default::default()
    })?;
    Ok(())
}
