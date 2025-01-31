use std::cmp::PartialEq;
use std::error::Error;
use std::net::SocketAddr;
use super::node::{Node, ResourceState};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use super::resources::ResourceMessageType::*;

#[derive(Debug, Serialize, Deserialize)]
pub enum ResourceMessageType {
    ResourceQuery(String),
    Acquire(String),
    Release(String),
    Owner(SocketAddr),
    Granted(String, SocketAddr),
    Queued,
    Unknown,
    Error,
    Success,
}

impl Node {
    
    pub async fn assign_resource(&self, resource: String) -> Result<(), Box<dyn Error>> {
        let mut owned = self.owned_resources.write().unwrap();
        owned.insert(resource.clone(), ResourceState {
            current_user: None,
            request_queue: Vec::new(),
        });
        Ok(())
    }

    // Find who owns a resource
    pub async fn find_resource_owner(&self, resource_id: &str) -> Result<SocketAddr, Box<dyn Error>> {
        // Check if we own it
        {
            let owned = self.owned_resources.read().unwrap();
            if owned.contains_key(resource_id) {
                return Ok(self.addr);
            }
        }
    
        // Forward query to next node
        let msg = ResourceQuery(resource_id.to_string());
        match self.send_resource_msg(msg, self.addr).await {
            Owner(s) => {
                Ok(s)
            },
            _ => Err("Resource not found".into())
        }
    }
    
    async fn use_resource(&self, resource: String, user: SocketAddr, owner: SocketAddr) -> Result<String, Box<dyn Error>>{
        if user != self.addr {
            // Panic or somethinmg?
            tracing::error!("Node {} received GRANTED message from wrong node", self.id.bold().red());
        }
        self.waiting_for.write().unwrap().remove(&resource);
        self.blocked_processes.write().unwrap().remove(&owner);
        // if self.waiting_for.read().unwrap().len() == 0 {
        //     self.set_active().await?;
        // }
        tracing::info!("Node {} acquired resource {}", self.id.bold().green(), resource.bold().green());
        self.used_resources.write().unwrap().insert(resource.clone(), owner);
        Ok("GRANTED".to_string())
    }

    // Acquire a resource
    pub async fn acquire_resource(&self, resource: String) -> Result<String, Box<dyn Error>> {
        tracing::info!("Node {} trying to acquire resource {}", self.id.bold().green(), resource.bold().green());

        // TODO: Optimize this, since this isnt needed
        let owner = self.find_resource_owner(&resource).await?;
        // self.set_passive().await?;

        tracing::error!("Node {} got owner {}", self.id.bold().green(), resource.bold().green());
        let msg = Acquire(resource.clone());
        let response = self.send_resource_msg(msg, self.addr).await;

        match response {
            Granted(resource, addr) => {
                self.use_resource(resource, addr, owner).await
            },
            Queued => {
                self.waiting_for.write().unwrap().insert(resource.clone(), owner);
                self.blocked_processes.write().unwrap().insert(owner);
                tracing::debug!("Node {} queued for resource {}", self.id.bold().green(), resource.bold().green());
                Ok("QUEUED".to_string())
            },
            _ => Ok("UNKNOWN".to_string())
        }
    }

    // Release a resource
    pub async fn release_resource(&self, resource: String) -> Result<(), Box<dyn Error>> {
        tracing::info!("Node {} trying to release resource {}", self.id.bold().green(), resource.bold().green());
        if !self.used_resources.read().unwrap().contains_key(&resource) {
            tracing::error!("Node {} does not own resource {}", self.id.bold().red(), resource.bold().red());
            return Ok(());
        }
        let owner = self.used_resources.write().unwrap().remove(&resource).unwrap();
        let msg = Release(resource.clone());
        self.send_resource_msg(msg, self.addr).await;
        Ok(())
    }

    // Process resource request
    async fn process_resource_request(&self, resource: String, from: SocketAddr) -> Result<ResourceMessageType, Box<dyn Error>> {
        let mut is_owned = false;
        let mut current_user = None;

        {
            let mut owned_resources = self.owned_resources.write().unwrap();
            if let Some(state) = owned_resources.get_mut(&resource) {
                is_owned = true;
                current_user = state.current_user;
                if current_user.is_none() {
                    state.current_user = Some(from);
                } else {
                    state.request_queue.push(from);
                }
            }
        }

        if is_owned {
            if current_user.is_none() {
                tracing::debug!("Node {} granting resource {} to {}", self.id.bold().green(), resource.bold().green(), from.to_string().bold().green());
                Ok(Granted(resource, from))
            } else {
                tracing::debug!("Node {} queuing resource {} for {}", self.id.bold().green(), resource.bold().green(), from.to_string().bold().green());
                Ok(Queued)
            }
        } else {
            // We don't own the resource, forward to next node
            tracing::debug!("Node {} forwarding resource {} to {}", self.id.bold().green(), resource.bold().green(), from.to_string().bold().green());
            let msg = Acquire(resource);
            Ok(self.send_resource_msg(msg, from).await)
        }
    }

    // Process resource release
    async fn process_resource_release(&self, resource: String, from: SocketAddr) -> Result<(), Box<dyn Error>> {
        let mut next_user = None;
        let mut is_owned = false;

        {
            let mut owned = self.owned_resources.write().unwrap();
            if let Some(state) = owned.get_mut(&resource) {
                if state.current_user == Some(from) {
                    is_owned = true;
                    state.current_user = None;
                    next_user = state.request_queue.pop();
                    if let Some(next) = next_user {
                        state.current_user = Some(next);
                    }
                }
            }
        }

        if is_owned {
            tracing::debug!("Node {} reacquiring resource {}", self.id.bold().green(), resource.bold().green());
            if let Some(next) = next_user {
                tracing::debug!("Node {} granting resource {} to {}", self.id.bold().green(), resource.bold().green(), next.to_string().bold().green());
                let msg = Granted(resource, next);
                self.send_resource_msg(msg, self.addr).await;
            }
            Ok(())
        } else {
            // We don't own the resource, forward to next node
            tracing::debug!("Node {} forwarding resource {} to {}", self.id.bold().green(), resource.bold().green(), from.to_string().bold().green());
            let msg = Release(resource);
            self.send_resource_msg(msg, from).await;
            Ok(())
        }
    }

    // Handle messages
    pub async fn handle_message(&self, msg: ResourceMessageType, from: SocketAddr) -> Result<ResourceMessageType, Box<dyn Error>> {
        // let _lock = self.critical_section.lock().await;
        tracing::debug!("Node {} starting handling of message {:?}", self.id.bold().green(), msg);

        match msg {
            ResourceMessageType::ResourceQuery(r_id) => {
                let resource_id = r_id.clone();
                tracing::debug!("Node {} handling resource query for {}", self.id.bold().green(), resource_id);
                if self.owned_resources.read().unwrap().contains_key(&resource_id) {
                    tracing::debug!("Node {} is owner of {}", self.id.bold().green(), resource_id);
                    Ok(Owner(self.addr))
                } else {
                    // Forward to next node
                    if from == self.neighbor_info.read().unwrap().next {
                        // We have already checked all nodes
                        tracing::debug!("Node {} is last node, resource not found", self.id.bold().green());
                        Ok(Unknown)
                    } else {
                        tracing::debug!("Node {} forwarding resource query for {}", self.id.bold().green(), resource_id);
                        Ok(self.send_resource_msg(ResourceQuery(resource_id), from).await)
                    }
                }
            }
            ResourceMessageType::Acquire(resource) => {
                tracing::debug!("Node {} handling acquire request for {}", self.id.bold().green(), resource);
                let response = self.process_resource_request(resource.clone(), from).await?;
                Ok(response)
            }
            ResourceMessageType::Release(resource) => {
                tracing::debug!("Node {} handling release request for {}", self.id.bold().green(), resource);
                self.process_resource_release(resource.clone(), from).await?;
                Ok(Success)
            }
            ResourceMessageType::Granted(resource, user) => {
                tracing::debug!("Node {} handling granted request for {}", self.id.bold().green(), resource);
                if user != self.addr {
                    // Forward to next node
                    tracing::debug!("Node {} forwarding granted request for {}", self.id.bold().green(), resource);
                    let msg = Granted(resource, user);
                    Ok(self.send_resource_msg(msg, from).await)
                } else {
                    self.use_resource(resource, user, from).await?;
                    Ok(Success)
                }
            }
            _ => {Ok(Unknown)}
        }
    }
}