use flash_cat_common::utils::{human_bytes, human_duration};
use iced::{
    widget::{horizontal_space, progress_bar, row, text},
    Element,
};

#[derive(Clone, Debug)]
pub enum Message {}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Sending(f32),
    Skip,
    Finished,
}

#[derive(Debug, Clone)]
pub struct ProgressBar {
    file_id: u64,
    file_size: u64,
    num_files: u64,
    pb: indicatif::ProgressBar,
    per_sec: u64,
    state: State,
}

impl ProgressBar {
    pub fn new(file_id: u64, file_size: u64, num_files: u64) -> Self {
        let pb = indicatif::ProgressBar::new(file_size);
        Self {
            file_id,
            file_size,
            num_files,
            pb,
            per_sec: 0,
            state: State::Idle,
        }
    }

    pub fn get_id(&self) -> u64 {
        self.file_id
    }

    pub fn start(&mut self) {
        match &self.state {
            State::Idle { .. } => {
                self.state = State::Sending(0.0);
            }
            _ => {}
        }
    }

    pub fn update_state(&mut self, new_state: Option<State>) {
        if new_state.is_some() {
            let new_state = new_state.unwrap();
            match new_state {
                State::Skip => self.pb.finish(),
                State::Finished => {
                    self.per_sec = self.pb.per_sec() as u64;
                    self.pb.finish();
                }
                _ => (),
            }
            self.state = new_state;
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::none()
    }

    pub fn view(&self) -> Element<Message> {
        let (current_progress, per_sec) = match &self.state {
            State::Idle => (0.0, 0),
            State::Sending(progress) => (*progress, self.pb.per_sec() as u64),
            State::Skip => (0.0, 0),
            State::Finished => (self.file_size as f32, self.per_sec),
        };

        if self.state.eq(&State::Skip) {
            text("Skip").into()
        } else {
            self.pb.set_position(current_progress as u64);
            row![
                progress_bar(0.0..=self.file_size as f32, current_progress)
                    .height(12)
                    .width(200),
                horizontal_space(),
                text(format!(
                    "{}/{} ({}/s, {}) {}/{}",
                    human_bytes(current_progress as u64),
                    human_bytes(self.file_size as u64),
                    human_bytes(per_sec),
                    if current_progress == 0.0 {
                        human_duration(std::time::Duration::ZERO)
                    } else {
                        human_duration(self.pb.eta())
                    },
                    self.file_id,
                    self.num_files,
                ))
                .size(12)
            ]
            .spacing(5)
            .align_items(iced::Alignment::Center)
            .into()
        }
    }
}
