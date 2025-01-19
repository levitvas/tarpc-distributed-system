use std::net::SocketAddr;
use std::sync::Arc;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Json, Router};
use axum::routing::{get, post};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use crate::node_base::node::Node;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: String,
    node_id: String,
    addr: String,
}

#[derive(Debug, Deserialize)]
pub struct DelayConfig {
    delay_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct KillReviveRequest {
    node_id: String,
}

// Gracefully leave the topology
async fn leave(State(node): State<Arc<Node>>) -> StatusCode {
    tracing::info!("Node {} gracefully leaving the topology", node.id.bold().green());

    match node.leave().await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// Kill a node (simulate crash)
async fn kill(
    State(node): State<Arc<Node>>,
    Json(payload): Json<KillReviveRequest>,
) -> StatusCode {
    tracing::info!("Killing node {}", payload.node_id.bold().red());

    match node.kill(&payload.node_id).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// Revive a node
async fn revive(
    State(node): State<Arc<Node>>,
    Json(payload): Json<KillReviveRequest>,
) -> StatusCode {
    tracing::info!("Reviving node {}", payload.node_id.bold().green());

    match node.revive(&payload.node_id).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn health_check(State(node): State<Arc<Node>>) -> Json<HealthResponse> {
    tracing::debug!("Health check requested for node {}", node.id);
    Json(HealthResponse {
        status: "healthy".to_string(),
        node_id: node.id.clone(),
        addr: node.addr.to_string(),
    })
}

async fn set_delay(
    State(node): State<Arc<Node>>,
    Json(config): Json<DelayConfig>,
) -> StatusCode {
    tracing::debug!("Delay update requested for node {}: {}ms", node.id, config.delay_ms);
    node.set_delay(config.delay_ms);
    StatusCode::OK
}

#[derive(Deserialize)]
struct JoinRequest {
    address: String, 
}

// Joins other node, accepts address
async fn join_other(
    State(node): State<Arc<Node>>,
    Json(payload): Json<JoinRequest>,
) -> StatusCode {
    let address: SocketAddr = payload.address.parse().expect("Invalid address format");
    tracing::debug!("Node {} will request to join {}", node.id.bold().green(), address.to_string().bold().green());
    node.try_join_other(address).await;
    StatusCode::OK
}

async fn status(State(node): State<Arc<Node>>) {
    tracing::debug!("Status requested for node {}", node.id);
    node.print_status();
}

pub async fn serve(node: Arc<Node>, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/delay", post(set_delay))
        .route("/joinother", post(join_other))
        .route("/status", get(status))
        .route("/leave", post(leave))
        .route("/kill", post(kill))
        .route("/revive", post(revive))
        .with_state(node);

    tracing::info!("Starting REST API server on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}




