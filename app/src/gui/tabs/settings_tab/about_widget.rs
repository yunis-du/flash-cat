use iced::{
    Element, Length, Task, mouse,
    widget::{button, column, container, grid, mouse_area, row, svg, text},
};
use log::error;
use rust_i18n::t;

use crate::gui::{assets::icons::GITHUB_ICON, styles};

#[derive(Debug, Clone)]
pub enum Message {
    Repository,
}

pub struct About {}

impl About {
    pub fn new() -> (Self, Task<Message>) {
        (Self {}, Task::none())
    }

    pub fn update(
        &mut self,
        message: Message,
    ) -> Task<Message> {
        match message {
            Message::Repository => {
                webbrowser::open(built_info::PKG_REPOSITORY).unwrap_or_else(|err| error!("failed to open repository site: {}", err));
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content =
            column![text(t!("app.tab.settings.about-widget.title")).style(styles::text_styles::accent_color_theme).size(21), info_widget(), social_buttons(),]
                .spacing(10);

        container(content).style(styles::container_styles::first_class_container_rounded_theme).width(Length::Fill).padding(5).into()
    }
}

fn info_widget() -> Element<'static, Message> {
    let repository = mouse_area(text(built_info::PKG_REPOSITORY).style(styles::text_styles::accent_color_theme))
        .interaction(mouse::Interaction::Pointer)
        .on_press(Message::Repository);

    let mut grid = grid![
        // author
        row![text(t!("app.tab.settings.about-widget.author")).width(120), text(built_info::PKG_AUTHORS)],
        // version
        row![text(t!("app.tab.settings.about-widget.version")).width(120), text(built_info::PKG_VERSION)],
        // license
        row![text(t!("app.tab.settings.about-widget.license")).width(120), text(built_info::PKG_LICENSE)],
        // repository
        row![text(t!("app.tab.settings.about-widget.repository")).width(120), repository],
    ]
    .height(grid::Sizing::EvenlyDistribute(Length::Shrink))
    .columns(1);

    if let Some(commit_hash) = built_info::GIT_COMMIT_HASH {
        grid = grid.push(row![
            text(t!("app.tab.settings.about-widget.commit-hash")).width(120),
            text(commit_hash)
        ]);
    }

    grid = grid.push(row![
        text(t!("app.tab.settings.about-widget.built-time")).width(120),
        text(built_info::BUILT_TIME_UTC)
    ]);

    grid.into()
}

fn social_buttons() -> Element<'static, Message> {
    let github_icon_handle = svg::Handle::from_memory(GITHUB_ICON);
    let github_icon = svg(github_icon_handle).style(styles::svg_styles::colored_svg_theme).height(30).width(30);
    let github_button = button(github_icon).style(styles::button_styles::transparent_button_theme).on_press(Message::Repository);

    let social_buttons = row![github_button].spacing(5);

    container(social_buttons).width(Length::Fill).center_x(Length::Fill).into()
}

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
