use std::time::Duration;

/// The name of the application.
pub const APP_NAME: &'static str = "flash_cat";

/// The name of the application config file.
pub const APP_CONFIG_FILE_NAME: &'static str = "app_config";

/// Default port for relay.
pub const DEFAULT_RELAY_PORT: u16 = 20018;

/// Domain for pubilc relay.
pub const PUBLIC_RELAY: &'static str = "flashcat.yunisdu.com";

/// The default cryptographic private key | 256-bit (32 bytes) key.
pub const DEFAULT_SECRET_KEY: &'static str = "FLASH-CAT.UYfvV4jQOW.OtUuM38b0iD";

/// The default http2 keepalive interval.
pub const DEFAULT_HTTP2_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

/// The default http2 keepalive timeout.
pub const DEFAULT_HTTP2_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(3);

/// Send buffer size: 32Kib
pub const SEND_BUFF_SIZE: usize = 32 * 1024;
