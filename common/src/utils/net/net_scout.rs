use std::{collections::HashSet, net::Ipv4Addr, net::SocketAddr, time::Duration};

use anyhow::{Result, anyhow};
use bytes::{BufMut, BytesMut};
use if_addrs::{IfAddr, get_if_addrs};
use tokio::{
    net::UdpSocket,
    select,
    task::JoinSet,
    time::{self, MissedTickBehavior},
};

use crate::Shutdown;

/// Broadcast port.
const BROADCAST_PORT: u16 = 30086;
/// Interval for broadcast message.
const BROADCAST_INTERVAL: Duration = Duration::from_secs(1);

/// Broadcast Discovery
pub struct NetScout {
    match_content: Vec<u8>,
    timeout: Option<Duration>,
    shutdown: Shutdown,
}

impl NetScout {
    pub fn new(
        match_content: Vec<u8>,
        timeout: Option<Duration>,
        shutdown: Shutdown,
    ) -> Self {
        Self {
            match_content,
            timeout,
            shutdown,
        }
    }

    pub async fn broadcast(
        &mut self,
        port: u16,
    ) -> Result<()> {
        self.start_timeout();

        self.match_content.put_u16(port);
        let targets = broadcast_targets()?;
        if targets.is_empty() {
            return Err(anyhow!("no broadcast-capable IPv4 interface found"));
        }

        let mut workers = JoinSet::new();
        for (local_ip, broadcast_ip) in targets {
            let match_content = self.match_content.clone();
            let shutdown = self.shutdown.clone();
            workers.spawn(async move { broadcast_on_interface(local_ip, broadcast_ip, match_content, shutdown).await });
        }

        let mut last_error = None;
        while let Some(result) = workers.join_next().await {
            match result {
                Ok(Ok(true)) => {
                    self.shutdown.shutdown();
                    workers.abort_all();
                    return Ok(());
                }
                Ok(Ok(false)) => {
                    workers.abort_all();
                    return Ok(());
                }
                Ok(Err(error)) => last_error = Some(error),
                Err(error) => last_error = Some(error.into()),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("LAN broadcast workers stopped unexpectedly")))
    }

    pub async fn discovery(&self) -> Result<Option<SocketAddr>> {
        let socket = UdpSocket::bind(format!("{}:{}", "0.0.0.0", BROADCAST_PORT)).await?;
        socket.set_broadcast(true)?;

        self.start_timeout();

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
                    let match_buf: &[u8] = &buf[..match_content_len];
                    if match_content == match_buf {
                        port_buf[..].copy_from_slice(&buf[match_content_len..recv_len]);
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

    fn start_timeout(&self) {
        if let Some(timeout) = self.timeout {
            let shutdown = self.shutdown.clone();
            tokio::spawn(async move {
                tokio::time::sleep(timeout).await;
                shutdown.shutdown();
            });
        }
    }
}

async fn broadcast_on_interface(
    local_ip: Ipv4Addr,
    broadcast_ip: Ipv4Addr,
    match_content: Vec<u8>,
    shutdown: Shutdown,
) -> Result<bool> {
    let socket = UdpSocket::bind((local_ip, 0)).await?;
    socket.set_broadcast(true)?;
    let broadcast_addr = SocketAddr::from((broadcast_ip, BROADCAST_PORT));
    let mut broadcast_interval = time::interval(BROADCAST_INTERVAL);
    broadcast_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut buf = [0; 2];

    loop {
        select! {
            _ = broadcast_interval.tick() => {
                socket.send_to(&match_content, broadcast_addr).await?;
            }
            Ok(recv_len) = socket.recv(&mut buf) => {
                if &buf[..recv_len] == b"ok" {
                    return Ok(true);
                }
            }
            _ = shutdown.wait() => {
                return Ok(false);
            }
        }
    }
}

fn broadcast_targets() -> Result<Vec<(Ipv4Addr, Ipv4Addr)>> {
    let mut targets = HashSet::new();
    for interface in get_if_addrs()? {
        if !interface.is_oper_up() || interface.is_loopback() || interface.is_link_local() || interface.is_p2p() {
            continue;
        }
        if let IfAddr::V4(address) = interface.addr
            && let Some(broadcast) = eligible_broadcast(address.ip, address.broadcast)
        {
            targets.insert((address.ip, broadcast));
        }
    }
    Ok(targets.into_iter().collect())
}

fn eligible_broadcast(
    local_ip: Ipv4Addr,
    broadcast_ip: Option<Ipv4Addr>,
) -> Option<Ipv4Addr> {
    let broadcast_ip = broadcast_ip?;
    if local_ip.is_loopback() || local_ip.is_link_local() || local_ip.is_unspecified() || local_ip == broadcast_ip {
        return None;
    }
    Some(broadcast_ip)
}

#[cfg(test)]
mod test {
    use std::net::Ipv4Addr;

    use super::eligible_broadcast;

    #[test]
    fn accepts_broadcast_capable_lan_address() {
        assert_eq!(
            eligible_broadcast(Ipv4Addr::new(192, 168, 1, 10), Some(Ipv4Addr::new(192, 168, 1, 255))),
            Some(Ipv4Addr::new(192, 168, 1, 255))
        );
    }

    #[test]
    fn rejects_non_broadcast_addresses() {
        assert_eq!(
            eligible_broadcast(Ipv4Addr::LOCALHOST, Some(Ipv4Addr::new(127, 255, 255, 255))),
            None
        );
        assert_eq!(
            eligible_broadcast(Ipv4Addr::new(169, 254, 1, 2), Some(Ipv4Addr::new(169, 254, 255, 255))),
            None
        );
        assert_eq!(eligible_broadcast(Ipv4Addr::new(198, 18, 0, 1), None), None);
        assert_eq!(
            eligible_broadcast(Ipv4Addr::new(10, 0, 0, 1), Some(Ipv4Addr::new(10, 0, 0, 1))),
            None
        );
    }
}
