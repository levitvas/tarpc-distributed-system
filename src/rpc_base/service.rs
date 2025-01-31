use std::net::SocketAddr;
use tarpc::context::Context;
use crate::node_base::node::NeighborInfo;
use crate::node_base::resources::{ResourceMessageType};
use crate::node_base::cmh_funcs::{CmhMessageType};

#[tarpc::service]
pub trait NodeRpc {
    async fn heartbeat() -> bool;
    async fn handle_resource_msg(message: ResourceMessageType, from: SocketAddr) -> ResourceMessageType;
    async fn handle_cmh_msg(message: CmhMessageType, from: SocketAddr) -> CmhMessageType;
    async fn other_joining(addr: SocketAddr) -> NeighborInfo;
    async fn leave_topology() -> bool;
    async fn change_next(next: SocketAddr) -> bool;
    async fn change_nnext(nnext: SocketAddr) -> bool;
    async fn change_prev(prev: SocketAddr) -> SocketAddr;
    async fn change_nnext_of_prev(next: SocketAddr) -> bool;
    async fn missing_node(addr: SocketAddr) -> bool;
}