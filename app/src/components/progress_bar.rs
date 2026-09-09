use crate::helpers::i18n_common;
use flash_cat_common::utils::{human_bytes, human_duration};
use flash_cat_core::TransferPhase;
use gpui_kit::component::{h_flex, label::Label, progress::Progress, v_flex};
use gpui_kit::{App, IntoElement, ParentElement, RenderOnce, Styled, Window, div};
use std::time::{Duration, Instant};

#[derive(IntoElement, Clone)]
pub struct ProgressBar {
    file_id: u64,
    file_name: String,
    file_size: u64,
    current_progress: u64,
    baseline: u64,
    phase: TransferPhase,
    skip: bool,
    error: Option<String>,
    started_at: Option<Instant>,
    finished_elapsed: Option<Duration>,
}
impl ProgressBar {
    pub fn new(
        file_id: u64,
        file_name: String,
        file_size: u64,
    ) -> Self {
        Self {
            file_id,
            file_name,
            file_size,
            current_progress: 0,
            baseline: 0,
            phase: TransferPhase::Waiting,
            skip: false,
            error: None,
            started_at: None,
            finished_elapsed: None,
        }
    }
    fn terminal(&self) -> bool {
        self.skip || self.error.is_some() || self.finished_elapsed.is_some()
    }
    pub fn set_stage(
        &mut self,
        phase: TransferPhase,
        position: u64,
    ) {
        if self.terminal() {
            return;
        }
        self.phase = phase;
        self.current_progress = position.min(self.file_size);
        if phase == TransferPhase::Transferring {
            self.baseline = position;
            self.started_at = Some(Instant::now());
        }
    }
    pub fn set_progress(
        &mut self,
        progress: u64,
    ) {
        if !self.terminal() {
            self.current_progress = progress.min(self.file_size);
        }
    }
    pub fn fail(
        &mut self,
        error: String,
    ) {
        self.error = Some(error);
    }
    pub fn skip(&mut self) {
        self.skip = true;
    }
    pub fn finish(&mut self) {
        self.current_progress = self.file_size;
        self.finished_elapsed = Some(self.started_at.map(|s| s.elapsed()).unwrap_or(Duration::ZERO));
    }
}
impl RenderOnce for ProgressBar {
    fn render(
        self,
        _window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let percent = if self.finished_elapsed.is_some() {
            100.0
        } else if self.file_size == 0 {
            0.0
        } else {
            (self.current_progress as f64 / self.file_size as f64 * 100.0).min(99.9) as f32
        };
        let label = if self.skip {
            i18n_common(cx, "skip").to_string()
        } else if let Some(error) = &self.error {
            format!("{}: {error}", i18n_common(cx, "failed"))
        } else if let Some(elapsed) = self.finished_elapsed {
            format!(
                "{} • {} • {}",
                i18n_common(cx, "succeeded"),
                human_bytes(self.file_size),
                human_duration(elapsed)
            )
        } else {
            match self.phase {
                TransferPhase::Preparing => i18n_common(cx, "preparing_file").to_string(),
                TransferPhase::Waiting => i18n_common(cx, "waiting_receiver").to_string(),
                TransferPhase::Saving => i18n_common(cx, "saving").to_string(),
                TransferPhase::Transferring => {
                    let seconds = self.started_at.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
                    let speed = if seconds > 0.0 {
                        self.current_progress.saturating_sub(self.baseline) as f64 / seconds
                    } else {
                        0.0
                    };
                    let eta = if speed > 0.0 && seconds >= 0.2 {
                        human_duration(Duration::from_secs_f64(
                            ((self.file_size - self.current_progress) as f64 / speed).min(315360000.0),
                        ))
                    } else {
                        "—".to_owned()
                    };
                    format!(
                        "{}/{} • {}/s • ETA {eta}",
                        human_bytes(self.current_progress),
                        human_bytes(self.file_size),
                        human_bytes(speed as u64)
                    )
                }
            }
        };
        v_flex().w_full().child(Label::new(self.file_name).text_sm().truncate()).child(
            h_flex()
                .justify_between()
                .gap_2()
                .child(div().flex_1().max_w_40().child(Progress::new(("file-progress", self.file_id)).value(percent)))
                .child(Label::new(label).text_xs()),
        )
    }
}
