mod about_widget;
mod appearance_widget;
mod general_widget;

use about_widget::{About, Message as AboutMessage};
use appearance_widget::{Appearance, Message as AppearanceMessage};
use general_widget::{General, Message as GeneralMessage};
use iced::{
    widget::scrollable::{RelativeOffset, Viewport},
    widget::{column, scrollable},
    Command, Element,
};
use iced::{Alignment, Length};

use crate::gui::assets::icons::GEAR_WIDE_CONNECTED;
use crate::gui::styles;

use super::Tab;

#[derive(Debug, Clone)]
pub enum Message {
    Appearance(AppearanceMessage),
    General(GeneralMessage),
    About(AboutMessage),
    PageScrolled(Viewport),
}

pub struct SettingsTab {
    appearance_settings: Appearance,
    general_settings: General,
    about: About,
    scrollable_offset: RelativeOffset,
}

impl SettingsTab {
    pub fn new() -> (Self, Command<Message>) {
        let (about_widget, about_command) = About::new();
        (
            Self {
                appearance_settings: Appearance,
                general_settings: General::new(),
                about: about_widget,
                scrollable_offset: RelativeOffset::START,
            },
            about_command.map(Message::About),
        )
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Appearance(message) => self
                .appearance_settings
                .update(message)
                .map(Message::Appearance),
            Message::General(message) => {
                self.general_settings.update(message).map(Message::General)
            }
            Message::About(message) => self.about.update(message).map(Message::About),
            Message::PageScrolled(view_port) => {
                self.scrollable_offset = view_port.relative_offset();
                Command::none()
            }
        }
    }
    pub fn view(&self) -> Element<Message> {
        let settings_body = scrollable(
            column![
                self.appearance_settings.view().map(Message::Appearance),
                self.general_settings.view().map(Message::General),
                self.about.view().map(Message::About),
            ]
            .spacing(10)
            .width(Length::Fill)
            .align_items(Alignment::Center)
            .padding(5),
        )
        .id(Self::scrollable_id())
        .on_scroll(Message::PageScrolled)
        .direction(styles::scrollable_styles::vertical_direction());

        column![settings_body.height(Length::FillPortion(10))]
            .align_items(Alignment::Center)
            .spacing(5)
            .into()
    }
}

impl Tab for SettingsTab {
    type Message = Message;

    fn title() -> &'static str {
        "Settings"
    }

    fn icon_bytes() -> &'static [u8] {
        GEAR_WIDE_CONNECTED
    }

    fn get_scrollable_offset(&self) -> scrollable::RelativeOffset {
        self.scrollable_offset
    }
}

pub mod settings_config {
    use flash_cat_common::consts::PUBLIC_RELAY;
    use std::{
        io::ErrorKind,
        sync::{Arc, LazyLock, RwLock},
    };

    use log::{error, info, warn};
    use serde::{Deserialize, Serialize};

    use crate::folder::get_config_dir_path;

    #[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
    pub enum Theme {
        #[default]
        Light,
        Dark,
    }

    pub const ALL_THEMES: [Theme; 2] = [Theme::Light, Theme::Dark];

    impl std::fmt::Display for Theme {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let str = match self {
                Theme::Light => "Light",
                Theme::Dark => "Dark",
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
            let download_path = directories::UserDirs::new()
                .unwrap()
                .download_dir()
                .unwrap()
                .to_string_lossy()
                .to_string();
            Self {
                general: GeneralSettings {
                    download_path,
                    relay_addr: format!("https://{PUBLIC_RELAY}"),
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
    }

    pub static SETTINGS: LazyLock<Arc<RwLock<Settings>>> =
        LazyLock::new(|| Arc::new(RwLock::new(Settings::new())));

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

    pub const CONFIG_FILE_NAME: &str = "config.toml";

    fn load_config() -> Config {
        let config_directory = get_config_dir_path();

        let mut config_file = config_directory.clone();
        config_file.push(CONFIG_FILE_NAME);

        info!("loading config file at: '{}'", config_file.display());

        let file_contents = match std::fs::read_to_string(&config_file) {
            Ok(file_contents) => file_contents,
            Err(err) => {
                let default_config = Config::default();
                if let ErrorKind::NotFound = err.kind() {
                    warn!("could not find config file at: '{}'", config_file.display());
                    std::fs::DirBuilder::new()
                        .recursive(true)
                        .create(config_directory)
                        .unwrap_or_else(|err| error!("could not create config directory: {err}"));
                    std::fs::write(
                        &config_file,
                        toml::to_string_pretty(&default_config).unwrap(),
                    )
                    .unwrap_or_else(|err| error!("could not write default config file: {err}"));
                    info!(
                        "created a new default config file at: '{}'",
                        config_file.display()
                    );
                }
                return default_config;
            }
        };

        match toml::from_str(&file_contents) {
            Ok(config) => config,
            Err(err) => {
                error!("could not parse the config file: {}", err);
                warn!("loading with default settings");
                Config::default()
            }
        }
    }

    fn save_config(settings_config: &Config) {
        let config_directory = get_config_dir_path();

        let mut config_file = config_directory.clone();
        config_file.push(CONFIG_FILE_NAME);

        if let Err(err) = std::fs::write(
            &config_file,
            toml::to_string_pretty(&settings_config).unwrap(),
        ) {
            error!(
                "Could not write default config file '{}': {}",
                config_file.display(),
                err
            );
        }
    }
}