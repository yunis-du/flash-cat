use std::path::Path;

use fern::colors::{Color, ColoredLevelConfig};
use log::LevelFilter;

pub fn init_logger<P: AsRef<Path>>(
    log_level: String,
    log_file: P,
) {
    let log_path = Path::new(log_file.as_ref());
    let log_parent_path = log_path.parent();
    if let Some(log_parent_path) = log_parent_path {
        if !Path::new(log_parent_path).exists() {
            let res = std::fs::create_dir_all(log_parent_path);
            if let Err(e) = res {
                eprintln!(
                    "create logs path [{}] failed. {}",
                    log_parent_path.to_string_lossy().to_string(),
                    e
                );
                std::process::exit(1);
            }
        }
    }

    let colors = ColoredLevelConfig::new().info(Color::Green).debug(Color::Cyan).warn(Color::Yellow).error(Color::Red);

    let log_level = match std::env::var("RUST_LOG").unwrap_or(log_level).to_lowercase().trim() {
        "off" => LevelFilter::Off,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "warn" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };

    fern::Dispatch::new()
        .format(move |out, message, record| {
            let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            out.finish(format_args!(
                "[{} {} {}] {}",
                time,
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .level(log_level)
        .chain(std::io::stdout())
        .chain(fern::log_file(log_file.as_ref()).unwrap())
        .chain(fern::DateBased::new(log_file.as_ref(), ".%Y-%m-%d"))
        .apply()
        .unwrap();
}
