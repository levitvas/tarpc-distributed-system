use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc};
use std::time::{Duration, Instant};
use colored::Colorize;
use tarpc::{client, context};
use tarpc::tokio_serde::formats::Json;
use tokio::sync::RwLock;
use crate::node_base::node::NeighborInfo;
use crate::rpc_base::service::NodeRpcClient;
use super::{service};

// Manager that will return the client for an ip address
#[derive(Clone, Debug)]
pub struct RpcClientManager {
    clients: Arc<RwLock<HashMap<SocketAddr, service::NodeRpcClient>>>,
}

impl RpcClientManager {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
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

    pub async fn get_c(&self, addr: SocketAddr) -> (Result<NodeRpcClient, ()>, context::Context) {
        let client = self.get_client(addr).await.map_err(|_| ());
        let mut ctx = context::current();
        (client, ctx)
    }
    
}




