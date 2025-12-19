mod about_widget;
mod appearance_widget;
mod general_widget;

use iced::{
    Alignment, Element, Length, Task,
    widget::{
        Id, Scrollable, column,
        scrollable::{RelativeOffset, Viewport},
    },
};

use super::Tab;
use crate::gui::assets::icons::GEAR_WIDE_CONNECTED;
use crate::gui::styles;

use about_widget::{About, Message as AboutMessage};
use appearance_widget::{Appearance, Message as AppearanceMessage};
use general_widget::{General, Message as GeneralMessage};

#[derive(Debug, Clone)]
pub enum Message {
    Appearance(AppearanceMessage),
    General(GeneralMessage),
    About(AboutMessage),
    PageScrolled(Viewport),
}

pub struct SettingsTab {
    scrollable_offset: RelativeOffset,
    scrollable_id: Id,
    appearance_settings: Appearance,
    general_settings: General,
    about: About,
}

impl SettingsTab {
    pub fn new() -> (Self, Task<Message>) {
        let (about_widget, about_task) = About::new();
        (
            Self {
                scrollable_offset: RelativeOffset::START,
                scrollable_id: Self::scrollable_id(),
                appearance_settings: Appearance,
                general_settings: General::new(),
                about: about_widget,
            },
            about_task.map(Message::About),
        )
    }

    pub fn update(
        &mut self,
        message: Message,
    ) -> Task<Message> {
        match message {
            Message::Appearance(message) => self.appearance_settings.update(message).map(Message::Appearance),
            Message::General(message) => self.general_settings.update(message).map(Message::General),
            Message::About(message) => self.about.update(message).map(Message::About),
            Message::PageScrolled(view_port) => {
                self.scrollable_offset = view_port.relative_offset();
                Task::none()
            }
        }
    }
    pub fn view(&self) -> Element<'_, Message> {
        let settings_body = Scrollable::new(
            column![
                self.appearance_settings.view().map(Message::Appearance),
                self.general_settings.view().map(Message::General),
                self.about.view().map(Message::About),
            ]
            .spacing(10)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .padding(5),
        )
        .id(self.scrollable_id.clone())
        .on_scroll(Message::PageScrolled)
        .direction(styles::scrollable_styles::vertical_direction());

        column![settings_body.height(Length::FillPortion(10))].align_x(Alignment::Center).spacing(5).into()
    }
}

impl Tab for SettingsTab {
    type Message = Message;

    fn title() -> &'static str {
        "app.tab.settings.title"
    }

    fn icon_bytes() -> &'static [u8] {
        GEAR_WIDE_CONNECTED
    }

    fn get_scrollable_offset(&self) -> RelativeOffset {
        self.scrollable_offset
    }
}

pub mod settings_config {
    use std::sync::{Arc, LazyLock, RwLock};

    use flash_cat_common::consts::{APP_CONFIG_FILE_NAME, APP_NAME, PUBLIC_RELAY};
    use log::error;
    use rust_i18n::t;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
    pub enum Theme {
        #[default]
        Light,
        Dark,
    }

    pub const ALL_THEMES: [Theme; 2] = [Theme::Light, Theme::Dark];

    impl std::fmt::Display for Theme {
        fn fmt(
            &self,
            f: &mut std::fmt::Formatter<'_>,
        ) -> std::fmt::Result {
            let str = match self {
                Theme::Light => t!("app.tab.settings.appearance-widget.theme-light").to_string(),
                Theme::Dark => t!("app.tab.settings.appearance-widget.theme-dark").to_string(),
            };

            write!(f, "{}", str)
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
    pub struct Config {
        pub appearance: AppearanceSettings,
        pub general: GeneralSettings,
    }

    impl Default for Config {
        fn default() -> Self {
            let download_path = directories::UserDirs::new().unwrap().download_dir().unwrap().to_string_lossy().to_string();
            Self {
                general: GeneralSettings {
                    download_path,
                    relay_addr: format!("https://{PUBLIC_RELAY}"),
                    i18n: "en".to_string(),
                },
                appearance: AppearanceSettings::default(),
            }
        }
    }

    #[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
    pub struct AppearanceSettings {
        pub theme: Theme,
    }

    #[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
    pub struct GeneralSettings {
        pub download_path: String,
        pub relay_addr: String,
        pub i18n: String,
    }

    pub static SETTINGS: LazyLock<Arc<RwLock<Settings>>> = LazyLock::new(|| Arc::new(RwLock::new(Settings::new())));

    pub struct Settings {
        current_config: Config,
        unsaved_config: Config,
    }

    impl Settings {
        pub fn new() -> Self {
            let config = load_config();
            Self {
                current_config: config.clone(),
                unsaved_config: config,
            }
        }

        pub fn change_settings(&mut self) -> &mut Config {
            &mut self.unsaved_config
        }

        pub fn get_current_settings(&self) -> &Config {
            &self.unsaved_config
        }

        /// Resets the settings to the initial unmodified state
        pub fn reset_settings(&mut self) {
            self.unsaved_config = self.current_config.clone();
        }

        /// Loads the default settings
        ///
        /// # Note
        /// Does not save the settings
        pub fn set_default_settings(&mut self) {
            self.unsaved_config = Config::default();
        }

        /// Checks if the unsaved settings curresponds to the
        /// default settings of the program
        pub fn has_default_settings(&self) -> bool {
            self.unsaved_config == Config::default()
        }

        pub fn has_pending_save(&self) -> bool {
            self.current_config != self.unsaved_config
        }

        pub fn save_settings(&mut self) {
            save_config(&self.unsaved_config);
            self.current_config = self.unsaved_config.clone();
        }
    }

    impl Default for Settings {
        fn default() -> Self {
            Self::new()
        }
    }

    fn load_config() -> Config {
        let cfg: Config = match confy::load(APP_NAME, APP_CONFIG_FILE_NAME) {
            Ok(cfg) => cfg,
            Err(_) => {
                let default_config = Config::default();
                let _ = confy::store(APP_NAME, APP_CONFIG_FILE_NAME, &default_config);
                return default_config;
            }
        };
        cfg
    }

    fn save_config(settings_config: &Config) {
        if let Err(err) = confy::store(APP_NAME, APP_CONFIG_FILE_NAME, &settings_config) {
            error!("Could not write config file: {}", err);
        }
    }
}
