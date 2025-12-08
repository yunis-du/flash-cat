pub mod receiver_tab;
pub mod sender_tab;
pub mod settings_tab;

use iced::{Element, Task};

use receiver_tab::{Message as ReceiverMessage, ReceiverTab};
use sender_tab::{Message as SenderMessage, SenderTab};
use settings_tab::{Message as SettingsMessage, SettingsTab};

pub trait Tab {
    type Message;

    fn title() -> &'static str;

    fn icon_bytes() -> &'static [u8];

    fn tab_label() -> TabLabel {
        TabLabel::new(Self::title(), Self::icon_bytes())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TabId {
    Sender,
    Receiver,
    Settings,
}

impl From<usize> for TabId {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::Sender,
            1 => Self::Receiver,
            2 => Self::Settings,
            _ => unreachable!("no more tabs"),
        }
    }
}

impl From<TabId> for usize {
    fn from(val: TabId) -> Self {
        match val {
            TabId::Sender => 0,
            TabId::Receiver => 1,
            TabId::Settings => 2,
        }
    }
}

impl std::fmt::Display for TabId {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let text = match self {
            TabId::Sender => "Sender",
            TabId::Receiver => "Receiver",
            TabId::Settings => "Settings",
        };

        write!(f, "{text}")
    }
}

pub struct TabLabel {
    pub text: &'static str,
    pub icon: &'static [u8],
}

impl TabLabel {
    pub fn new(
        text: &'static str,
        icon: &'static [u8],
    ) -> Self {
        Self {
            text,
            icon,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Sender(SenderMessage),
    Receiver(ReceiverMessage),
    Settings(SettingsMessage),
}

pub struct TabsController {
    current_tab: TabId,
    sender_tab: SenderTab,
    receiver_tab: ReceiverTab,
    settings_tab: SettingsTab,
}

impl TabsController {
    pub fn new() -> (Self, Task<Message>) {
        let (sender_tab, sender_task) = SenderTab::new();
        let (receiver_tab, receiver_task) = ReceiverTab::new();
        let (settings_tab, settings_task) = SettingsTab::new();

        (
            Self {
                current_tab: TabId::Sender,
                sender_tab,
                receiver_tab,
                settings_tab,
            },
            Task::batch([sender_task.map(Message::Sender), receiver_task.map(Message::Receiver), settings_task.map(Message::Settings)]),
        )
    }

    pub fn switch_to_tab(
        &mut self,
        tab: TabId,
    ) -> Task<Message> {
        self.current_tab = tab;
        Task::none()
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch([self.sender_tab.subscription().map(Message::Sender), self.receiver_tab.subscription().map(Message::Receiver)])
    }

    pub fn update(
        &mut self,
        message: Message,
    ) -> Task<Message> {
        match message {
            Message::Sender(message) => self.sender_tab.update(message).map(Message::Sender),
            Message::Receiver(message) => self.receiver_tab.update(message).map(Message::Receiver),
            Message::Settings(message) => self.settings_tab.update(message).map(Message::Settings),
        }
    }

    pub fn get_labels(&self) -> [TabLabel; 3] {
        [SenderTab::tab_label(), ReceiverTab::tab_label(), SettingsTab::tab_label()]
    }

    pub fn view(&self) -> Element<'_, Message> {
        match self.current_tab {
            TabId::Sender => self.sender_tab.view().map(Message::Sender),
            TabId::Receiver => self.receiver_tab.view().map(Message::Receiver),
            TabId::Settings => self.settings_tab.view().map(Message::Settings),
        }
    }
}
