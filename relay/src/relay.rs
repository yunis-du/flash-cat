use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use dashmap::DashMap;
use flash_cat_common::Shutdown;
use log::debug;
use tokio::time;

use crate::{listen, session::Session};

/// Session timeout.
const DISCONNECTED_SESSION_EXPIRY: Duration = Duration::from_secs(300);

/// Relay state.
pub struct RelayState {
    external_ip: Option<IpAddr>,
    store: DashMap<String, Arc<Session>>,
    local_relay: bool,
}

impl RelayState {
    /// Create a new relay state.
    pub fn new(
        external_ip: Option<IpAddr>,
        local_relay: bool,
    ) -> Result<Self> {
        Ok(Self {
            store: DashMap::new(),
            external_ip,
            local_relay,
        })
    }

    /// External IP address.
    pub fn external_ip(&self) -> Option<IpAddr> {
        self.external_ip
    }

    /// Lookup session by name.
    pub fn lookup(
        &self,
        name: &str,
    ) -> Option<Arc<Session>> {
        self.store.get(name).map(|s| s.clone())
    }

    /// Close old sessions.
    pub async fn close_old_sessions(&self) {
        loop {
            debug!("start check old sessions");
            time::sleep(DISCONNECTED_SESSION_EXPIRY / 5).await;
            let mut to_close = Vec::new();
            for entry in &self.store {
                let session = entry.value();
                if session.last_accessed().elapsed() > DISCONNECTED_SESSION_EXPIRY {
                    to_close.push(entry.key().clone());
                }
            }
            for name in to_close {
                self.close_session(&name);
                debug!("closeed old session {name}");
            }
        }
    }

    /// Close session and remove it from store.
    pub fn close_session(
        &self,
        name: &str,
    ) {
        if let Some((_, session)) = self.store.remove(name) {
            session.shutdown();
        }
    }

    /// Insert session into store.
    pub fn insert(
        &self,
        name: &str,
        session: Arc<Session>,
    ) {
        if let Some(prev_session) = self.store.insert(name.to_string(), session) {
            prev_session.shutdown();
        }
    }

    /// Whether is local relay.
    pub fn is_local_relay(&self) -> bool {
        self.local_relay
    }

    /// Shutdown all sessions.
    pub fn shutdown(&self) {
        for entry in &self.store {
            entry.value().shutdown();
        }
    }
}

/// Relay server.
pub struct Relay {
    state: Arc<RelayState>,

    shutdown: Shutdown,
}

impl Relay {
    /// Create a new relay server.
    pub fn new(
        external_ip: Option<IpAddr>,
        local_relay: bool,
    ) -> Result<Self> {
        Ok(Self {
            state: Arc::new(RelayState::new(external_ip, local_relay)?),
            shutdown: Shutdown::new(),
        })
    }

    /// Create a new relay server with shutdown.
    pub fn new_with_shutdown(
        external_ip: Option<IpAddr>,
        local_relay: bool,
        shutdown: Shutdown,
    ) -> Result<Self> {
        Ok(Self {
            state: Arc::new(RelayState::new(external_ip, local_relay)?),
            shutdown,
        })
    }

    /// Relay state.
    pub fn state(&self) -> Arc<RelayState> {
        Arc::clone(&self.state)
    }

    /// Run the application server, listening on a stream of connections.
    pub async fn listen(
        &self,
        addr: SocketAddr,
    ) -> Result<()> {
        let state = self.state.clone();
        let shutdown_signal = self.shutdown.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = shutdown_signal.wait() => {}
                _ = state.close_old_sessions() => {}
            }
        });
        listen::start_server(self.state(), addr, self.shutdown.wait()).await
    }

    /// Convenience function to call [`Server::listen`] bound to a TCP address.
    pub async fn bind(
        &self,
        addr: SocketAddr,
    ) -> Result<()> {
        self.listen(addr).await
    }

    /// Send a graceful shutdown signal to the server.
    pub fn shutdown(&self) {
        // Stop receiving new network connections.
        self.shutdown.shutdown();
        // Terminate each of the existing sessions.
        self.state.shutdown();
    }
}
