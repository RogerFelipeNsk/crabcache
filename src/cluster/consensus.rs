//! Raft consensus protocol implementation
//! 
//! This module implements the Raft consensus algorithm for distributed
//! coordination and leader election in CrabCache clusters.

use crate::cluster::{ClusterNode, NodeId, ClusterResult, ClusterError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, warn};

/// Raft term number
pub type Term = u64;

/// Log index
pub type LogIndex = u64;

/// Raft log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub term: Term,
    pub index: LogIndex,
    pub command: RaftCommand,
    pub timestamp: u64,
}

/// Raft command types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftCommand {
    /// No-op command for heartbeats
    NoOp,
    /// Node join command
    NodeJoin { node: ClusterNode },
    /// Node leave command
    NodeLeave { node_id: NodeId },
    /// Configuration change
    ConfigChange { config: String },
    /// Shard assignment
    ShardAssignment { node_id: NodeId, shard_ids: Vec<u32> },
    /// Custom application command
    Application { data: Vec<u8> },
}

/// Raft node state
#[derive(Debug, Clone, PartialEq)]
pub enum RaftState {
    Follower {
        leader_id: Option<NodeId>,
        last_heartbeat: Instant,
    },
    Candidate {
        votes_received: HashSet<NodeId>,
        election_start: Instant,
        current_term: Term,
    },
    Leader {
        next_index: HashMap<NodeId, LogIndex>,
        match_index: HashMap<NodeId, LogIndex>,
        last_heartbeat_sent: Instant,
    },
}

/// Raft RPC messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftMessage {
    /// Request vote RPC
    RequestVote {
        term: Term,
        candidate_id: NodeId,
        last_log_index: LogIndex,
        last_log_term: Term,
    },
    /// Request vote response
    RequestVoteResponse {
        term: Term,
        vote_granted: bool,
    },
    /// Append entries RPC (heartbeat and log replication)
    AppendEntries {
        term: Term,
        leader_id: NodeId,
        prev_log_index: LogIndex,
        prev_log_term: Term,
        entries: Vec<LogEntry>,
        leader_commit: LogIndex,
    },
    /// Append entries response
    AppendEntriesResponse {
        term: Term,
        success: bool,
        match_index: LogIndex,
    },
}

/// Raft log storage
pub struct RaftLog {
    pub entries: Vec<LogEntry>,
    pub current_term: Term,
    pub voted_for: Option<NodeId>,
    pub commit_index: LogIndex,
    pub last_applied: LogIndex,
}

impl RaftLog {
    /// Create new empty log
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            current_term: 0,
            voted_for: None,
            commit_index: 0,
            last_applied: 0,
        }
    }
    
    /// Get last log index
    pub fn last_log_index(&self) -> LogIndex {
        self.entries.len() as LogIndex
    }
    
    /// Get last log term
    pub fn last_log_term(&self) -> Term {
        self.entries.last().map(|e| e.term).unwrap_or(0)
    }
    
    /// Append entries to log
    pub fn append_entries(&mut self, prev_log_index: LogIndex, entries: Vec<LogEntry>) -> bool {
        // Check if we have the previous log entry
        if prev_log_index > 0 && (prev_log_index as usize) > self.entries.len() {
            return false;
        }
        
        if prev_log_index > 0 {
            let prev_entry = &self.entries[(prev_log_index - 1) as usize];
            if prev_entry.index != prev_log_index {
                return false;
            }
        }
        
        // Remove conflicting entries
        if prev_log_index < self.entries.len() as LogIndex {
            self.entries.truncate(prev_log_index as usize);
        }
        
        // Append new entries
        for entry in entries {
            self.entries.push(entry);
        }
        
        true
    }
    
    /// Get entry at index
    pub fn get_entry(&self, index: LogIndex) -> Option<&LogEntry> {
        if index == 0 || index > self.entries.len() as LogIndex {
            None
        } else {
            self.entries.get((index - 1) as usize)
        }
    }
    
    /// Get entries from index
    pub fn get_entries_from(&self, from_index: LogIndex) -> Vec<LogEntry> {
        if from_index > self.entries.len() as LogIndex {
            Vec::new()
        } else {
            self.entries[(from_index as usize)..].to_vec()
        }
    }
    
    /// Update commit index
    pub fn update_commit_index(&mut self, new_commit_index: LogIndex) {
        if new_commit_index > self.commit_index {
            self.commit_index = new_commit_index.min(self.entries.len() as LogIndex);
        }
    }
    
    /// Apply committed entries
    pub fn apply_committed_entries(&mut self) -> Vec<LogEntry> {
        let mut applied = Vec::new();
        
        while self.last_applied < self.commit_index {
            self.last_applied += 1;
            if let Some(entry) = self.get_entry(self.last_applied) {
                applied.push(entry.clone());
            }
        }
        
        applied
    }
}

/// Raft peer information
#[derive(Debug, Clone)]
pub struct RaftPeer {
    pub node_id: NodeId,
    pub address: std::net::SocketAddr,
    pub next_index: LogIndex,
    pub match_index: LogIndex,
    pub last_contact: Instant,
}

/// Raft node implementation
pub struct RaftNode {
    /// Node configuration
    pub id: NodeId,
    pub address: std::net::SocketAddr,
    
    /// Raft state
    state: Arc<RwLock<RaftState>>,
    log: Arc<RwLock<RaftLog>>,
    peers: Arc<RwLock<HashMap<NodeId, RaftPeer>>>,
    
    /// Timing configuration
    election_timeout: Duration,
    heartbeat_interval: Duration,
    
    /// Communication channels
    message_sender: mpsc::UnboundedSender<(NodeId, RaftMessage)>,
    message_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<(NodeId, RaftMessage)>>>>,
    command_sender: mpsc::UnboundedSender<RaftCommand>,
    command_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<RaftCommand>>>>,
    
    /// Shutdown signal
    shutdown_sender: broadcast::Sender<()>,
    
    /// Applied command callback
    apply_callback: Option<Box<dyn Fn(&LogEntry) + Send + Sync>>,
}

impl RaftNode {
    /// Create new Raft node
    pub fn new(
        id: NodeId,
        address: std::net::SocketAddr,
        election_timeout: Duration,
        heartbeat_interval: Duration,
    ) -> Self {
        let (message_sender, message_receiver) = mpsc::unbounded_channel();
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        let (shutdown_sender, _) = broadcast::channel(1);
        
        Self {
            id,
            address,
            state: Arc::new(RwLock::new(RaftState::Follower {
                leader_id: None,
                last_heartbeat: Instant::now(),
            })),
            log: Arc::new(RwLock::new(RaftLog::new())),
            peers: Arc::new(RwLock::new(HashMap::new())),
            election_timeout,
            heartbeat_interval,
            message_sender,
            message_receiver: Arc::new(RwLock::new(Some(message_receiver))),
            command_sender,
            command_receiver: Arc::new(RwLock::new(Some(command_receiver))),
            shutdown_sender,
            apply_callback: None,
        }
    }
    
    /// Set callback for applied commands
    pub fn set_apply_callback<F>(&mut self, callback: F)
    where
        F: Fn(&LogEntry) + Send + Sync + 'static,
    {
        self.apply_callback = Some(Box::new(callback));
    }
    
    /// Start the Raft node
    pub async fn start(&self) -> ClusterResult<()> {
        info!("Starting Raft node {} at {}", self.id, self.address);
        
        // Start main loop
        self.start_main_loop().await;
        
        // Start command processor
        self.start_command_processor().await;
        
        Ok(())
    }
    
    /// Add peer to the cluster
    pub async fn add_peer(&self, peer: RaftPeer) {
        let peer_id = peer.node_id;
        let mut peers = self.peers.write().await;
        peers.insert(peer.node_id, peer);
        info!("Added peer {} to Raft cluster", peer_id);
    }
    
    /// Remove peer from the cluster
    pub async fn remove_peer(&self, node_id: NodeId) {
        let mut peers = self.peers.write().await;
        peers.remove(&node_id);
        info!("Removed peer {} from Raft cluster", node_id);
    }
    
    /// Submit command to the cluster
    pub async fn submit_command(&self, command: RaftCommand) -> ClusterResult<()> {
        self.command_sender.send(command)
            .map_err(|_| ClusterError::ConsensusError { 
                message: "Command channel closed".to_string() 
            })?;
        Ok(())
    }
    
    /// Get current leader
    pub async fn get_leader(&self) -> Option<NodeId> {
        let state = self.state.read().await;
        match &*state {
            RaftState::Leader { .. } => Some(self.id),
            RaftState::Follower { leader_id, .. } => *leader_id,
            RaftState::Candidate { .. } => None,
        }
    }
    
    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        matches!(*self.state.read().await, RaftState::Leader { .. })
    }
    
    /// Get current term
    pub async fn get_current_term(&self) -> Term {
        self.log.read().await.current_term
    }
    
    /// Start main Raft loop
    async fn start_main_loop(&self) {
        let state = self.state.clone();
        let log = self.log.clone();
        let peers = self.peers.clone();
        let message_sender = self.message_sender.clone();
        let mut message_receiver = self.message_receiver.write().await.take()
            .expect("Message receiver should be available");
        let mut shutdown_receiver = self.shutdown_sender.subscribe();
        
        let election_timeout = self.election_timeout;
        let heartbeat_interval = self.heartbeat_interval;
        let node_id = self.id;
        
        tokio::spawn(async move {
            let mut election_timer = interval(election_timeout);
            let mut heartbeat_timer = interval(heartbeat_interval);
            
            loop {
                tokio::select! {
                    _ = election_timer.tick() => {
                        Self::handle_election_timeout(
                            &state, &log, &peers, &message_sender, node_id, election_timeout
                        ).await;
                    }
                    _ = heartbeat_timer.tick() => {
                        Self::handle_heartbeat_timeout(
                            &state, &log, &peers, &message_sender, node_id, heartbeat_interval
                        ).await;
                    }
                    message = message_receiver.recv() => {
                        if let Some((from_node, raft_message)) = message {
                            Self::handle_raft_message(
                                &state, &log, &peers, &message_sender, node_id, from_node, raft_message
                            ).await;
                        }
                    }
                    _ = shutdown_receiver.recv() => {
                        info!("Shutting down Raft main loop for node {}", node_id);
                        break;
                    }
                }
            }
        });
    }
    
    /// Handle election timeout
    async fn handle_election_timeout(
        state: &Arc<RwLock<RaftState>>,
        log: &Arc<RwLock<RaftLog>>,
        peers: &Arc<RwLock<HashMap<NodeId, RaftPeer>>>,
        message_sender: &mpsc::UnboundedSender<(NodeId, RaftMessage)>,
        node_id: NodeId,
        election_timeout: Duration,
    ) {
        let mut state_guard = state.write().await;
        
        match &*state_guard {
            RaftState::Follower { last_heartbeat, .. } => {
                if last_heartbeat.elapsed() > election_timeout {
                    // Start election
                    info!("Node {} starting election", node_id);
                    
                    let mut log_guard = log.write().await;
                    log_guard.current_term += 1;
                    log_guard.voted_for = Some(node_id);
                    
                    *state_guard = RaftState::Candidate {
                        votes_received: HashSet::from([node_id]),
                        election_start: Instant::now(),
                        current_term: log_guard.current_term,
                    };
                    
                    let current_term = log_guard.current_term;
                    let last_log_index = log_guard.last_log_index();
                    let last_log_term = log_guard.last_log_term();
                    
                    drop(log_guard);
                    drop(state_guard);
                    
                    // Send RequestVote to all peers
                    let peers_guard = peers.read().await;
                    for peer in peers_guard.values() {
                        let request = RaftMessage::RequestVote {
                            term: current_term,
                            candidate_id: node_id,
                            last_log_index,
                            last_log_term,
                        };
                        
                        if let Err(e) = message_sender.send((peer.node_id, request)) {
                            error!("Failed to send RequestVote to {}: {}", peer.node_id, e);
                        }
                    }
                }
            }
            RaftState::Candidate { election_start, .. } => {
                if election_start.elapsed() > election_timeout {
                    // Restart election
                    info!("Node {} restarting election", node_id);
                    
                    let mut log_guard = log.write().await;
                    log_guard.current_term += 1;
                    log_guard.voted_for = Some(node_id);
                    
                    *state_guard = RaftState::Candidate {
                        votes_received: HashSet::from([node_id]),
                        election_start: Instant::now(),
                        current_term: log_guard.current_term,
                    };
                }
            }
            RaftState::Leader { .. } => {
                // Leaders don't have election timeouts
            }
        }
    }
    
    /// Handle heartbeat timeout (for leaders)
    async fn handle_heartbeat_timeout(
        state: &Arc<RwLock<RaftState>>,
        log: &Arc<RwLock<RaftLog>>,
        peers: &Arc<RwLock<HashMap<NodeId, RaftPeer>>>,
        message_sender: &mpsc::UnboundedSender<(NodeId, RaftMessage)>,
        node_id: NodeId,
        heartbeat_interval: Duration,
    ) {
        let mut state_guard = state.write().await;
        
        if let RaftState::Leader { last_heartbeat_sent, next_index, match_index: _ } = &mut *state_guard {
            if last_heartbeat_sent.elapsed() > heartbeat_interval {
                *last_heartbeat_sent = Instant::now();
                
                let log_guard = log.read().await;
                let current_term = log_guard.current_term;
                let commit_index = log_guard.commit_index;
                
                // Send heartbeats to all peers
                let peers_guard = peers.read().await;
                for peer in peers_guard.values() {
                    let prev_log_index = next_index.get(&peer.node_id).copied().unwrap_or(1) - 1;
                    let prev_log_term = if prev_log_index > 0 {
                        log_guard.get_entry(prev_log_index).map(|e| e.term).unwrap_or(0)
                    } else {
                        0
                    };
                    
                    let entries = log_guard.get_entries_from(prev_log_index + 1);
                    
                    let heartbeat = RaftMessage::AppendEntries {
                        term: current_term,
                        leader_id: node_id,
                        prev_log_index,
                        prev_log_term,
                        entries,
                        leader_commit: commit_index,
                    };
                    
                    if let Err(e) = message_sender.send((peer.node_id, heartbeat)) {
                        error!("Failed to send heartbeat to {}: {}", peer.node_id, e);
                    }
                }
            }
        }
    }
    
    /// Handle incoming Raft message
    async fn handle_raft_message(
        state: &Arc<RwLock<RaftState>>,
        log: &Arc<RwLock<RaftLog>>,
        peers: &Arc<RwLock<HashMap<NodeId, RaftPeer>>>,
        message_sender: &mpsc::UnboundedSender<(NodeId, RaftMessage)>,
        node_id: NodeId,
        from_node: NodeId,
        message: RaftMessage,
    ) {
        match message {
            RaftMessage::RequestVote { term, candidate_id, last_log_index, last_log_term } => {
                Self::handle_request_vote(
                    state, log, message_sender, node_id, from_node, 
                    term, candidate_id, last_log_index, last_log_term
                ).await;
            }
            RaftMessage::RequestVoteResponse { term, vote_granted } => {
                Self::handle_request_vote_response(
                    state, log, peers, node_id, from_node, term, vote_granted
                ).await;
            }
            RaftMessage::AppendEntries { term, leader_id, prev_log_index, prev_log_term, entries, leader_commit } => {
                Self::handle_append_entries(
                    state, log, message_sender, node_id, from_node,
                    term, leader_id, prev_log_index, prev_log_term, entries, leader_commit
                ).await;
            }
            RaftMessage::AppendEntriesResponse { term, success, match_index } => {
                Self::handle_append_entries_response(
                    state, log, peers, node_id, from_node, term, success, match_index
                ).await;
            }
        }
    }
    
    /// Handle RequestVote RPC
    async fn handle_request_vote(
        state: &Arc<RwLock<RaftState>>,
        log: &Arc<RwLock<RaftLog>>,
        message_sender: &mpsc::UnboundedSender<(NodeId, RaftMessage)>,
        node_id: NodeId,
        from_node: NodeId,
        term: Term,
        candidate_id: NodeId,
        last_log_index: LogIndex,
        last_log_term: Term,
    ) {
        let mut log_guard = log.write().await;
        let mut vote_granted = false;
        
        // Update term if necessary
        if term > log_guard.current_term {
            log_guard.current_term = term;
            log_guard.voted_for = None;
            
            // Convert to follower
            let mut state_guard = state.write().await;
            *state_guard = RaftState::Follower {
                leader_id: None,
                last_heartbeat: Instant::now(),
            };
        }
        
        // Check if we can vote for this candidate
        if term == log_guard.current_term &&
           (log_guard.voted_for.is_none() || log_guard.voted_for == Some(candidate_id)) {
            
            // Check if candidate's log is at least as up-to-date as ours
            let our_last_log_term = log_guard.last_log_term();
            let our_last_log_index = log_guard.last_log_index();
            
            let log_ok = last_log_term > our_last_log_term ||
                        (last_log_term == our_last_log_term && last_log_index >= our_last_log_index);
            
            if log_ok {
                vote_granted = true;
                log_guard.voted_for = Some(candidate_id);
                debug!("Node {} voted for {} in term {}", node_id, candidate_id, term);
            }
        }
        
        let response_term = log_guard.current_term;
        drop(log_guard);
        
        // Send response
        let response = RaftMessage::RequestVoteResponse {
            term: response_term,
            vote_granted,
        };
        
        if let Err(e) = message_sender.send((from_node, response)) {
            error!("Failed to send RequestVoteResponse to {}: {}", from_node, e);
        }
    }
    
    /// Handle RequestVoteResponse
    async fn handle_request_vote_response(
        state: &Arc<RwLock<RaftState>>,
        log: &Arc<RwLock<RaftLog>>,
        peers: &Arc<RwLock<HashMap<NodeId, RaftPeer>>>,
        node_id: NodeId,
        from_node: NodeId,
        term: Term,
        vote_granted: bool,
    ) {
        let mut state_guard = state.write().await;
        
        if let RaftState::Candidate { votes_received, current_term, .. } = &mut *state_guard {
            if term == *current_term && vote_granted {
                votes_received.insert(from_node);
                
                // Check if we have majority
                let peers_guard = peers.read().await;
                let total_nodes = peers_guard.len() + 1; // +1 for self
                let majority = total_nodes / 2 + 1;
                
                if votes_received.len() >= majority {
                    // Become leader
                    info!("Node {} became leader in term {}", node_id, current_term);
                    
                    let log_guard = log.read().await;
                    let last_log_index = log_guard.last_log_index();
                    
                    let mut next_index = HashMap::new();
                    let mut match_index = HashMap::new();
                    
                    for peer_id in peers_guard.keys() {
                        next_index.insert(*peer_id, last_log_index + 1);
                        match_index.insert(*peer_id, 0);
                    }
                    
                    *state_guard = RaftState::Leader {
                        next_index,
                        match_index,
                        last_heartbeat_sent: Instant::now(),
                    };
                }
            }
        }
    }
    
    /// Handle AppendEntries RPC
    async fn handle_append_entries(
        state: &Arc<RwLock<RaftState>>,
        log: &Arc<RwLock<RaftLog>>,
        message_sender: &mpsc::UnboundedSender<(NodeId, RaftMessage)>,
        node_id: NodeId,
        from_node: NodeId,
        term: Term,
        leader_id: NodeId,
        prev_log_index: LogIndex,
        prev_log_term: Term,
        entries: Vec<LogEntry>,
        leader_commit: LogIndex,
    ) {
        let mut log_guard = log.write().await;
        let mut success = false;
        
        // Update term and convert to follower if necessary
        if term >= log_guard.current_term {
            log_guard.current_term = term;
            log_guard.voted_for = None;
            
            let mut state_guard = state.write().await;
            *state_guard = RaftState::Follower {
                leader_id: Some(leader_id),
                last_heartbeat: Instant::now(),
            };
            
            // Check log consistency
            if prev_log_index == 0 || 
               (prev_log_index <= log_guard.last_log_index() &&
                log_guard.get_entry(prev_log_index).map(|e| e.term).unwrap_or(0) == prev_log_term) {
                
                // Append entries
                success = log_guard.append_entries(prev_log_index, entries);
                
                if success {
                    // Update commit index
                    if leader_commit > log_guard.commit_index {
                        log_guard.update_commit_index(leader_commit);
                    }
                }
            }
        }
        
        let response_term = log_guard.current_term;
        let match_index = if success { log_guard.last_log_index() } else { 0 };
        drop(log_guard);
        
        // Send response
        let response = RaftMessage::AppendEntriesResponse {
            term: response_term,
            success,
            match_index,
        };
        
        if let Err(e) = message_sender.send((from_node, response)) {
            error!("Failed to send AppendEntriesResponse to {}: {}", from_node, e);
        }
    }
    
    /// Handle AppendEntriesResponse
    async fn handle_append_entries_response(
        state: &Arc<RwLock<RaftState>>,
        log: &Arc<RwLock<RaftLog>>,
        peers: &Arc<RwLock<HashMap<NodeId, RaftPeer>>>,
        node_id: NodeId,
        from_node: NodeId,
        term: Term,
        success: bool,
        match_index: LogIndex,
    ) {
        let mut state_guard = state.write().await;
        
        if let RaftState::Leader { next_index, match_index: leader_match_index, .. } = &mut *state_guard {
            if success {
                // Update indices
                next_index.insert(from_node, match_index + 1);
                leader_match_index.insert(from_node, match_index);
                
                // Check if we can advance commit index
                let mut log_guard = log.write().await;
                let peers_guard = peers.read().await;
                
                for n in (log_guard.commit_index + 1)..=log_guard.last_log_index() {
                    let mut count = 1; // Count self
                    
                    for match_idx in leader_match_index.values() {
                        if *match_idx >= n {
                            count += 1;
                        }
                    }
                    
                    let majority = (peers_guard.len() + 1) / 2 + 1;
                    if count >= majority {
                        if let Some(entry) = log_guard.get_entry(n) {
                            if entry.term == log_guard.current_term {
                                log_guard.update_commit_index(n);
                            }
                        }
                    }
                }
            } else {
                // Decrement next_index and retry
                if let Some(next_idx) = next_index.get_mut(&from_node) {
                    if *next_idx > 1 {
                        *next_idx -= 1;
                    }
                }
            }
        }
    }
    
    /// Start command processor
    async fn start_command_processor(&self) {
        let log = self.log.clone();
        let state = self.state.clone();
        let mut command_receiver = self.command_receiver.write().await.take()
            .expect("Command receiver should be available");
        let mut shutdown_receiver = self.shutdown_sender.subscribe();
        let node_id = self.id;
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    command = command_receiver.recv() => {
                        if let Some(cmd) = command {
                            // Only leaders can process commands
                            let is_leader = matches!(*state.read().await, RaftState::Leader { .. });
                            
                            if is_leader {
                                let mut log_guard = log.write().await;
                                let entry = LogEntry {
                                    term: log_guard.current_term,
                                    index: log_guard.last_log_index() + 1,
                                    command: cmd,
                                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                };
                                
                                log_guard.entries.push(entry);
                                debug!("Node {} appended command to log at index {}", 
                                       node_id, log_guard.last_log_index());
                            } else {
                                warn!("Node {} received command but is not leader", node_id);
                            }
                        }
                    }
                    _ = shutdown_receiver.recv() => {
                        info!("Shutting down command processor for node {}", node_id);
                        break;
                    }
                }
            }
        });
    }
    
    /// Shutdown the Raft node
    pub async fn shutdown(&self) -> ClusterResult<()> {
        info!("Shutting down Raft node {}", self.id);
        let _ = self.shutdown_sender.send(());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_raft_log_creation() {
        let log = RaftLog::new();
        assert_eq!(log.current_term, 0);
        assert_eq!(log.last_log_index(), 0);
        assert_eq!(log.last_log_term(), 0);
        assert!(log.entries.is_empty());
    }
    
    #[test]
    fn test_raft_log_append() {
        let mut log = RaftLog::new();
        
        let entry1 = LogEntry {
            term: 1,
            index: 1,
            command: RaftCommand::NoOp,
            timestamp: 0,
        };
        
        let entry2 = LogEntry {
            term: 1,
            index: 2,
            command: RaftCommand::NoOp,
            timestamp: 1,
        };
        
        // Append entries
        assert!(log.append_entries(0, vec![entry1.clone()]));
        assert_eq!(log.last_log_index(), 1);
        
        assert!(log.append_entries(1, vec![entry2.clone()]));
        assert_eq!(log.last_log_index(), 2);
        
        // Check entries
        assert_eq!(log.get_entry(1).unwrap().term, 1);
        assert_eq!(log.get_entry(2).unwrap().term, 1);
    }
    
    #[test]
    fn test_raft_log_commit() {
        let mut log = RaftLog::new();
        
        let entry = LogEntry {
            term: 1,
            index: 1,
            command: RaftCommand::NoOp,
            timestamp: 0,
        };
        
        log.append_entries(0, vec![entry]);
        log.update_commit_index(1);
        
        assert_eq!(log.commit_index, 1);
        
        let applied = log.apply_committed_entries();
        assert_eq!(applied.len(), 1);
        assert_eq!(log.last_applied, 1);
    }
    
    #[tokio::test]
    async fn test_raft_node_creation() {
        let node_id = NodeId::generate();
        let address = "127.0.0.1:8000".parse().unwrap();
        let election_timeout = Duration::from_millis(5000);
        let heartbeat_interval = Duration::from_millis(1000);
        
        let node = RaftNode::new(node_id, address, election_timeout, heartbeat_interval);
        
        assert_eq!(node.id, node_id);
        assert_eq!(node.address, address);
        assert!(!node.is_leader().await);
        assert_eq!(node.get_current_term().await, 0);
    }
    
    #[tokio::test]
    async fn test_raft_peer_management() {
        let node_id = NodeId::generate();
        let address = "127.0.0.1:8000".parse().unwrap();
        let election_timeout = Duration::from_millis(5000);
        let heartbeat_interval = Duration::from_millis(1000);
        
        let node = RaftNode::new(node_id, address, election_timeout, heartbeat_interval);
        
        let peer_id = NodeId::generate();
        let peer_address = "127.0.0.1:8001".parse().unwrap();
        let peer = RaftPeer {
            node_id: peer_id,
            address: peer_address,
            next_index: 1,
            match_index: 0,
            last_contact: Instant::now(),
        };
        
        node.add_peer(peer).await;
        
        let peers = node.peers.read().await;
        assert!(peers.contains_key(&peer_id));
        assert_eq!(peers.get(&peer_id).unwrap().address, peer_address);
    }
    
    #[test]
    fn test_raft_command_serialization() {
        let command = RaftCommand::NodeJoin {
            node: crate::cluster::ClusterNode::new(
                NodeId::generate(),
                "127.0.0.1:8000".parse().unwrap(),
                "127.0.0.1:8001".parse().unwrap(),
                crate::cluster::node::NodeCapabilities::default(),
            ),
        };
        
        let serialized = bincode::serialize(&command).unwrap();
        let deserialized: RaftCommand = bincode::deserialize(&serialized).unwrap();
        
        match deserialized {
            RaftCommand::NodeJoin { .. } => (),
            _ => panic!("Wrong command type"),
        }
    }
}