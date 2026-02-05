use std::time::{Duration, Instant};

use gpui::{App, IntoElement, ParentElement, RenderOnce, Styled, Window, div};
use gpui_component::{h_flex, label::Label, progress::Progress, v_flex};

use flash_cat_common::utils::{human_bytes, human_duration};

use crate::helpers::i18n_common;

#[derive(IntoElement, Clone)]
pub struct ProgressBar {
    file_id: u64,
    file_name: String,
    file_size: u64,
    pb: indicatif::ProgressBar,
    current_progress: u64,
    skip: bool,
    started_at: Option<Instant>,
    finished_elapsed: Option<Duration>,
}

impl ProgressBar {
    pub fn new(
        file_id: u64,
        file_name: String,
        file_size: u64,
    ) -> Self {
        let pb = indicatif::ProgressBar::new(file_size);
        Self {
            file_id,
            file_name,
            file_size,
            pb,
            current_progress: 0,
            skip: false,
            started_at: None,
            finished_elapsed: None,
        }
    }

    pub fn set_progress(
        &mut self,
        progress: u64,
    ) {
        // Record start time on first progress update
        if self.started_at.is_none() && progress > 0 {
            self.started_at = Some(Instant::now());
        }
        self.current_progress = progress;
        self.pb.set_position(progress);
    }

    pub fn skip(&mut self) {
        self.skip = true;
        self.pb.finish_and_clear();
    }

    pub fn get_file_id(&self) -> u64 {
        self.file_id
    }

    pub fn finish(&mut self) {
        self.current_progress = self.file_size;
        // Calculate elapsed from our recorded start time
        self.finished_elapsed = Some(self.started_at.map(|s| s.elapsed()).unwrap_or(Duration::ZERO));
        self.pb.finish();
    }
}

impl RenderOnce for ProgressBar {
    fn render(
        self,
        _window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let v_flex = v_flex().w_full();

        if self.skip {
            v_flex.child(Label::new(i18n_common(cx, "skip")).text_sm()).into_any_element()
        } else {
            let precent = if self.file_size == 0 {
                0.0
            } else {
                (self.current_progress as f32) / (self.file_size as f32)
            };
            let per_sec = self.pb.per_sec();

            v_flex
                .child(Label::new(self.file_name).text_sm())
                .child(
                    h_flex().justify_between().child(div().flex_1().max_w_40().child(Progress::new().value(precent * 100.0))).child(
                        Label::new(if self.current_progress >= self.file_size {
                            // Finished: show elapsed time (use recorded elapsed to avoid continued counting)
                            let elapsed = self.finished_elapsed.unwrap_or_else(|| self.started_at.map(|s| s.elapsed()).unwrap_or(Duration::ZERO));
                            format!("{} • in {}", human_bytes(self.file_size), human_duration(elapsed))
                        } else {
                            // In progress: show ETA
                            format!(
                                "{}/{} • {}/s • ETA {}",
                                human_bytes(self.current_progress),
                                human_bytes(self.file_size),
                                human_bytes(per_sec as u64),
                                if self.current_progress == 0 {
                                    human_duration(std::time::Duration::ZERO)
                                } else {
                                    human_duration(self.pb.eta())
                                },
                            )
                        })
                        .text_xs()
                        .flex_shrink_0(),
                    ),
                )
                .into_any_element()
        }
    }
}
