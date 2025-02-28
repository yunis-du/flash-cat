use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use dashmap::DashMap;
use flash_cat_common::Shutdown;
use log::{debug, error};
use tokio::time;

use crate::{listen, session::Session};

const DISCONNECTED_SESSION_EXPIRY: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Default)]
pub struct RelayOptions {}

pub struct RelayState {
    external_ip: Option<IpAddr>,
    store: DashMap<String, Arc<Session>>,
}

impl RelayState {
    pub fn new(external_ip: Option<IpAddr>) -> Result<Self> {
        Ok(Self {
            store: DashMap::new(),
            external_ip,
        })
    }

    pub fn external_ip(&self) -> Option<IpAddr> {
        self.external_ip
    }

    pub fn lookup(&self, name: &str) -> Option<Arc<Session>> {
        self.store.get(name).map(|s| s.clone())
    }

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
                if let Err(err) = self.close_session(&name).await {
                    error!("failed to close old session {name}, {err}");
                }
                debug!("closeed old session {name}");
            }
        }
    }

    pub fn remove(&self, name: &str) -> bool {
        if let Some((_, session)) = self.store.remove(name) {
            session.shutdown();
            true
        } else {
            false
        }
    }
    pub async fn close_session(&self, name: &str) -> Result<()> {
        self.remove(name);
        Ok(())
    }

    pub fn insert(&self, name: &str, session: Arc<Session>) {
        if let Some(prev_session) = self.store.insert(name.to_string(), session) {
            prev_session.shutdown();
        }
    }

    pub fn shutdown(&self) {
        for entry in &self.store {
            entry.value().shutdown();
        }
    }
}

pub struct Relay {
    state: Arc<RelayState>,
    shutdown: Shutdown,
}

impl Relay {
    pub fn new(external_ip: Option<IpAddr>) -> Result<Self> {
        Ok(Self {
            state: Arc::new(RelayState::new(external_ip)?),
            shutdown: Shutdown::new(),
        })
    }

    pub fn new_with_shutdown(external_ip: Option<IpAddr>, shutdown: Shutdown) -> Result<Self> {
        Ok(Self {
            state: Arc::new(RelayState::new(external_ip)?),
            shutdown,
        })
    }

    pub fn state(&self) -> Arc<RelayState> {
        Arc::clone(&self.state)
    }

    /// Run the application server, listening on a stream of connections.
    pub async fn listen(&self, addr: SocketAddr) -> Result<()> {
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
    pub async fn bind(&self, addr: SocketAddr) -> Result<()> {
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
