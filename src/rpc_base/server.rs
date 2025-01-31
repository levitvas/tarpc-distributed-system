use std::net::SocketAddr;
use std::sync::Arc;
use colored::Colorize;
use futures::{future, StreamExt};
use tarpc::{context};
use tarpc::context::Context;
use tarpc::server::{BaseChannel, Channel};
use tarpc::tokio_serde::formats::Json;
use crate::node_base::cmh_funcs::CmhMessageType;
use crate::node_base::node::{NeighborInfo, Node};
use crate::node_base::resources::ResourceMessageType;
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
    async fn handle_resource_msg(self, _: context::Context, message: ResourceMessageType, from: SocketAddr) -> ResourceMessageType {
        tracing::debug!("Node {} received Resource message", self.node.id.bold().green());
        self.node.handle_message(message, from).await.unwrap()
    }

    async fn handle_cmh_msg(self, _: context::Context, message: CmhMessageType, from: SocketAddr) -> CmhMessageType {
        tracing::debug!("Node {} received CMH message", self.node.id.bold().green());
        self.node.handle_cmh_message(message, from).await.unwrap()
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
            prev_client.change_nnext(_context, nnext).await.unwrap();
            prev_client.change_nnext_of_prev(_context, next).await.unwrap();

            // update next node's prev pointer
            let next_client = self.node.rpc.get_client(next).await.unwrap();
            next_client.change_prev(_context, prev).await.unwrap();
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

    async fn change_prev(self, _context: Context, prev: SocketAddr) -> SocketAddr {
        tracing::debug!("Node {} changing Prev to {}", self.node.id.bold().green(), prev.to_string().bold().yellow());
        match self.node.neighbor_info.write() {
            Ok(mut neighbor_info) => {
                neighbor_info.prev = prev;
                neighbor_info.next
            },
            Err(e) => {
                tracing::error!("Error changing Prev: {} in Node {}", e, self.node.id.bold().green());
                SocketAddr::new([0, 0, 0, 0].into(), 0)
            }
        }
    }

    async fn change_nnext_of_prev(self, _context: Context, next: SocketAddr) -> bool {
        tracing::debug!("Node {} changing nnext of Prev to {}", self.node.id.bold().green(), next.to_string().bold().yellow());
        let prev = {
            let neighbor_info = self.node.neighbor_info.read().unwrap();
            neighbor_info.prev
        };
        let prev_client = self.node.rpc.get_client(prev).await.unwrap();
        prev_client.change_nnext(_context, next).await.unwrap()
    }

    async fn missing_node(self, context: Context, missing_node: SocketAddr) -> bool {
        // TODO: Its not finished
        tracing::debug!("Node {} fixing topology with missing node: {}", self.node.id.bold().green(), missing_node.to_string().bold().red());
        let next = self.node.neighbor_info.read().unwrap().next;
        let nnext = self.node.neighbor_info.read().unwrap().nnext;
        let prev = self.node.neighbor_info.read().unwrap().prev;
        
        if missing_node == self.node.neighbor_info.read().unwrap().next {
            // its for me
            // to my nnext send msg ChPrev with myaddr -> my nnext = next
            self.node.neighbor_info.write().unwrap().next = nnext;
            self.node.neighbor_info.write().unwrap().nnext = self.node.rpc.get_client(nnext).await.unwrap().change_prev(context, next).await.unwrap();
            // to my prev send msg ChNNext to my.next
            if prev == next {
                // only 2 nodes
                self.node.neighbor_info.write().unwrap().prev = nnext;
                true
            } else {
                // 3+ nodes
                self.node.rpc.get_client(prev).await.unwrap().change_nnext(context, nnext).await.unwrap()
            }
        } else {
            // send to next node
            self.node.rpc.get_client(next).await.unwrap().missing_node(context, missing_node).await.unwrap()
        }
    }
}

pub async fn spawn(fut: impl std::future::Future<Output = ()> + Send + 'static) {
    tracing::debug!("Spawning a new task");
    tokio::spawn(fut);
}

pub async fn serve_rpc(node: Arc<Node>) -> Result<(), Box<dyn std::error::Error>> {
    let server = NodeRpcServer::new(node.clone());
    let listener = tarpc::serde_transport::tcp::listen(&node.addr, Json::default).await?;

    let addr = listener.local_addr().ip();
    let port = listener.local_addr().port();
    tracing::info!("RPC server running on {}:{}", addr, port);
    let mut stop_signal = node.stop_signal.subscribe();

    tokio::select! {
        _ = async {
            listener
                .filter_map(|r| future::ready(r.ok()))
                .map(BaseChannel::with_defaults)
                .map(|channel| {
                    channel.execute(server.clone().serve())
                        .for_each(spawn)
                })
                .buffer_unordered(10)
                .for_each(|_| async {})
                .await;
        } => {},
        _ = stop_signal.changed() => {
            tracing::info!("RPC server is shutting down");
        }
    }

    Ok(())
}