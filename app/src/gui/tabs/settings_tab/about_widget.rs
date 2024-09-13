use crate::gui::assets::icons::GITHUB_ICON;
use crate::gui::styles;

use flash_cat_common::APP_VERSION;
use iced::widget::{button, column, container, mouse_area, row, svg, text};
use iced::{mouse, Command, Element, Length};
use iced_aw::{Grid, GridRow};
use log::error;

#[derive(Debug, Clone)]
pub enum Message {
    Repository,
}

pub struct About {}

impl About {
    pub fn new() -> (Self, Command<Message>) {
        (Self {}, Command::none())
    }
    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Repository => {
                webbrowser::open(built_info::PKG_REPOSITORY)
                    .unwrap_or_else(|err| error!("failed to open repository site: {}", err));
            }
        };

        Command::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = column![
            text("About")
                .style(styles::text_styles::accent_color_theme())
                .size(21),
            info_widget(),
            social_buttons(),
        ]
        .spacing(10);

        container(content)
            .style(styles::container_styles::first_class_container_rounded_theme())
            .width(1000)
            .padding(5)
            .into()
    }
}

fn info_widget() -> Element<'static, Message> {
    let mut grid = Grid::new();

    grid = grid.push(
        GridRow::new()
            .push(text("Author"))
            .push(text(built_info::PKG_AUTHORS)),
    );

    grid = grid.push(
        GridRow::new()
            .push(text("Version"))
            .push(text(APP_VERSION)),
    );

    grid = grid.push(
        GridRow::new()
            .push(text("License"))
            .push(text(built_info::PKG_LICENSE)),
    );

    let repository = mouse_area(
        text(built_info::PKG_REPOSITORY).style(styles::text_styles::accent_color_theme()),
    )
    .interaction(mouse::Interaction::Pointer)
    .on_press(Message::Repository);

    grid = grid.push(GridRow::new().push(text("Repository")).push(repository));

    if built_info::GIT_DIRTY.unwrap_or(false) {
        if let Some(commit_hash) = built_info::GIT_COMMIT_HASH {
            grid = grid.push(
                GridRow::new()
                    .push(text("Commit Hash"))
                    .push(text(commit_hash)),
            );
        }
    }
    grid = grid.push(
        GridRow::new()
            .push(text("Build Time"))
            .push(text(built_info::BUILT_TIME_UTC)),
    );

    grid.into()
}

fn social_buttons() -> Element<'static, Message> {
    let github_icon_handle = svg::Handle::from_memory(GITHUB_ICON);
    let github_icon = svg(github_icon_handle)
        .style(styles::svg_styles::colored_svg_theme())
        .height(30)
        .width(30);
    let github_button = button(github_icon)
        .style(styles::button_styles::transparent_button_theme())
        .on_press(Message::Repository);

    let social_buttons = row![github_button].spacing(5);

    container(social_buttons)
        .width(Length::Fill)
        .center_x()
        .center_y()
        .into()
}

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
