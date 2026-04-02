use std::{borrow::Cow, collections::HashMap, fmt::Write};

use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressState, ProgressStyle};

use flash_cat_common::format::HumanDuration;

pub struct Progress {
    num_files: u64,
    max_file_name_len: usize,
    total_size: u64,
    multi: MultiProgress,
    total_bar: Option<ProgressBar>,
    file_info: HashMap<u64, (String, u64)>,
    file_positions: HashMap<u64, u64>,
    progress_bar_map: HashMap<u64, ProgressBar>,
    finished_count: u64,
}

impl Progress {
    pub fn new(
        num_files: u64,
        max_file_name_len: usize,
        total_size: u64,
    ) -> Progress {
        Progress {
            num_files,
            max_file_name_len,
            total_size,
            multi: MultiProgress::new(),
            total_bar: None,
            file_info: HashMap::new(),
            file_positions: HashMap::new(),
            progress_bar_map: HashMap::new(),
            finished_count: 0,
        }
    }

    pub fn update(
        &mut self,
        num_files: u64,
        max_file_name_len: usize,
        total_size: u64,
    ) {
        self.num_files = num_files;
        self.max_file_name_len = max_file_name_len;
        self.total_size = total_size;
    }

    /// Register file metadata for lazy progress bar creation.
    pub fn register_file(
        &mut self,
        file_name: &str,
        file_id: u64,
        total_size: u64,
    ) {
        self.file_info.insert(file_id, (file_name.to_string(), total_size));
    }

    /// Create and immediately show a progress bar (used by receiver on-demand).
    pub fn add_progress(
        &mut self,
        file_name: &str,
        file_id: u64,
        total_size: u64,
    ) {
        self.file_info.insert(file_id, (file_name.to_string(), total_size));
        self.ensure_bar(file_id);
    }

    fn ensure_total_bar(&mut self) {
        if self.total_bar.is_some() || self.total_size == 0 || self.num_files <= 1 {
            return;
        }
        let prefix = format!("{:<width$}", "Total", width = self.max_file_name_len);
        let total_pb = ProgressBar::new(self.total_size).with_prefix(prefix).with_message(format!("0/{}", self.num_files));
        total_pb.set_style(
            ProgressStyle::with_template(
                "  ------------------------\n  {prefix:.bold.green} [{bar:50.cyan/blue}] {bytes}/{total_bytes} • {bytes_per_sec} • {msg}",
            )
            .unwrap()
            .progress_chars("#>-"),
        );
        let total_pb = self.multi.add(total_pb);
        self.total_bar = Some(total_pb);
    }

    fn ensure_bar(
        &mut self,
        file_id: u64,
    ) {
        if self.progress_bar_map.contains_key(&file_id) {
            return;
        }
        self.ensure_total_bar();
        let bar_info = self.file_info.get(&file_id).cloned();
        if let Some((name, total_size)) = bar_info {
            let file_name = format!("{:<width$}", name, width = self.max_file_name_len);
            let pb = ProgressBar::new(total_size).with_prefix(file_name);
            pb.set_style(
                ProgressStyle::with_template("{spinner:.green} {prefix:.bold.green} [{bar:50.cyan/blue}] {bytes}/{total_bytes} • {bytes_per_sec} • ETA {eta}")
                    .unwrap()
                    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
                        write!(w, "{:#}", HumanDuration(state.eta())).unwrap()
                    })
                    .progress_chars("#>-"),
            );
            let pb = if let Some(total_bar) = &self.total_bar {
                self.multi.insert_before(total_bar, pb)
            } else {
                self.multi.add(pb)
            };
            self.progress_bar_map.insert(file_id, pb);
        }
    }

    fn update_total(
        &mut self,
        file_id: u64,
        new_pos: u64,
    ) {
        let old_pos = self.file_positions.insert(file_id, new_pos).unwrap_or(0);
        if let Some(total_bar) = &self.total_bar {
            let current = total_bar.position();
            total_bar.set_position(current + new_pos.saturating_sub(old_pos));
        }
    }

    fn finish_total_if_done(&mut self) {
        if self.finished_count != self.num_files {
            return;
        }
        if let Some(total_bar) = self.total_bar.take() {
            let summary = format!(
                "  \x1b[1;32m{}\x1b[0m [\x1b[36m{}\x1b[0m] {} • in {:#} • {}/{}",
                format!("{:<width$}", "Total", width = self.max_file_name_len),
                "#".repeat(50),
                HumanBytes(self.total_size),
                HumanDuration(total_bar.elapsed()),
                self.finished_count,
                self.num_files,
            );
            total_bar.finish_and_clear();
            let _ = self.multi.println("  ------------------------");
            let _ = self.multi.println(summary);
        }
    }

    pub fn set_position(
        &mut self,
        file_id: u64,
        pos: u64,
    ) {
        self.ensure_bar(file_id);
        if let Some(progress_bar) = self.progress_bar_map.get(&file_id) {
            if progress_bar.position() == 0 {
                progress_bar.reset();
            }
            progress_bar.set_position(pos);
        }
        self.update_total(file_id, pos);
    }

    pub fn finish(
        &mut self,
        file_id: u64,
    ) {
        self.ensure_bar(file_id);
        if let Some(progress_bar) = self.progress_bar_map.remove(&file_id) {
            let summary = format!(
                "  \x1b[1;32m{}\x1b[0m [\x1b[36m{}\x1b[0m] {} • in {:#}",
                progress_bar.prefix(),
                "#".repeat(50),
                HumanBytes(progress_bar.length().unwrap_or(0)),
                HumanDuration(progress_bar.elapsed()),
            );
            progress_bar.finish_and_clear();
            let _ = self.multi.println(summary);
        }
        let file_size = self.file_info.get(&file_id).map(|(_, s)| *s).unwrap_or(0);
        self.update_total(file_id, file_size);
        self.finished_count += 1;
        if let Some(total_bar) = &self.total_bar {
            total_bar.set_message(format!("{}/{}", self.finished_count, self.num_files));
        }
        self.finish_total_if_done();
    }

    pub fn skip(
        &mut self,
        file_id: u64,
    ) {
        if let Some((name, _)) = self.file_info.get(&file_id) {
            let _ = self.multi.println(format!("skip '{}'", name));
        }
        if let Some(progress_bar) = self.progress_bar_map.remove(&file_id) {
            progress_bar.finish_and_clear();
        }
        let file_size = self.file_info.get(&file_id).map(|(_, s)| *s).unwrap_or(0);
        self.update_total(file_id, file_size);
        self.finished_count += 1;
        if let Some(total_bar) = &self.total_bar {
            total_bar.set_message(format!("{}/{}", self.finished_count, self.num_files));
        }
        self.finish_total_if_done();
    }

    pub fn finish_with_message(
        &mut self,
        file_id: u64,
        msg: impl Into<Cow<'static, str>>,
    ) {
        self.ensure_bar(file_id);
        if let Some(progress_bar) = self.progress_bar_map.get(&file_id) {
            progress_bar.finish_with_message(msg);
        }
    }

    pub fn println(
        &self,
        msg: &str,
    ) {
        let _ = self.multi.println(msg);
    }
}
