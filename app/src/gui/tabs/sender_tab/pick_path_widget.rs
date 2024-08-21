use iced::{
    widget::{button, horizontal_space, row, svg, text},
    Command, Element, Length,
};

use crate::{
    gui::{assets::icons::REMOVE_ICON, styles},
    message::IndexedMessage,
};

use super::{SenderState, SENDER_STATE};

#[derive(Clone, Debug)]
pub enum Message {
    Remove,
}

pub struct PickPath {
    path: String,
}

impl PickPath {
    pub fn new(path: impl ToString) -> Self {
        Self {
            path: path.to_string(),
        }
    }

    pub fn get_path(&self) -> String {
        self.path.clone()
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Remove => Command::none(),
        }
    }

    pub fn view(&self) -> Element<'_, IndexedMessage<String, Message>> {
        let element = row![
            text(&self.path).style(styles::text_styles::accent_color_theme()),
            horizontal_space(),
            if !SENDER_STATE.read().unwrap().eq(&SenderState::Sending) {
                remove_button()
            } else {
                progress_bar()
            }
        ]
        .align_items(iced::Alignment::Center);

        let element: Element<'_, Message> = element.into();
        element.map(|message| IndexedMessage::new(self.path.clone(), message))
    }
}

impl PartialEq for PickPath {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

fn remove_button() -> Element<'static, Message> {
    let remove_icon_handle = svg::Handle::from_memory(REMOVE_ICON);
    let remove_icon = svg(remove_icon_handle)
        .style(styles::svg_styles::colored_svg_theme())
        .width(Length::Shrink);
    button(remove_icon)
        .style(styles::button_styles::transparent_button_theme())
        .on_press(Message::Remove)
        .into()
}

fn progress_bar() -> Element<'static, Message> {
    text("progress_bar").into()
}
