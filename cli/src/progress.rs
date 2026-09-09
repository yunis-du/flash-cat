use flash_cat_common::format::HumanDuration;
use flash_cat_core::{FileStage, RelayType, TransferPhase};
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::{
    borrow::Cow,
    collections::HashMap,
    time::{Duration, Instant},
};

const REFRESH: Duration = Duration::from_millis(80);

/// Keep terminal redraws out of an interactive prompt without holding a drawing
/// lock while waiting for input. Dropping the prompt future also restores output.
pub(crate) struct PromptDisplay {
    multi: MultiProgress,
    bars: Vec<ProgressBar>,
}

impl Drop for PromptDisplay {
    fn drop(&mut self) {
        self.multi.set_draw_target(ProgressDrawTarget::stderr());
        if let Some(bar) = self.bars.first() {
            bar.force_draw();
        }
    }
}
struct FileProgress {
    name: String,
    size: u64,
    position: u64,
    baseline: u64,
    started: Option<Instant>,
    terminal: bool,
}
pub struct Progress {
    num_files: u64,
    name_width: usize,
    total_size: u64,
    processed: u64,
    transferred: u64,
    transfer_started: Option<Instant>,
    finished: u64,
    multi: MultiProgress,
    total: Option<ProgressBar>,
    files: HashMap<u64, FileProgress>,
    bars: HashMap<u64, ProgressBar>,
}

impl Drop for Progress {
    fn drop(&mut self) {
        // Keep the last positions visible when an error or cancellation drops
        // the transfer future. Finishing would incorrectly fill the bars to 100%.
        for bar in self.bars.values() {
            bar.disable_steady_tick();
            bar.abandon_with_message("Interrupted");
        }
        if let Some(total) = &self.total {
            total.abandon_with_message(format!("{}/{} • Interrupted", self.finished, self.num_files));
        }
    }
}

impl Progress {
    pub(crate) fn pause_for_prompt(&self) -> PromptDisplay {
        let bars = self.bars.values().cloned().collect::<Vec<_>>();
        let _ = self.multi.clear();
        self.multi.set_draw_target(ProgressDrawTarget::hidden());
        PromptDisplay {
            multi: self.multi.clone(),
            bars,
        }
    }

    pub fn new(
        num_files: u64,
        name_len: usize,
        total_size: u64,
    ) -> Self {
        Self {
            num_files,
            name_width: name_len.min(48),
            total_size,
            processed: 0,
            transferred: 0,
            transfer_started: None,
            finished: 0,
            multi: MultiProgress::new(),
            total: None,
            files: HashMap::new(),
            bars: HashMap::new(),
        }
    }
    pub fn update(
        &mut self,
        num_files: u64,
        name_len: usize,
        total_size: u64,
    ) {
        self.num_files = num_files;
        self.name_width = name_len.min(48);
        self.total_size = total_size;
    }
    pub fn add_spinner(
        &self,
        msg: impl Into<Cow<'static, str>>,
    ) -> ProgressBar {
        let spinner = self.multi.add(ProgressBar::new_spinner().with_message(msg));
        spinner.set_style(ProgressStyle::with_template("{spinner:.green} {msg}").unwrap());
        spinner.enable_steady_tick(REFRESH);
        spinner
    }
    pub fn register_file(
        &mut self,
        name: &str,
        id: u64,
        size: u64,
    ) {
        self.files.entry(id).or_insert_with(|| FileProgress {
            name: name.to_owned(),
            size,
            position: 0,
            baseline: 0,
            started: None,
            terminal: false,
        });
    }
    pub fn add_progress(
        &mut self,
        name: &str,
        id: u64,
        size: u64,
    ) {
        self.register_file(name, id, size);
        self.ensure_bar(id);
    }
    fn ensure_bar(
        &mut self,
        id: u64,
    ) {
        if self.bars.contains_key(&id) || self.files.get(&id).is_none_or(|f| f.terminal) {
            return;
        }
        if self.total.is_none() && self.num_files > 1 {
            let prefix = format!("{:<width$}", "Total", width = self.name_width);
            let bar = self.multi.add(ProgressBar::new(self.total_size).with_prefix(prefix));
            bar.set_style(
                ProgressStyle::with_template("  ------------------------\n  {prefix:.bold.green} [{bar:50.cyan/blue}] {bytes}/{total_bytes} • {msg}")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            self.total = Some(bar);
            self.update_total();
        }
        if let Some(file) = self.files.get(&id) {
            let name = format!("{:<width$}", truncate(&file.name, 48), width = self.name_width);
            let bar = ProgressBar::new(file.size).with_prefix(name).with_message("Waiting");
            bar.set_style(
                ProgressStyle::with_template("{spinner:.green} {prefix:.bold.green} [{bar:50.cyan/blue}] {bytes}/{total_bytes} • {msg}")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            let bar = match &self.total {
                Some(total) => self.multi.insert_before(total, bar),
                None => self.multi.add(bar),
            };
            self.bars.insert(id, bar);
        }
    }
    pub fn set_stage(
        &mut self,
        stage: FileStage,
    ) {
        if self.files.get(&stage.file_id).is_none_or(|f| f.terminal) {
            return;
        }
        self.ensure_bar(stage.file_id);
        if stage.phase == TransferPhase::Transferring {
            self.transfer_started.get_or_insert_with(Instant::now);
            if let Some(file) = self.files.get_mut(&stage.file_id) {
                self.processed = self.processed.saturating_sub(file.position).saturating_add(stage.position);
                file.position = stage.position;
                file.baseline = stage.position;
                file.started = Some(Instant::now());
            }
        } else if stage.phase == TransferPhase::Saving {
            self.set_position(stage.file_id, stage.position);
        }
        if let Some(bar) = self.bars.get(&stage.file_id) {
            bar.set_position(stage.position);
            bar.set_message(match stage.phase {
                TransferPhase::Preparing => "Preparing file",
                TransferPhase::Waiting => "Waiting for receiver",
                TransferPhase::Transferring => "Transferring • ETA —",
                TransferPhase::Saving => "Saving at receiver",
            });
        }
        self.update_total();
        self.draw(stage.file_id);
    }
    pub fn set_position(
        &mut self,
        id: u64,
        position: u64,
    ) {
        self.ensure_bar(id);
        if let Some(file) = self.files.get_mut(&id) {
            if file.terminal {
                return;
            }
            let position = position.min(file.size);
            self.transferred += position.saturating_sub(file.position);
            self.processed = self.processed.saturating_sub(file.position).saturating_add(position);
            file.position = position;
            if let Some(bar) = self.bars.get(&id) {
                bar.set_position(position);
                let elapsed = file.started.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
                let bytes = position.saturating_sub(file.baseline);
                let speed = if elapsed > 0.0 {
                    bytes as f64 / elapsed
                } else {
                    0.0
                };
                let eta = if elapsed >= 0.2 && speed > 0.0 {
                    format!(
                        "{:#}",
                        HumanDuration(Duration::from_secs_f64(
                            ((file.size - position) as f64 / speed).min(315360000.0)
                        ))
                    )
                } else {
                    "—".into()
                };
                bar.set_message(format!("{}/s • ETA {eta}", HumanBytes(speed as u64)));
            }
        }
        self.update_total();
        self.draw(id);
    }
    // Core throttles intermediate progress events. Render the complete state
    // once at the end of each event, without a timer for every file bar.
    fn draw(
        &self,
        id: u64,
    ) {
        if let Some(bar) = self.total.as_ref().or_else(|| self.bars.get(&id)) {
            bar.force_draw();
        }
    }
    fn update_total(&self) {
        if let Some(total) = &self.total {
            let remaining = self.total_size.saturating_sub(self.processed);
            let elapsed = self.transfer_started.map(|start| start.elapsed().as_secs_f64()).unwrap_or(0.0);
            // Only bytes sent during this run contribute to throughput. Skipped
            // files and existing resume data reduce the remaining work, not time.
            let eta = if self.finished == self.num_files {
                "0s".to_owned()
            } else if remaining > 0 && elapsed >= 0.2 && self.transferred > 0 {
                let seconds = remaining as f64 * elapsed / self.transferred as f64;
                format!("{:#}", HumanDuration(Duration::from_secs_f64(seconds.min(315360000.0))))
            } else {
                "—".to_owned()
            };
            total.set_position(self.processed.min(self.total_size));
            total.set_message(format!("{}/{} • ETA {}", self.finished, self.num_files, eta));
        }
    }
    pub fn finish(
        &mut self,
        id: u64,
    ) {
        if self.files.get(&id).is_none_or(|f| f.terminal) {
            return;
        }
        let size = self.files[&id].size;
        self.set_position(id, size);
        self.complete(id, "Succeeded");
    }
    pub fn skip(
        &mut self,
        id: u64,
    ) {
        if let Some(file) = self.files.get_mut(&id) {
            if file.terminal {
                return;
            }
            self.processed += file.size.saturating_sub(file.position);
            file.position = file.size;
        }
        self.complete(id, "Skipped");
    }
    fn complete(
        &mut self,
        id: u64,
        label: &str,
    ) {
        if let Some(file) = self.files.get_mut(&id) {
            if file.terminal {
                return;
            }
            file.terminal = true;
            self.finished += 1;
            let text = if label == "Succeeded" {
                format!(
                    "  \x1b[1;32m{:<width$}\x1b[0m [\x1b[36m{}\x1b[0m] {} • in {:#}",
                    truncate(&file.name, 48),
                    "#".repeat(50),
                    HumanBytes(file.size),
                    HumanDuration(file.started.map(|time| time.elapsed()).unwrap_or_default()),
                    width = self.name_width,
                )
            } else {
                format!("{label}: {} ({})", file.name, HumanBytes(file.size))
            };
            if let Some(bar) = self.bars.remove(&id) {
                // Detach without drawing an empty frame. println below renders
                // the completed line and remaining bars together.
                bar.disable_steady_tick();
                self.multi.remove(&bar);
                bar.finish_and_clear();
            }
            self.println(&text);
        }
        self.update_total();
        if self.finished == self.num_files {
            if let Some(total) = self.total.take() {
                let summary = format!(
                    "  \x1b[1;32m{:<width$}\x1b[0m [\x1b[36m{}\x1b[0m] {} • in {:#} • {}/{}",
                    "Total",
                    "#".repeat(50),
                    HumanBytes(self.transferred),
                    HumanDuration(total.elapsed()),
                    self.finished,
                    self.num_files,
                    width = self.name_width,
                );
                self.multi.remove(&total);
                total.finish_and_clear();
                self.println(&format!("  ------------------------\n{summary}"));
            }
        }
    }
    pub fn finish_with_message(
        &mut self,
        id: u64,
        msg: impl Into<Cow<'static, str>>,
    ) {
        self.complete(id, &msg.into());
    }
    pub fn transfer_mode_label(relay: RelayType) -> &'static str {
        match relay {
            RelayType::Local => "LAN",
            _ => "Relay",
        }
    }
    pub fn println(
        &self,
        msg: &str,
    ) {
        if self.multi.is_hidden() {
            // Keep messages in redirected output, where indicatif hides bars.
            println!("{msg}");
        } else {
            let _ = self.multi.println(msg);
        }
    }
}
fn truncate(
    value: &str,
    limit: usize,
) -> String {
    if value.chars().count() <= limit {
        value.to_owned()
    } else {
        format!("{}…", value.chars().take(limit - 1).collect::<String>())
    }
}
