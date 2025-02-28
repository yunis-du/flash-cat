use std::{net::SocketAddr, time::Duration};

use anyhow::Result;
use bytes::{BufMut, BytesMut};
use tokio::{
    net::UdpSocket,
    select,
    time::{self, MissedTickBehavior},
};

use crate::Shutdown;

/// Broadcast address.
const BROADCAST_ADDR: &'static str = "255.255.255.255";
/// Broadcast port.
const BROADCAST_PORT: u16 = 30086;
/// Interval for broadcast message.
const BROADCAST_INTERVAL: Duration = Duration::from_secs(1);

/// Broadcast Discovery
pub struct NetScout {
    match_content: Vec<u8>,
    timeout: Duration,
    shutdown: Shutdown,
}

impl NetScout {
    pub fn new(match_content: Vec<u8>, timeout: Duration, shutdown: Shutdown) -> Self {
        Self {
            match_content,
            timeout,
            shutdown,
        }
    }

    pub async fn broadcast(&mut self, port: u16) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.set_broadcast(true)?;
        let broadcast_addr: SocketAddr =
            format!("{}:{}", BROADCAST_ADDR, BROADCAST_PORT).parse()?;

        let mut broadcast_interval = time::interval(BROADCAST_INTERVAL);
        broadcast_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let shutdown = self.shutdown.clone();
        let timeout = self.timeout.clone();
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            shutdown.shutdown();
        });
        self.match_content.put_u16(port);
        let match_content: &[u8] = &self.match_content;
        let mut buf = [0; 2];
        loop {
            select! {
                 // Send broadcast messages.
                 _ = broadcast_interval.tick() => {
                    socket.send_to(match_content, broadcast_addr).await?;
                }
                Ok(recv_len) = socket.recv(&mut buf) => {
                    let recv_buf = buf[..recv_len].as_ref();
                    if b"ok".eq(recv_buf) {
                        return Ok(());
                    }
                }
                // Exit.
                _ = self.terminated() => {
                    return Ok(());
                }
            }
        }
    }

    pub async fn discovery(&self) -> Result<Option<SocketAddr>> {
        let socket = UdpSocket::bind(format!("{}:{}", "0.0.0.0", BROADCAST_PORT)).await?;
        socket.set_broadcast(true)?;

        let shutdown = self.shutdown.clone();
        let timeout = self.timeout.clone();
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            shutdown.shutdown();
        });

        let match_content: &[u8] = &self.match_content;
        let match_content_len = match_content.len();
        let mut buf = BytesMut::with_capacity(1024);
        buf.resize(1024, 0);
        let mut port_buf = [0u8; 2];
        loop {
            select! {
                 // Send broadcast messages.
                 Ok((recv_len, mut remote_addr)) = socket.recv_from(&mut buf) => {
                    if recv_len == 0 {
                        continue;
                    }
                    let match_buf = buf[..match_content_len].as_ref();
                    if match_content.eq(match_buf) {
                        port_buf[..].copy_from_slice(buf[match_content_len..recv_len].as_ref());
                        let _ = socket.send_to(b"ok", remote_addr).await;
                        remote_addr.set_port(u16::from_be_bytes(port_buf));
                        return Ok(Some(remote_addr));
                    }
                    buf.clear();
                }
                // Exit.
                _ = self.terminated() => {
                    return Ok(None);
                }
            }
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.shutdown()
    }

    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::Shutdown;

    use super::NetScout;

    #[tokio::test]
    async fn broadcast() {
        let content = b"broadcast_test0123456789abcdefg123";
        let mut broadcast =
            NetScout::new(content.to_vec(), Duration::from_secs(60), Shutdown::new());
        if let Err(err) = broadcast.broadcast(2018).await {
            println!("broadcast failed, {err}");
        }
    }

    #[tokio::test]
    async fn discovery() {
        let content = b"broadcast_test0123456789abcdefg123";
        let discovery = NetScout::new(content.to_vec(), Duration::from_secs(3), Shutdown::new());
        match discovery.discovery().await {
            Ok(remote_addr) => match remote_addr {
                Some(remote_addr) => {
                    println!("discovery success, {remote_addr}");
                }
                None => {
                    println!("discovery -> not found, timeout.");
                }
            },
            Err(err) => {
                println!("discovery failed, {err}");
            }
        }
    }
}
