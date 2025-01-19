use std::net::SocketAddr;
use std::sync::RwLock;
use std::time::Duration;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use tarpc::context;
use crate::rpc_base::rpc_client_manager::RpcClientManager;

// TODO: Add a unique generated id
#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub addr: SocketAddr,
    pub message_delay: RwLock<Duration>,
    pub neighbor_info: RwLock<NeighborInfo>,
    pub active: RwLock<bool>,
    pub repairing: RwLock<bool>,
    pub rpc: RpcClientManager,
}

// Circle topology with a leader
#[derive(Debug, Serialize, Deserialize)]
pub struct NeighborInfo {
    pub next: SocketAddr,
    pub nnext: SocketAddr,
    pub prev: SocketAddr,
}

impl Node {
    pub fn new(id: String, addr: SocketAddr) -> Self {
        tracing::debug!("Creating new node with id: {}, addr: {}", id, addr);
        Self {
            id,
            addr,
            message_delay: RwLock::new(Duration::from_millis(0)),
            neighbor_info: RwLock::new(NeighborInfo {
                next: addr,
                nnext: addr,
                prev: addr,
            }),
            active: RwLock::new(true),
            repairing: RwLock::new(false),
            rpc: RpcClientManager::new(addr),
        }
    }

    pub fn set_delay(&self, delay_ms: u64) {
        tracing::debug!("Setting delay to {}ms for node {}", delay_ms, self.id);
        let mut delay_lock = self.message_delay.write().unwrap();
        *delay_lock = Duration::from_millis(0);
    }
    
    pub fn print_status(&self) {
        tracing::info!("Node id: {}, addr: {}", self.id.bold().green(), self.addr.to_string().bold().green());
        // now print the neighbor info
        let neighbor_info = self.neighbor_info.read().unwrap();
        tracing::info!("Next: {}, NNext: {}, Prev: {}", neighbor_info.next.to_string().green(), neighbor_info.nnext.to_string().green(), neighbor_info.prev.to_string().green());
    }

    pub async fn try_join_other(&self, other_addr: SocketAddr) {
        if self.addr == other_addr {
            tracing::info!("Node {} cannot join itself", self.id.bold().red());
            return;
        }
        let neighbor_new_info = match self.rpc.join_other(other_addr).await {
            Ok(n_info) => {n_info}
            Err(e) => {tracing::error!("Error during joining node {}: {}", other_addr.to_string().bold().red(), e); return;}
        };
        tracing::debug!("Node {} received neighbor info: {:?} from {}", self.id.bold().green(), neighbor_new_info, other_addr.to_string().bold().green());
        *self.neighbor_info.write().unwrap() = neighbor_new_info;
    }

    pub async fn send_msg(&self, other_addr: SocketAddr, msg: String) -> Result<String, Box<dyn std::error::Error>> {
        tracing::debug!("Node {} sending message to {} with delay {}ms", self.id.bold().green(), other_addr.to_string().bold().green(), self.message_delay.read().unwrap().as_millis());
        let delay = *self.message_delay.read().unwrap();
        tokio::time::sleep(delay).await;
        match self.rpc.send_msg(other_addr, msg).await {
            Ok(response) => Ok(response),
            Err(e) => Err(e),
        }
    }

    // leave - inform neighbors and update their connections
    pub async fn leave(&self) -> Result<(), Box<dyn std::error::Error>> {
        match self.rpc.leave_topology().await {
            Ok(_) => {
                // clean own neighbor info
                let mut neighbor_info = self.neighbor_info.write().unwrap();
                neighbor_info.next = self.addr;
                neighbor_info.nnext = self.addr;
                neighbor_info.prev = self.addr;
                Ok(())
            }
            Err(e) => {tracing::error!("Error during leaving {}: {}", self.id.bold().red(), e); Err(e)}
        }
    }

    // kill - mark node as inactive/dead
    pub async fn kill(&self, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        if node_id == &self.id {
            // mark self as inactive
            *self.active.write().unwrap() = false;
            Ok(())
        } else {
            Err("Cannot kill other nodes directly".into())
        }
    }

    // revive - mark node as active and rejoin topology
    pub async fn revive(&self, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        if node_id == &self.id {
            // mark self as active
            *self.active.write().unwrap() = true;

            // try to rejoin using the last known neighbor
            let last_known_next = self.neighbor_info.read().unwrap().prev;
            if last_known_next != self.addr {
                self.try_join_other(last_known_next).await;
            }

            Ok(())
        } else {
            Err("Cannot revive other nodes directly".into())
        }
    }
}



