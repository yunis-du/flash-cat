use anyhow::Result;
use flash_cat_common::consts::PUBLIC_RELAY;
use gpui::{App, AppContext, Bounds, Context, Entity, Global, Pixels};
use gpui_component::ThemeMode;
use locale_config::Locale;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::helpers::{get_or_create_config_path, get_user_download_dir};

const LIGHT_THEME_MODE: &str = "light";
const DARK_THEME_MODE: &str = "dark";

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum Route {
    #[default]
    Send,
    Receive,
    Settings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlashCatAppState {
    route: Route,
    locale: Option<String>,
    relay_address: Option<String>,
    save_path: Option<String>,
    bounds: Option<Bounds<Pixels>>,
    theme: Option<String>,
}

impl FlashCatAppState {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn try_new() -> Result<Self> {
        let path = get_or_create_config_path()?;
        let value = std::fs::read_to_string(path)?;
        let mut state: Self = toml::from_str(&value)?;
        if state.locale.clone().unwrap_or_default().is_empty()
            && let Some((lang, _)) = Locale::current().to_string().split_once("-")
        {
            state.locale = Some(lang.to_string());
        }
        Ok(state)
    }

    pub fn route(&self) -> Route {
        self.route
    }

    pub fn theme(&self) -> Option<ThemeMode> {
        match self.theme.as_deref() {
            Some(LIGHT_THEME_MODE) => Some(ThemeMode::Light),
            Some(DARK_THEME_MODE) => Some(ThemeMode::Dark),
            _ => None,
        }
    }

    pub fn locale(&self) -> &str {
        self.locale.as_deref().unwrap_or("en")
    }

    pub fn relay_address(&self) -> String {
        self.relay_address.clone().unwrap_or_else(|| format!("https://{PUBLIC_RELAY}"))
    }

    pub fn save_path(&self) -> String {
        self.save_path.clone().unwrap_or_else(|| get_user_download_dir())
    }

    pub fn set_theme(
        &mut self,
        theme: Option<ThemeMode>,
    ) {
        match theme {
            Some(ThemeMode::Light) => {
                self.theme = Some(LIGHT_THEME_MODE.to_string());
            }
            Some(ThemeMode::Dark) => {
                self.theme = Some(DARK_THEME_MODE.to_string());
            }
            _ => {
                self.theme = None;
            }
        }
    }

    pub fn set_locale(
        &mut self,
        locale: String,
    ) {
        self.locale = Some(locale);
    }

    pub fn set_relay_address(
        &mut self,
        relay_address: String,
    ) {
        self.relay_address = Some(relay_address);
    }

    pub fn set_save_path(
        &mut self,
        save_path: String,
    ) {
        self.save_path = Some(save_path);
    }

    pub fn go_to(
        &mut self,
        route: Route,
    ) {
        if self.route != route {
            self.route = route;
        }
    }
}

#[derive(Debug, Clone)]
pub struct FlashCatAppGlobalStore {
    app_state: Entity<FlashCatAppState>,
}

impl FlashCatAppGlobalStore {
    pub fn new(app_state: Entity<FlashCatAppState>) -> Self {
        Self {
            app_state,
        }
    }

    // pub fn state(&self) -> Entity<FlashCatAppState> {
    //     self.app_state.clone()
    // }

    // pub fn value(
    //     &self,
    //     cx: &App,
    // ) -> FlashCatAppState {
    //     self.app_state.read(cx).clone()
    // }

    pub fn update<R, C: AppContext>(
        &self,
        cx: &mut C,
        update: impl FnOnce(&mut FlashCatAppState, &mut Context<FlashCatAppState>) -> R,
    ) -> C::Result<R> {
        self.app_state.update(cx, update)
    }

    pub fn read<'a>(
        &self,
        cx: &'a App,
    ) -> &'a FlashCatAppState {
        self.app_state.read(cx)
    }
}

impl Global for FlashCatAppGlobalStore {}

pub fn save_app_state(state: &FlashCatAppState) -> Result<()> {
    let path = get_or_create_config_path()?;
    let value = toml::to_string(state)?;
    std::fs::write(path, value)?;
    Ok(())
}

#[inline]
pub fn update_app_state_and_save<F>(
    cx: &App,
    action_name: &'static str,
    mutation: F,
) where
    F: FnOnce(&mut FlashCatAppState, &App) + Send + 'static + Clone,
{
    let store = cx.global::<FlashCatAppGlobalStore>().clone();

    cx.spawn(async move |cx| {
        // update global state with the mutation
        let current_state = store.update(cx, |state, cx| {
            mutation(state, cx);
            state.clone() // Return clone for async persistence
        });

        // persist to disk in background executor
        if let Ok(state) = current_state {
            cx.background_executor()
                .spawn(async move {
                    if let Err(e) = save_app_state(&state) {
                        error!(error = %e, action = action_name, "Failed to save state");
                    } else {
                        info!(action = action_name, "State saved successfully");
                    }
                })
                .await;
        }

        // refresh windows to apply visual changes (theme/locale)
        cx.update(|cx| cx.refresh_windows()).ok();
    })
    .detach();
}
