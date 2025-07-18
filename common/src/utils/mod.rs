use std::time::{Duration, SystemTime};

use indicatif::{HumanBytes, HumanDuration};
use rand::{Rng, SeedableRng, distr::Alphanumeric};

pub mod fs;
pub mod net;

pub fn convert_bytes_to_human_readable(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let mut unit_index = 0;
    let mut bytes = bytes as f64;

    while bytes >= 1024.0 && unit_index < units.len() - 1 {
        bytes /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2}{}", bytes, units[unit_index])
}

pub fn gen_share_code() -> String {
    let mut rng = rand::rngs::StdRng::from_os_rng();
    format!(
        "{}-{}-{}",
        rng.random_range(10..99),
        (0..4).map(|_| rng.sample(Alphanumeric) as char).collect::<String>(),
        (0..4).map(|_| rng.sample(Alphanumeric) as char).collect::<String>()
    )
}

pub fn get_time_ms() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("system time is before the UNIX epoch").as_millis() as u64
}

pub fn human_bytes(bytes: u64) -> String {
    HumanBytes(bytes).to_string()
}

pub fn human_duration(duration: Duration) -> String {
    format!("{:#}", HumanDuration(duration))
}

#[cfg(test)]
mod test {
    use crate::utils::gen_share_code;

    #[test]
    fn t1() {
        println!("gen_share_code: {}", gen_share_code());
        println!("gen_share_code: {}", gen_share_code());
        println!("gen_share_code: {}", gen_share_code());
        println!("gen_share_code: {}", gen_share_code());
        println!("gen_share_code: {}", gen_share_code());
        println!("gen_share_code: {}", gen_share_code());
    }
}
