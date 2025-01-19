use std::net::SocketAddr;
use std::sync::Arc;
use colored::Colorize;
use futures::{future, StreamExt};
use tarpc::{context};
use tarpc::context::Context;
use tarpc::server::{BaseChannel, Channel};
use tarpc::tokio_serde::formats::Json;
use crate::node_base::node::{NeighborInfo, Node};
use super::service::NodeRpc;

#[derive(Clone)]
pub struct NodeRpcServer {
    node: Arc<Node>,
}

impl NodeRpcServer {
    pub fn new(node: Arc<Node>) -> Self {
        Self { node }
    }
}

impl NodeRpc for NodeRpcServer {
    
    async fn heartbeat(self, _context: Context) -> bool {
        tracing::debug!("Node {} received heartbeat", self.node.id.bold().green());
        true
    }
    async fn msg(self, _: context::Context, message: String) -> String {
        tracing::info!("Node {} received message: {}", self.node.id.bold().green(), message.bold().yellow());
        format!("Hello back from {}", self.node.id)
    }

    async fn other_joining(self, _context: Context, addr: SocketAddr) -> NeighborInfo {
        tracing::debug!("Node {} received other_joining from {}", self.node.id.bold().green(), addr.to_string().bold().green());
        // Copy the values we need from the lock, then drop the guard
        let (mut tmp_neighbor, my_next, my_prev) = {
            let neighbor_info = self.node.neighbor_info.read().unwrap();
            (NeighborInfo {
                next: neighbor_info.next,
                nnext: neighbor_info.nnext,
                prev: self.node.addr,
            }, neighbor_info.next, neighbor_info.prev)
        };

        // update next node to change prev to new
        self.node.rpc.get_client(my_next).await.unwrap().change_prev(_context, addr).await.unwrap();
        // update prev node to change next next to new
        self.node.rpc.get_client(my_prev).await.unwrap().change_nnext(_context, addr).await.unwrap();
        
        tmp_neighbor.nnext = self.node.neighbor_info.read().unwrap().nnext;
        
        // change my neighbors
        self.node.neighbor_info.write().unwrap().nnext = my_next;
        self.node.neighbor_info.write().unwrap().next = addr;
        
        self.node.print_status();
        tmp_neighbor
    }
    
    async fn leave_topology(self, _context: Context) -> bool {
        tracing::debug!("Node {} leaving the topology", self.node.id.bold().green());
        let (prev, next, nnext) = {
            let neighbor_info= self.node.neighbor_info.read().unwrap();
            let prev = neighbor_info.prev;
            let next = neighbor_info.next;
            let nnext = neighbor_info.nnext;
            (prev, next, nnext)
        };

        if prev == next {
            // Only one other node exists
            // Update the remaining node to point to itself
            let remaining_client = self.node.rpc.get_client(next).await.unwrap();
            remaining_client.change_next(_context, next).await.unwrap();
            remaining_client.change_prev(_context, next).await.unwrap();
            remaining_client.change_nnext(_context, next).await.unwrap();
        } else if prev == nnext {
            // 3-node topology
            // Update the next node to point to itself
            let next_client = self.node.rpc.get_client(next).await.unwrap();
            next_client.change_prev(_context, prev).await.unwrap();
            next_client.change_nnext(_context, next).await.unwrap();
            let prev_client = self.node.rpc.get_client(prev).await.unwrap();
            prev_client.change_next(_context, next).await.unwrap();
            prev_client.change_nnext(_context, nnext).await.unwrap();
        } else {
            // normal case with 3+ nodes
            // update previous node's next pointer
            let prev_client = self.node.rpc.get_client(prev).await.unwrap();
            prev_client.change_next(_context, next).await.unwrap();

            // update next node's prev pointer
            let next_client = self.node.rpc.get_client(next).await.unwrap();
            next_client.change_prev(_context, prev).await.unwrap();

            // update previous node's nnext pointer
            // only update nnext if it's not pointing to the leaving node
            if nnext != self.node.addr {
                prev_client.change_nnext(_context, nnext).await.unwrap();
            } else {
                // if nnext is the leaving node, set it to next instead
                prev_client.change_nnext(_context, next).await.unwrap();
            }
        }
        
        true
    }
    async fn change_next(self, _context: Context, next: SocketAddr) -> bool {
        tracing::debug!("Node {} changing Next to {}", self.node.id.bold().green(), next.to_string().bold().yellow());
        match self.node.neighbor_info.write() {
            Ok(mut neighbor_info) => {
                neighbor_info.next = next;
                true
            },
            Err(e) => {
                tracing::error!("Error changing Next: {} in Node {}", e, self.node.id.bold().green());
                false
            }
        }
    }

    async fn change_nnext(self, _context: Context, nnext: SocketAddr) -> bool {
        tracing::debug!("Node {} changing NNext to {}", self.node.id.bold().green(), nnext.to_string().bold().yellow());
        match self.node.neighbor_info.write() {
            Ok(mut neighbor_info) => {
                neighbor_info.nnext = nnext;
                true
            },
            Err(e) => {
                tracing::error!("Error changing NNext: {} in Node {}", e, self.node.id.bold().green());
                false
            }
        }
    }

    async fn change_prev(self, _context: Context, prev: SocketAddr) -> bool {
        tracing::debug!("Node {} changing Prev to {}", self.node.id.bold().green(), prev.to_string().bold().yellow());
        match self.node.neighbor_info.write() {
            Ok(mut neighbor_info) => {
                neighbor_info.prev = prev;
                true
            },
            Err(e) => {
                tracing::error!("Error changing Prev: {} in Node {}", e, self.node.id.bold().green());
                false
            }
        }
    }

    async fn missing_node(self, context: Context, missing_node: SocketAddr) -> bool {
        tracing::debug!("Node {} fixing topology with missing node: {}", self.node.id.bold().green(), missing_node.to_string().bold().red());
        if !self.node.repairing.read().unwrap().clone() {
            tracing::info!("Node {} is already repairing", self.node.id.bold().red());
            return true;
        } else {
            // self.node.repairing = true;
            true

            // match self.rpc.missing_node(missing_node_addr).await {
            //     Ok(_) => {
            //         tracing::info!("Topology was repaired");
            //         self.repairing = false;
            //         Ok(())
            //     }
            //     Err(e) => {
            //         tracing::error!("Error repairing topology: {}", e);
            //         self.repairing = false;
            //         Err(e)
            //     }
            // }
        }
    }
}

pub async fn spawn(fut: impl std::future::Future<Output = ()> + Send + 'static) {
    tokio::spawn(fut);
}

pub async fn serve_rpc(node: Arc<Node>) -> Result<(), Box<dyn std::error::Error>> {
    let server = NodeRpcServer::new(node.clone());
    let listener = tarpc::serde_transport::tcp::listen(&node.addr, Json::default).await?;

    let addr = listener.local_addr().ip();
    let port = listener.local_addr().port();
    tracing::info!("RPC server running on {}:{}", addr, port);

    listener
        .filter_map(|r| future::ready(r.ok()))
        .map(BaseChannel::with_defaults)
        // Limit channels to 1 per IP
        // .max_channels_per_key(1, |t| t.transport().peer_addr().unwrap().ip())
        .map(|channel| {
            channel.execute(server.clone().serve())
                .for_each(spawn)
        })
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    Ok(())
}