mod node_base;
mod api_base;
mod rpc_base;

use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;
use rpc_base::server;
use crate::api_base::api;
use crate::node_base::node;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with some better defaults
    // std::env::set_var("RUST_LOG", "asd");
    std::env::set_var("RUST_LOG", "tarpc=error,tarpc_distributed_system=debug");
    
    tracing_subscriber::fmt()
        .with_target(false) // Don't include target
        .with_level(true) // Include log level
        .with_file(true) // Include file where the log was generated
        .with_line_number(true) // Include line numbers
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Get command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        tracing::error!("Incorrect arguments provided");
        tracing::info!("Usage: {} <ip> <port>", args[0]);
        std::process::exit(1);
    }

    let ip = &args[1];
    let port: u16 = args[2].parse()?;
    let rpc_addr: SocketAddr = format!("{}:{}", ip, port).parse()?;
    tracing::info!("Starting node with address: {}", rpc_addr);

    // Create node and start server
    let node = node::Node::new(format!("node_{}", port), rpc_addr);
    
    let rpc_node = node.clone();
    tokio::spawn(async move {
        if let Err(e) = server::serve_rpc(rpc_node).await {
            tracing::error!("RPC server error: {}", e);
        }
    });
    
    let rest_node = node.clone();
    let rest_addr = format!("{}:{}", ip, port+1).parse()?;
    tracing::info!("Created node with id: {}", node.id);
    api::serve(rest_node, rest_addr).await?;

    Ok(())
}
