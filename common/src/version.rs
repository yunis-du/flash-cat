use std::fmt;

pub const APP_VERSION: &'static str = "2.0.0";
pub const CLI_VERSION: &'static str = "2.0.0";
pub const RELAY_VERSION: &'static str = "2.0.0";

pub struct VersionInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub commit_hash: Option<&'static str>,
    pub build_time: &'static str,
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "    Name:        {}
    Version:     {}
    Commit Hash: {}
    Build Time:  {}",
            self.name,
            self.version,
            self.commit_hash.unwrap_or("None"),
            self.build_time
        )
    }
}

/// Compare two versions.
pub fn compare_versions(ver1: &str, ver2: &str) -> std::cmp::Ordering {
    let v1: Vec<u32> = ver1.split('.').map(|s| s.parse().unwrap_or(0)).collect();
    let v2: Vec<u32> = ver2.split('.').map(|s| s.parse().unwrap_or(0)).collect();

    for (n1, n2) in v1.iter().zip(v2.iter()) {
        match n1.cmp(n2) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    v1.len().cmp(&v2.len())
}
