use std::net::SocketAddr;
use tarpc::context::Context;
use crate::node_base::node::NeighborInfo;

#[tarpc::service]
pub trait NodeRpc {
    async fn heartbeat() -> bool;
    async fn msg(message: String) -> String;
    async fn other_joining(addr: SocketAddr) -> NeighborInfo;
    async fn leave_topology() -> bool;
    async fn change_next(next: SocketAddr) -> bool;
    async fn change_nnext(nnext: SocketAddr) -> bool;
    async fn change_prev(prev: SocketAddr) -> bool;
    async fn change_nnext_of_prev(next: SocketAddr) -> bool;
    async fn missing_node(addr: SocketAddr) -> bool;
}