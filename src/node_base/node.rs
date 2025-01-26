use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use tarpc::context;
use tokio::sync::watch;
use crate::rpc_base::rpc_client_manager::RpcClientManager;
use crate::rpc_base::server;

// TODO: Add a unique generated id
#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub addr: SocketAddr,
    pub message_delay: RwLock<Duration>,
    pub neighbor_info: RwLock<NeighborInfo>,
    pub lamport_time: RwLock<u64>,
    pub repairing: RwLock<bool>,
    pub rpc: RpcClientManager,
    pub stop_signal: watch::Sender<()>,
    // TODO: Add circle token for critical section
}

// Circle topology with a leader
#[derive(Debug, Serialize, Deserialize)]
pub struct NeighborInfo {
    pub next: SocketAddr,
    pub nnext: SocketAddr,
    pub prev: SocketAddr,
}

impl Node {
    pub fn new(id: String, addr: SocketAddr) -> Arc<Self> {
        tracing::debug!("Creating new node with id: {}, addr: {}", id, addr);
        let (stop_signal, _) = watch::channel(());
        Arc::new(Self {
            id,
            addr,
            message_delay: RwLock::new(Duration::from_millis(0)),
            neighbor_info: RwLock::new(NeighborInfo {
                next: addr,
                nnext: addr,
                prev: addr,
            }),
            lamport_time: RwLock::new(0),
            repairing: RwLock::new(false),
            rpc: RpcClientManager::new(addr),
            stop_signal,
        })
    }

    pub fn increment_lamport(&self) -> u64{
        *self.lamport_time.write().unwrap() += 1;
        *self.lamport_time.read().unwrap()
    }

    pub fn update_clock(&self, received_time: u64) {
        let mut clock = self.lamport_time.write().unwrap();
        *clock = std::cmp::max(*clock, received_time) + 1;
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

    pub async fn send_msg(&self, other_addr: SocketAddr, msg: String) -> Result<String, Box<dyn Error>> {
        tracing::debug!("Node {} sending message to {} with delay {}ms", self.id.bold().green(), other_addr.to_string().bold().green(), self.message_delay.read().unwrap().as_millis());
        let delay = *self.message_delay.read().unwrap();
        self.increment_lamport();
        tokio::time::sleep(delay).await;
        match self.rpc.send_msg(other_addr, msg).await {
            Ok(response) => Ok(response),
            Err(e) => Err(e),
        }
    }

    // leave - inform neighbors and update their connections
    pub async fn leave(&self) -> Result<(), Box<dyn Error>> {
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
    pub async fn kill(&self, node_addr: SocketAddr) -> Result<(), Box<dyn Error>> {
        if node_addr == self.addr {
            // send stop signal to the RPC server
            // TODO: KILL CLIENT AS WELL
            let _ = self.stop_signal.send(());
            Ok(())
        } else {
            Err("Cannot kill other nodes directly".into())
        }
    }

    // revive - mark node as active and rejoin topology
    pub async fn revive(self: &Arc<Self>, node_addr: SocketAddr) -> Result<(), Box<dyn Error>> {
        if node_addr == self.addr {
            // // restart the RPC server
            let rpc_node = self.clone();
            tokio::spawn(async move {
                if let Err(e) = server::serve_rpc(rpc_node).await {
                    tracing::error!("RPC server error: {}", e);
                }
            });

            // try to rejoin using the last known neighbor
            let last_known_prev = self.neighbor_info.read().unwrap().prev;
            if last_known_prev != self.addr {
                self.try_join_other(last_known_prev).await;
            }

            Ok(())
        } else {
            Err("Cannot revive other nodes directly".into())
        }
    }
}



