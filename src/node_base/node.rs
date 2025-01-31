use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use crate::node_base::cmh_funcs::CmhMessageType;
use crate::node_base::resources::ResourceMessageType;
use crate::rpc_base::rpc_client_manager::RpcClientManager;
use crate::rpc_base::server;

#[derive(Debug, Clone)]
pub struct ResourceState {
    pub current_user: Option<SocketAddr>,
    pub request_queue: Vec<SocketAddr>,
}

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

    // Used for resource management which is not used
    // And can be ignored
    pub owned_resources: RwLock<HashMap<String, ResourceState>>,
    pub waiting_for: RwLock<HashMap<String, SocketAddr>>,
    pub used_resources: RwLock<HashMap<String, SocketAddr>>,
    pub blocked_processes: RwLock<HashSet<SocketAddr>>,

    
    // Necessary for deadlock algorithm
    pub is_active: RwLock<bool>,
    pub waiting_messages_from: RwLock<HashSet<SocketAddr>>,
    pub permission_queue: RwLock<HashSet<SocketAddr>>,

    pub last_test: RwLock<HashMap<SocketAddr, u64>>,
    pub wait_status: RwLock<HashMap<SocketAddr, bool>>,
    pub parent_nodes: RwLock<HashMap<SocketAddr, SocketAddr>>,
    pub probe_count: RwLock<HashMap<SocketAddr, u32>>,
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
            
            owned_resources: RwLock::new(HashMap::new()),
            waiting_for: RwLock::new(HashMap::new()),
            blocked_processes: RwLock::new(HashSet::new()),
            used_resources: RwLock::new(HashMap::new()),

            is_active: RwLock::new(true),
            waiting_messages_from: RwLock::new(HashSet::new()),
            permission_queue: RwLock::new(HashSet::new()),

            last_test: RwLock::new(HashMap::new()),
            wait_status: RwLock::new(HashMap::new()),
            parent_nodes: RwLock::new(HashMap::new()),
            probe_count: RwLock::new(HashMap::new()),
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
        *delay_lock = Duration::from_millis(delay_ms);
    }
    
    pub fn print_status(&self) {
        tracing::info!("Node id: {}, addr: {}", self.id.bold().green(), self.addr.to_string().bold().green());
        // now print the neighbor info
        let neighbor_info = self.neighbor_info.read().unwrap();
        tracing::info!("Next: {}, NNext: {}, Prev: {}", neighbor_info.next.to_string().green(), neighbor_info.nnext.to_string().green(), neighbor_info.prev.to_string().green());
        // show resources
        let owned = self.owned_resources.read().unwrap();
        tracing::info!("Owned resources: {:?}", owned.keys());
        // show used resources
        let used = self.used_resources.read().unwrap();
        tracing::info!("Used resources: {:?}", used.keys());
        // show queue for each resource
        for (resource, state) in owned.iter() {
            tracing::info!("Resource: {}, Current user: {:?}, Queue: {:?}", resource, state.current_user, state.request_queue);
        }
        // show resource requests
        let requests = self.waiting_for.read().unwrap();
        tracing::info!("Resource requests: {:?}", requests);
        
        // show deadlock stuff
        tracing::info!("Active: {}, Waiting messages from: {:?}", *self.is_active.read().unwrap(), *self.waiting_messages_from.read().unwrap());
        tracing::info!("Permission queue: {:?}", *self.permission_queue.read().unwrap());
    }

    pub async fn try_join_other(&self, other_addr: SocketAddr) {
        if self.addr == other_addr {
            tracing::info!("Node {} cannot join itself", self.id.bold().red());
            return;
        }
        let (n_client, ctx) = self.rpc.get_c(other_addr).await;
        let neighbor_new_info = match n_client.unwrap().other_joining(ctx, self.addr).await {
            Ok(n_info) => {n_info}
            Err(e) => {tracing::error!("Error during joining node {}: {}", other_addr.to_string().bold().red(), e); return;}
        };
        tracing::debug!("Node {} received neighbor info: {:?} from {}", self.id.bold().green(), neighbor_new_info, other_addr.to_string().bold().green());
        *self.neighbor_info.write().unwrap() = neighbor_new_info;
    }

    pub async fn send_resource_msg(&self, msg: ResourceMessageType, from: SocketAddr) -> ResourceMessageType {
        let next = {
            let neighbor_info = self.neighbor_info.read().unwrap();
            neighbor_info.next
        };
        tracing::debug!("Node {} sending message to {} with delay {}ms", self.id.bold().green(), next.to_string().bold().green(), self.message_delay.read().unwrap().as_millis());
        let delay = *self.message_delay.read().unwrap();
        self.increment_lamport();
        tokio::time::sleep(delay).await;

        let (client_result, ctx) = self.rpc.get_c(next).await;
        if let Ok(client) = client_result {
            self.increment_lamport();
            match client.handle_resource_msg(ctx, msg, from).await {
                Ok(response) => {
                    // tracing::info!("Node {} sent message to {}", self.id.bold().green(), next.to_string().bold().green());
                    response
                }
                Err(e) => {
                    tracing::error!("Error sending message to {}: {}", next.to_string().bold().red(), e);
                    ResourceMessageType::Error
                }
            }
        } else if let Err(e) = client_result {
            tracing::error!("Error getting client for {}", next.to_string().bold().red());
            self.repair_topology(next).await;
            ResourceMessageType::Error
        }
        else {
            ResourceMessageType::Error
        }
    }

    pub async fn send_cmh_msg(&self, msg: CmhMessageType, from: SocketAddr) -> CmhMessageType {
        let next = {
            let neighbor_info = self.neighbor_info.read().unwrap();
            neighbor_info.next
        };
        // tracing::debug!("Node {} sending message to {} with delay {}ms", self.id.bold().green(), next.to_string().bold().green(), self.message_delay.read().unwrap().as_millis());
        let delay = *self.message_delay.read().unwrap();
        self.increment_lamport();
        tokio::time::sleep(delay).await;

        let (client_result, ctx) = self.rpc.get_c(next).await;
        if let Ok(client) = client_result {
            self.increment_lamport();
            match client.handle_cmh_msg(ctx, msg, from).await {
                Ok(response) => {
                    // tracing::info!("Node {} sent message to {}", self.id.bold().green(), next.to_string().bold().green());
                    response
                }
                Err(e) => {
                    tracing::error!("Error sending message to {}: {}", next.to_string().bold().red(), e);
                    CmhMessageType::Error
                }
            }
        } else if let Err(e) = client_result {
            tracing::error!("Error getting client for {}", next.to_string().bold().red());
            self.repair_topology(next).await;
            CmhMessageType::Error
        }
        else {
            CmhMessageType::Error
        }
    }

    async fn repair_topology(&self, missing_node: SocketAddr) {
        tracing::info!("Node {} repairing topology with missing node: {}", self.id.bold().green(), missing_node.to_string().bold().red());
        if *self.repairing.read().unwrap() {
            return;
        }
        let (client, ctx) = self.rpc.get_c(self.addr).await;
        match client.unwrap().missing_node(ctx, missing_node).await {
            Ok(_) => {
                tracing::info!("Node {} repaired topology with missing node: {}", self.id.bold().green(), missing_node.to_string().bold().red());
            }
            Err(e) => {
                tracing::error!("Error during repairing topology: {}", e);
            }
        }

        *self.repairing.write().unwrap() = false;
    }

    // leave - inform neighbors and update their connections
    pub async fn leave(&self) -> Result<(), Box<dyn Error>> {
        tracing::info!("Node {} leaving", self.id.bold().red());

        let (client, ctx) = self.rpc.get_c(self.addr).await;
        match client.unwrap().leave_topology(ctx).await {
            Ok(_) => {
                // clean own neighbor info
                let mut neighbor_info = self.neighbor_info.write().unwrap();
                neighbor_info.next = self.addr;
                neighbor_info.nnext = self.addr;
                neighbor_info.prev = self.addr;
                Ok(())
            }
            Err(e) => {tracing::error!("Error during leaving {}: {}", self.id.bold().red(), e); Err(e.into())}
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



