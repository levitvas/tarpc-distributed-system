use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc};
use std::time::{Duration, Instant};
use colored::Colorize;
use tarpc::{client, context};
use tarpc::tokio_serde::formats::Json;
use tokio::sync::RwLock;
use crate::node_base::node::NeighborInfo;
use super::{service};

// Manager that will return the client for an ip address
#[derive(Clone, Debug)]
pub struct RpcClientManager {
    own_addr: SocketAddr,
    clients: Arc<RwLock<HashMap<SocketAddr, service::NodeRpcClient>>>,
}

impl RpcClientManager {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            own_addr: addr,
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_client(&self, addr: SocketAddr) -> Result<service::NodeRpcClient, Box<dyn std::error::Error>> {
        let transport = tarpc::serde_transport::tcp::connect(addr, Json::default).await?;

        // Create the client with the transport
        let client = service::NodeRpcClient::new(
            client::Config::default(),
            transport
        ).spawn();

        Ok(client)
    }

    pub async fn get_client(&self, addr: SocketAddr) -> Result<service::NodeRpcClient, Box<dyn std::error::Error>> {
        let mut clients = self.clients.write().await;
        if let std::collections::hash_map::Entry::Vacant(e) = clients.entry(addr) {
            match self.create_client(addr).await {
                Ok(client) => e.insert(client),
                Err(e) => return Err(e),
            };
        }
        
        // // Test if the server is still alive
        // let mut ctx = context::current();
        // ctx.deadline = Instant::now() + Duration::from_secs(5);
        // match clients.get(&addr).unwrap().heartbeat(ctx).await {
        //     Ok(_) => {},
        //     Err(e) => {
        //         clients.remove(&addr);
        //         self.missing_node(addr).await.unwrap();
        //         // match self.missing_node(addr).await {
        //         //     Ok(_) => {},
        //         //     Err(e) => return Err(e),
        //         // }
        //     }
        // }
        
        Ok(clients.get(&addr).unwrap().clone())
    }

    pub async fn send_msg(&self, addr: SocketAddr, msg: String) -> Result<String, Box<dyn std::error::Error>> {
        let client = self.get_client(addr).await?;
        let mut ctx = context::current();
        // Add deadline to request, after five seconds the request will be cancelled
        ctx.deadline = Instant::now() + Duration::from_secs(5);

        match client.msg(ctx, msg).await {
            Ok(response) => Ok(response),
            Err(e) => Err(format!("RPC error: {}", e).into()),
        }
    }
    
    // Client will invoke other_joining on the server 
    pub async fn join_other(&self, addr: SocketAddr) -> Result<NeighborInfo, Box<dyn std::error::Error>> {
        let client = self.get_client(addr).await?;
        let mut ctx = context::current();
        // Add deadline to request, after five seconds the request will be cancelled
        ctx.deadline = Instant::now() + Duration::from_secs(5);

        match client.other_joining(ctx, self.own_addr).await {
            Ok(response) => Ok(response),
            Err(e) => Err(format!("RPC error: {}", e).into()),
        }
    }

    pub async fn leave_topology(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.get_client(self.own_addr).await?;
        let mut ctx = context::current();
        // Add deadline to request, after five seconds the request will be cancelled
        ctx.deadline = Instant::now() + Duration::from_secs(5);

        match client.leave_topology(ctx).await {
            Ok(response) => Ok(()),
            Err(e) => Err(format!("RPC error: {}", e).into()),
        }
    }

    // Fix topology method
    pub async fn missing_node(&self, missing_node: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.get_client(self.own_addr).await?;
        let ctx = context::current();
        
        match client.missing_node(ctx, missing_node).await {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("RPC error: {}", e).into()),
        }
    }
    
}




