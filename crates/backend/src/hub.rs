//! WebSocket hub — shared state for daemon and frontend WebSocket connections.
//!
//! The hub owns:
//! - `daemon`: an optional active daemon connection (sender side of its WS)
//! - `frontend_clients`: a map of connected frontend clients with their senders
//!
//! The hub intentionally uses `tokio::sync::mpsc` channels to push messages out
//! without holding locks during I/O.

use std::collections::HashMap;
use tokio::sync::mpsc;

/// A unique client identifier for frontend connections.
pub type ClientId = u64;

/// A sender that delivers text messages to a single frontend client.
pub type ClientSender = mpsc::UnboundedSender<String>;

/// Sender for the daemon connection — used to push `RunDispatch` JSON to the daemon.
pub type DaemonSender = mpsc::UnboundedSender<String>;

/// Central WebSocket hub state.
#[derive(Debug, Default)]
pub struct Hub {
    /// The connected daemon's outbound message channel, if any.
    pub daemon: Option<DaemonSender>,
    /// All connected frontend clients indexed by a monotonic ID.
    pub frontend_clients: HashMap<ClientId, ClientSender>,
    /// Counter for assigning unique client IDs.
    next_client_id: u64,
}

impl Hub {
    pub fn new() -> Self {
        Hub { daemon: None, frontend_clients: HashMap::new(), next_client_id: 0 }
    }

    /// Register a new frontend client, returning its assigned ID.
    pub fn add_client(&mut self, sender: ClientSender) -> ClientId {
        let id = self.next_client_id;
        self.next_client_id += 1;
        self.frontend_clients.insert(id, sender);
        id
    }

    /// Remove a frontend client by ID.
    pub fn remove_client(&mut self, id: ClientId) {
        self.frontend_clients.remove(&id);
    }

    /// Broadcast a JSON text message to all connected frontend clients.
    /// Dead senders (client disconnected) are silently skipped.
    pub fn broadcast(&self, message: &str) {
        for sender in self.frontend_clients.values() {
            let _ = sender.send(message.to_owned());
        }
    }

    /// Send a message to the daemon, if connected.
    /// Returns true if sent, false if no daemon is connected.
    pub fn send_to_daemon(&self, message: &str) -> bool {
        match &self.daemon {
            Some(sender) => sender.send(message.to_owned()).is_ok(),
            None => false,
        }
    }

    /// Returns true if a daemon is currently connected.
    pub fn has_daemon(&self) -> bool {
        self.daemon.is_some()
    }
}
