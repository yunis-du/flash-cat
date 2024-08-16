use anyhow::Result;
use flash_cat_app::gui::{assets, FlashCatApp};
use iced::{window, Application, Settings, Size};

fn main() -> Result<()> {
    let icon = window::icon::from_file_data(assets::logos::IMG_LOGO, None).ok();

    FlashCatApp::run(Settings {
        window: iced::window::Settings {
            size: Size::new(480.0, 660.0),
            icon,
            ..Default::default()
        },
        default_text_size: 14.0.into(),
        ..Default::default()
    })?;
    Ok(())
}
