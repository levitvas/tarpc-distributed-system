use std::net::SocketAddr;
use std::sync::Arc;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Json, Router};
use axum::routing::{get, post};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use crate::node_base::node::Node;
use crate::node_base::resources::ResourceMessageType::{Acquire, Release, ResourceQuery};

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
    State(node): State<Arc<Node>>
) -> StatusCode {
    tracing::info!("Killing node {}", node.id.bold().red());

    match node.kill(node.addr).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// Revive a node
async fn revive(
    State(node): State<Arc<Node>>
) -> StatusCode {
    tracing::info!("Reviving node {}", node.id.bold().yellow());

    match node.revive(node.addr).await {
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

#[axum::debug_handler]
async fn send_message(State(node): State<Arc<Node>>) -> Result<(), StatusCode> {
    tracing::debug!("Sending message to next node from {}", node.id);
    let ans = node.send_resource_msg(ResourceQuery("C".to_string()), node.addr).await;
    tracing::info!("Received answer: {:?}", ans);
    Ok(())
}

#[derive(Deserialize)]
struct ResourceRequest {
    resource: String,
}

async fn acquire_resource(State(node): State<Arc<Node>>, Json(payload): Json<ResourceRequest>) -> Result<(), StatusCode> {
    tracing::debug!("Requesting resource from {}", node.id);
    let resource = payload.resource.clone();
    let ans = node.acquire_resource(resource).await;
    tracing::info!("Received answer: {:?}", ans);
    Ok(())
}

async fn release_resource(State(node): State<Arc<Node>>, Json(payload): Json<ResourceRequest>) -> Result<(), StatusCode> {
    tracing::debug!("Releasing resource from  {}", node.id);
    let resource = payload.resource.clone();
    let ans = node.release_resource(resource).await;
    tracing::info!("Received answer: {:?}", ans);
    Ok(())
}

async fn start_detection(
    State(node): State<Arc<Node>>
) -> Result<(), StatusCode> {
    tracing::info!("Starting detection on node {}", node.id);
    match node.start_detection().await {
        Ok(_) => Ok(()),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn wait_for_message(
    State(node): State<Arc<Node>>,
    Json(payload): Json<JoinRequest>,
) -> Result<(), StatusCode> {
    let address: SocketAddr = payload.address.parse().expect("Invalid address format");
    tracing::debug!("Node {} will wait for message from {}", node.id.bold().green(), address.to_string().bold().green());
    match node.handle_waiting_for(address).await {
        Ok(_) => Ok(()),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn set_active(
    State(node): State<Arc<Node>>
) -> Result<(), StatusCode> {
    tracing::info!("Setting node {} active", node.id);
    match node.set_active().await {
        Ok(_) => Ok(()),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn set_passive(
    State(node): State<Arc<Node>>
) -> Result<(), StatusCode> {
    tracing::info!("Setting node {} passive", node.id);
    match node.set_passive().await {
        Ok(_) => Ok(()),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
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
        .route("/msg", post(send_message))
        .route("/acquire", post(acquire_resource))
        .route("/release", post(release_resource))
        .route("/detection/start", post(start_detection))
        .route("/waitForMessage", post(wait_for_message))
        .route("/setActive", post(set_active))
        .route("/setPassive", post(set_passive))
        .with_state(node);

    tracing::info!("Starting REST API server on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}




