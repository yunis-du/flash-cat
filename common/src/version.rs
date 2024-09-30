use std::fmt;

pub const APP_VERSION: &'static str = "1.1.0";
pub const CLI_VERSION: &'static str = "1.1.0";
pub const RELAY_VERSION: &'static str = "1.1.0";

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
