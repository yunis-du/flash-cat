mod about;
mod content;
mod header;
mod receive;
mod send;
mod settings;
mod tab;
mod title_bar;

pub use about::open_about_window;
pub use content::FlashCatAppContent;
pub use header::FlashCatAppHeader;
pub use receive::ReceiveView;
pub use send::SendView;
pub use settings::SettingsView;
pub use tab::TabView;
pub use title_bar::TitleBarView;
