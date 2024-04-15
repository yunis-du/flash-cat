use std::fmt;

pub const CLI_VERSION: &'static str = "0.1.1";
pub const RELAY_VERSION: &'static str = "0.1.1";

pub struct VersionInfo {
    pub name: &'static str,
    pub version: &'static str,
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "    Name: {}
    Version: {}",
            self.name, self.version
        )
    }
}
