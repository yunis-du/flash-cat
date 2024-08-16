use iced::{
    widget::scrollable::{self, RelativeOffset},
    Command, Element,
};

use super::Tab;
use crate::gui::{assets::icons::RECEIVER_ICON, styles};
use iced::widget::{column, mouse_area, row, text};

#[derive(Debug, Clone)]
pub enum Message {
    Received,
}

pub struct ReceiverTab {
    scrollable_offset: RelativeOffset,
}

impl ReceiverTab {
    pub fn new() -> (Self, Command<Message>) {
        (
            Self {
                scrollable_offset: RelativeOffset::START,
            },
            Command::none(),
        )
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Received => Command::none(),
        }
    }

    pub fn view(&self) -> Element<Message> {
        let go_to_site_text = text("here")
            .size(11)
            .style(styles::text_styles::accent_color_theme());

        let receiver_text = row![
            text("- Receiver ").size(11),
            mouse_area(go_to_site_text.clone())
        ];

        column![receiver_text].into()
    }
}

impl Tab for ReceiverTab {
    type Message = Message;

    fn title() -> &'static str {
        "Receiver"
    }

    fn icon_bytes() -> &'static [u8] {
        RECEIVER_ICON
    }

    fn get_scrollable_offset(&self) -> scrollable::RelativeOffset {
        self.scrollable_offset
    }
}
