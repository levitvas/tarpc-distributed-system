use std::collections::HashSet;
use std::error::Error;
use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use super::node::Node;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeMessage {
    pub k: SocketAddr,     // initiator
    pub m: u64,        // test number
    pub j: SocketAddr, // sender
    pub i: SocketAddr  // receiver
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CmhMessageType {
    // CMH Messages
    ProbeRequest(ProbeMessage),
    ProbeAnswer(SocketAddr, u64, SocketAddr, SocketAddr),
    DetectionStart,

    // Communication Messages
    RequestPermission(SocketAddr),
    DenyPermission,
    GrantPermission(SocketAddr),

    // Error Messages
    Success,
    Error
}
impl Node {
    pub async fn start_detection(&self) -> Result<CmhMessageType, Box<dyn Error>> {
        if *self.is_active.read().unwrap() {
            tracing::error!("Cannot start detection - node {} is active", self.id);
            return Ok(CmhMessageType::Error);
        }
        tracing::info!("Starting detection for node {}", self.id);

        let k = self.addr;

        {
            let mut last = self.last_test.write().unwrap();
            *last.entry(k).or_insert(0) += 1;
            let test_num = last[&k];

            let mut wait = self.wait_status.write().unwrap();
            wait.insert(k, true);
        }

        let test_num = self.last_test.read().unwrap()[&k];
        let mut count = 0;

        // Get unique set of nodes we're waiting for
        let waiting_for = self.waiting_messages_from.read().unwrap().clone();
        let mut sent_to = HashSet::new();

        for owner in waiting_for {
            if sent_to.insert(owner) {  // Only send once per unique owner
                let msg = CmhMessageType::ProbeRequest(ProbeMessage {
                    k,
                    m: test_num,
                    j: self.addr,
                    i: owner
                });
                tracing::debug!("Node {} sending probe to {}: {:?}", self.id, owner, msg);
                count += 1;
                self.probe_count.write().unwrap().insert(k, count);
                self.send_cmh_msg(msg, self.addr).await;
            }
        }

        Ok(CmhMessageType::Success)
    }


    // State Management
    pub async fn set_active(&self) -> Result<(), Box<dyn Error>> {
        tracing::info!(" Node {} setting active", self.id);
        *self.is_active.write().unwrap() = true;
        // if someone is waiting for me, send them receiveMessage messages from permission queue
        self.waiting_messages_from.write().unwrap().clear();
        let perm_list = self.permission_queue.write().unwrap().clone();
        for owner in perm_list {
            let msg = CmhMessageType::GrantPermission(owner);
            self.permission_queue.write().unwrap().remove(&owner);
            self.send_cmh_msg(msg, self.addr).await;
        }
        self.permission_queue.write().unwrap().clear();
        tracing::info!("Node {} is now active", self.id);
        Ok(())
    }

    pub async fn set_passive(&self) -> Result<(), Box<dyn Error>> {
        *self.is_active.write().unwrap() = false;
        Ok(())
    }

    pub async fn handle_waiting_for(&self, from: SocketAddr) -> Result<CmhMessageType, Box<dyn Error>> {
        *self.is_active.write().unwrap() = false;

        self.waiting_messages_from.write().unwrap().insert(from);

        // now send a perm request, this is just dummy communication
        let msg = CmhMessageType::RequestPermission(from);
        match self.send_cmh_msg(msg, self.addr).await {
            CmhMessageType::GrantPermission(addr) => {
                tracing::info!("Node {} granted permission to {}", from, self.id);
                self.waiting_messages_from.write().unwrap().remove(&from);
                *self.is_active.write().unwrap() = true;
                Ok(CmhMessageType::Success)
            },
            CmhMessageType::DenyPermission => {
                tracing::info!("Node {} denied permission to {}", self.id, from);
                Ok(CmhMessageType::Success)
            },
            CmhMessageType::Error => Err("Error sending message".into()),
            _ => todo!(),
        }
    }

    pub async fn handle_permission_from(&self, from: SocketAddr) -> Result<CmhMessageType, Box<dyn Error>> {
        self.waiting_messages_from.write().unwrap().remove(&from);

        if self.waiting_messages_from.read().unwrap().is_empty() {
            self.set_active().await?;
        }

        Ok(CmhMessageType::Success)
    }

    pub async fn handle_cmh_message(&self, msg: CmhMessageType, from: SocketAddr) -> Result<CmhMessageType, Box<dyn Error>> {
        tracing::info!("Node {} received CMH message from {}: {:?}", self.id, from, msg);

        // if from == self.addr {
        //     tracing::info!("Node {} received message from itself", self.id);
        //     return Ok(CmhMessageType::Error);
        // }

        match msg {
            CmhMessageType::RequestPermission(addr) => {
                if addr == self.addr {
                    tracing::info!("Node {} received permission request from {}", self.id, from);
                    if *self.is_active.read().unwrap() {
                        tracing::debug!("Node {} is active - granting permission to {}", self.id, from);
                        Ok(CmhMessageType::GrantPermission(from))
                    } else {
                        self.permission_queue.write().unwrap().insert(from);
                        Ok(CmhMessageType::DenyPermission)
                    }

                } else {
                    // Send to next node
                    Ok(self.send_cmh_msg(CmhMessageType::RequestPermission(addr), from).await)
                }
            },
            CmhMessageType::GrantPermission(addr) => {
                if addr == self.addr {
                    tracing::info!("Node {} received permission from {}", self.id, from);
                    self.handle_permission_from(from).await
                } else {
                    tracing::debug!("Node {} forwarding permission to {}", self.id, addr);
                    // Send to next node
                    Ok(self.send_cmh_msg(CmhMessageType::GrantPermission(addr), from).await)
                }
            },
            CmhMessageType::ProbeRequest(probe) => {
                // forward or handle
                if probe.i == self.addr {
                    self.handle_probe(probe).await
                } else {
                    tracing::debug!("Node {} forwarding probe to {}: {:?}", self.id, probe.i, probe);
                    Ok(self.send_cmh_msg(CmhMessageType::ProbeRequest(probe), from).await)
                }
            },
            CmhMessageType::ProbeAnswer(k, m, i, j) => {
                // forward or handle
                if j == self.addr {
                    self.handle_probe_answer(k, m, i, j).await
                } else {
                    tracing::debug!("Node {} forwarding probe to {}", self.id, i);
                    Ok(self.send_cmh_msg(CmhMessageType::ProbeAnswer(k, m, i, j), from).await)
                }
            },
            CmhMessageType::DetectionStart => self.start_detection().await,
            _ => Ok(CmhMessageType::Error)
        }
    }


    pub async fn handle_probe(&self, probe: ProbeMessage) -> Result<CmhMessageType, Box<dyn Error>> {
        if *self.is_active.read().unwrap() {
            tracing::debug!("Node {} is active - ignoring probe from {}", self.id, probe.k);
            return Ok(CmhMessageType::Success);
        }
        tracing::info!("Handling probe from {} for node {}", probe.j, probe.k);
        tracing::debug!("Probe: {:?}", probe);

        let test_num = {
            let last = self.last_test.read().unwrap();
            *last.get(&probe.k).unwrap_or(&0)
        };

        if probe.m > test_num {
            tracing::debug!("Node {} received probe from {} with test number {} > {}", self.id, probe.j, probe.m, test_num);
            {
                let mut last = self.last_test.write().unwrap();
                let mut wait = self.wait_status.write().unwrap();
                let mut parent = self.parent_nodes.write().unwrap();

                last.insert(probe.k, probe.m);
                wait.insert(probe.k, true);
                parent.insert(probe.k, probe.j);
            }

            let mut count = 0;
            let waiting_for = self.waiting_messages_from.read().unwrap().clone();
            let mut sent_to = HashSet::new();

            for owner in waiting_for {
                if sent_to.insert(owner) {
                    let msg = CmhMessageType::ProbeRequest(ProbeMessage {
                        k: probe.k,
                        m: probe.m,
                        j: self.addr,
                        i: owner
                    });
                    tracing::debug!("-+- Node {} sending probe to {}: {:?}", self.id, owner, msg);
                    count += 1;
                    self.probe_count.write().unwrap().insert(probe.k, count);
                    self.send_cmh_msg(msg, self.addr).await;
                }
            }

            self.probe_count.write().unwrap().insert(probe.k, count);
            tracing::debug!("--- {} {}", probe.k, self.probe_count.read().unwrap().get(&probe.k).unwrap());

        } else if *self.wait_status.read().unwrap().get(&probe.k).unwrap_or(&false) &&
            test_num == probe.m {
            tracing::debug!("~~ Node {} sending probe answer to {}: {} {} {} {}", self.id, probe.j, probe.k, probe.m, probe.i, probe.j);
            self.send_cmh_msg(
                CmhMessageType::ProbeAnswer(probe.k, probe.m, probe.i, probe.j),
                self.addr
            ).await;
        }

        Ok(CmhMessageType::Success)
    }

    async fn handle_probe_answer(&self, k: SocketAddr, m: u64, r: SocketAddr, i: SocketAddr)
                           -> Result<CmhMessageType, Box<dyn Error>>
    {
        // Check if active
        if *self.is_active.read().unwrap() {
            return Ok(CmhMessageType::Success);
        }

        tracing::debug!("~~ Node {} received probe answer {} {} {} {}", self.id, k, m, r, i);

        // Check conditions without holding locks during await
        let should_process = {
            let last_test = self.last_test.read().unwrap();
            let wait_status = self.wait_status.read().unwrap();
            m == *last_test.get(&k).unwrap_or(&0) &&
                *wait_status.get(&k).unwrap_or(&false)
        };

        if !should_process {
            return Ok(CmhMessageType::Success);
        }

        // Update probe count and check if it's zero
        let (is_zero, is_initiator, parent) = {
            // Decrement counter
            let mut count = self.probe_count.write().unwrap();
            tracing::debug!("{:?}", count);
            tracing::debug!("~`~ Node {} probe count for {} is {}", self.id, k, count.get(&k).unwrap());
            if let Some(n) = count.get_mut(&k) {
                *n -= 1;
                tracing::debug!("Node {} decremented probe count for {} to {}", self.id, k, n);

                // If zero, get additional info we need
                if *n == 0 {
                    let is_init = k == self.addr && i == self.addr;
                    let parent_addr = self.parent_nodes.read().unwrap().get(&k).cloned();
                    (true, is_init, parent_addr)
                } else {
                    (false, false, None)
                }
            } else {
                (false, false, None)
            }
        };

        // Handle zero count case without holding locks
        if is_zero {
            tracing::debug!("Node {} probe count for {} is zero", self.id, k);
            if is_initiator {
                tracing::info!("DEADLOCK DETECTED at node {}", self.id);
            } else if let Some(parent_addr) = parent {
                tracing::debug!("Node {} sending probe answer to {}: {} {} {} {}", self.id, parent_addr, k, m, i, parent_addr);
                self.send_cmh_msg(
                    CmhMessageType::ProbeAnswer(k, m, i, parent_addr),
                    self.addr
                ).await;
            }

            // Set wait status after async operations
            let mut wait_status = self.wait_status.write().unwrap();
            wait_status.insert(k, false);
        } else {
            tracing::debug!("Node {} probe count for {} is not zero!!!!!", self.id, k);
        }

        Ok(CmhMessageType::Success)
    }
}