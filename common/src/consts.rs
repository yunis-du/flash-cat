use std::time::Duration;

/// Default port for relay.
pub const DEFAULT_RELAY_PORT: u16 = 20018;

/// Domain for pubilc relay.
pub const PUBLIC_RELAY: &'static str = "flashcat.yunisdu.com";

/// The default cryptographic private key | 256-bit (32 bytes) key.
pub const DEFAULT_SECRET_KEY: &'static str = "FLASH-CAT.UYfvV4jQOW.OtUuM38b0iD";

/// The default http2 keepalive interval.
pub const DEFAULT_HTTP2_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

/// The default http2 keepalive timeout.
pub const DEFAULT_HTTP2_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(20);

/// The default tcp keepalive.
pub const DEFAULT_TCP_KEEPALIVE: Duration = Duration::from_secs(30);

/// Send buffer size: 32Kib
pub const SEND_BUFF_SIZE: usize = 32 * 1024;
