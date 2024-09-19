#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use anyhow::Result;
use flash_cat_app::gui::{assets, FlashCatApp};
use iced::{window, Font, Settings, Size};

fn main() -> Result<()> {
    let icon = window::icon::from_file_data(assets::logos::ICON_LOGO, None).ok();

    iced::application(FlashCatApp::title, FlashCatApp::update, FlashCatApp::view)
        .subscription(FlashCatApp::subscription)
        .theme(FlashCatApp::theme)
        .window(iced::window::Settings {
            size: Size::new(480.0, 700.0),
            icon,
            resizable: false,
            ..Default::default()
        })
        .settings(Settings {
            default_text_size: 14.0.into(),
            fonts: assets::fonts::FONTS.into(),
            default_font: Font::with_name(assets::fonts::SOURCE_HAN_SANS_CN),
            ..Default::default()
        })
        .run()?;
    Ok(())
}
