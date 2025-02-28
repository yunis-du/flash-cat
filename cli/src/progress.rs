use std::{borrow::Cow, collections::HashMap, fmt::Write};

use indicatif::{ProgressBar, ProgressState, ProgressStyle};

pub struct Progress {
    num_files: u64,
    max_file_name_len: usize,
    progress_bar_map: HashMap<u64, ProgressBar>,
}

impl Progress {
    pub fn new(num_files: u64, max_file_name_len: usize) -> Progress {
        Progress {
            num_files,
            max_file_name_len,
            progress_bar_map: HashMap::new(),
        }
    }

    pub fn update(&mut self, num_files: u64, max_file_name_len: usize) {
        self.num_files = num_files;
        self.max_file_name_len = max_file_name_len;
    }

    pub fn add_progress(&mut self, file_name: &str, file_id: u64, total_size: u64) {
        let file_name = format!("{:<width$}", file_name, width = self.max_file_name_len);
        let serial_number = format!("{file_id}/{}", self.num_files);
        let progress_bar = ProgressBar::new(total_size)
            .with_prefix(file_name)
            .with_message(serial_number);
        progress_bar.set_style(ProgressStyle::with_template("{spinner:.green} {prefix:.bold.green} [{bar:50.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta}) {msg}")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));

        self.progress_bar_map.insert(file_id, progress_bar);
    }

    pub fn set_position(&self, file_id: u64, pos: u64) {
        if let Some(progress_bar) = self.progress_bar_map.get(&file_id) {
            progress_bar.set_position(pos);
        }
    }

    pub fn finish(&self, file_id: u64) {
        if let Some(progress_bar) = self.progress_bar_map.get(&file_id) {
            progress_bar.finish();
        }
    }

    pub fn skip(&self, file_id: u64) {
        if let Some(progress_bar) = self.progress_bar_map.get(&file_id) {
            progress_bar.finish_and_clear();
            println!(
                "skip '{}'     {}",
                progress_bar.prefix(),
                progress_bar.message()
            );
        }
    }

    pub fn finish_with_message(&self, file_id: u64, msg: impl Into<Cow<'static, str>>) {
        if let Some(progress_bar) = self.progress_bar_map.get(&file_id) {
            progress_bar.finish_with_message(msg);
        }
    }
}
