//! Per-resource state management with thread-safe CRDT synchronization.
//!
//! This module provides centralized state management for collaborative document editing.
//! It maintains an in-memory registry of documents, each with its own CRDT instance and
//! metadata. All access is thread-safe via `Arc<RwLock<>>`.

use std::sync::Arc;
use std::time::SystemTime;
use parking_lot::RwLock;
use std::collections::HashMap;
use crate::merge::DiamondCRDT;
use serde_json::Value;

/// The state of a single collaborative resource.
///
/// Each resource maintains its own CRDT instance along with synchronization metadata.
/// State is protected by a RwLock for safe concurrent access from multiple async tasks.
///
/// # Invariants
///
/// - The CRDT is always at the tip of its operation log
/// - `last_sync` is updated whenever operations are applied
/// - State is never invalid or inconsistent
#[derive(Debug, Clone)]
pub struct ResourceState {
    /// The document's CRDT with full operation history
    pub crdt: DiamondCRDT,

    /// When this resource was last modified
    pub last_sync: SystemTime,
}

/// Thread-safe registry of collaborative document resources.
///
/// `ResourceStateManager` maintains the canonical state for all active resources in the
/// system. It provides methods for creating resources, applying edits, and querying state.
/// All operations are atomic and thread-safe.
///
/// # Thread Safety
///
/// Uses `Arc<RwLock<>>` for lock-free reads when possible and exclusive writes only when
/// necessary. Concurrent readers do not block each other.
///
/// # Resource Lifecycle
///
/// Resources are created lazily on first access and remain in memory. There is currently
/// no automatic cleanup; consider implementing time-based eviction for long-running servers.
///
/// # Examples
///
/// ```ignore
/// use braid_axum_http::server::ResourceStateManager;
///
/// let manager = ResourceStateManager::new();
/// let _ = manager.apply_update("doc1", "hello", "session-1")?;
/// let state = manager.get_resource_state("doc1");
/// assert!(state.is_some());
/// ```
pub struct ResourceStateManager {
    /// Resource ID → Arc<RwLock<ResourceState>>
    /// Using Arc allows multiple concurrent tasks to reference the same resource
    resources: Arc<RwLock<HashMap<String, Arc<RwLock<ResourceState>>>>>,
}

impl ResourceStateManager {
    /// Create a new empty resource manager.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use braid_axum_http::server::ResourceStateManager;
    ///
    /// let manager = ResourceStateManager::new();
    /// ```
    pub fn new() -> Self {
        Self {
            resources: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ========== Resource Lifecycle ==========

    /// Get or create a resource, initializing its CRDT if needed.
    ///
    /// If the resource doesn't exist, it's created with a new CRDT using the provided
    /// agent ID. This agent ID is only used during initialization; future edits from
    /// any agent are merged into the same CRDT.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Unique resource identifier (e.g., document UUID)
    /// * `initial_agent_id` - Agent ID for the first CRDT initialization (if new)
    ///
    /// # Returns
    ///
    /// An Arc-wrapped RwLock to the resource state, allowing safe concurrent access.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let manager = ResourceStateManager::new();
    /// let resource = manager.get_or_create_resource("doc1", "session-1");
    /// // Now resource can be accessed and modified
    /// ```
    pub fn get_or_create_resource(
        &self,
        resource_id: &str,
        initial_agent_id: &str,
    ) -> Arc<RwLock<ResourceState>> {
        let mut resources = self.resources.write();

        resources
            .entry(resource_id.to_string())
            .or_insert_with(|| {
                Arc::new(RwLock::new(ResourceState {
                    crdt: DiamondCRDT::new(initial_agent_id),
                    last_sync: SystemTime::now(),
                }))
            })
            .clone()
    }

    /// Get an existing resource without creating it.
    ///
    /// Returns `None` if the resource hasn't been created yet.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Unique resource identifier
    ///
    /// # Returns
    ///
    /// `Some(Arc<RwLock<ResourceState>>)` if found, `None` otherwise.
    pub fn get_resource(&self, resource_id: &str) -> Option<Arc<RwLock<ResourceState>>> {
        let resources = self.resources.read();
        resources.get(resource_id).cloned()
    }

    /// List all resource IDs currently in memory.
    ///
    /// # Returns
    ///
    /// A vector of resource IDs in arbitrary order.
    pub fn list_resources(&self) -> Vec<String> {
        let resources = self.resources.read();
        resources.keys().cloned().collect()
    }

    // ========== Edit Operations ==========

    /// Apply a full document update (replacement).
    ///
    /// Inserts text at position 0. This is typically used for initial document loads
    /// or complete replacements. The agent ID identifies the origin of this edit.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to update
    /// * `content` - Content to insert
    /// * `agent_id` - Origin agent (for operation log tracking)
    ///
    /// # Returns
    ///
    /// Current resource state exported as JSON, or error if the operation failed.
    /// 
    /// # Note
    ///
    /// Errors currently never occur (Result wrapper is for future extensibility).
    pub fn apply_update(
        &self,
        resource_id: &str,
        content: &str,
        agent_id: &str,
    ) -> Result<Value, String> {
        let resource = self.get_or_create_resource(resource_id, agent_id);
        let mut state = resource.write();

        state.crdt.add_insert(0, content);
        state.last_sync = SystemTime::now();

        Ok(state.crdt.export_operations())
    }

    /// Apply a remote insertion operation.
    ///
    /// Merges a text insertion from a peer into the resource's CRDT.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to modify
    /// * `agent_id` - Peer's agent ID
    /// * `pos` - Position to insert at
    /// * `text` - Text to insert
    ///
    /// # Returns
    ///
    /// Current resource state after merge.
    pub fn apply_remote_insert(
        &self,
        resource_id: &str,
        agent_id: &str,
        pos: usize,
        text: &str,
    ) -> Result<Value, String> {
        let resource = self.get_or_create_resource(resource_id, agent_id);
        let mut state = resource.write();

        state.crdt.add_insert_remote(agent_id, pos, text);
        state.last_sync = SystemTime::now();

        Ok(state.crdt.export_operations())
    }

    /// Apply a remote deletion operation.
    ///
    /// Merges a deletion from a peer into the resource's CRDT.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to modify
    /// * `agent_id` - Peer's agent ID
    /// * `start` - Delete range start (inclusive)
    /// * `end` - Delete range end (exclusive)
    ///
    /// # Returns
    ///
    /// Current resource state after merge.
    pub fn apply_remote_delete(
        &self,
        resource_id: &str,
        agent_id: &str,
        start: usize,
        end: usize,
    ) -> Result<Value, String> {
        let resource = self.get_or_create_resource(resource_id, agent_id);
        let mut state = resource.write();

        state.crdt.add_delete_remote(agent_id, start..end);
        state.last_sync = SystemTime::now();

        Ok(state.crdt.export_operations())
    }

    // ========== Query Methods ==========

    /// Get a snapshot of a resource's current state.
    ///
    /// Returns a JSON checkpoint containing content, version, agent ID, and operation count.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to query
    ///
    /// # Returns
    ///
    /// `Some(Value)` if the resource exists, `None` otherwise.
    pub fn get_resource_state(&self, resource_id: &str) -> Option<Value> {
        self.get_resource(resource_id).map(|resource| {
            let state = resource.read();
            state.crdt.checkpoint()
        })
    }

    /// Get the merge quality score for a resource.
    ///
    /// Returns a heuristic score (0-100) indicating how well concurrent edits have converged.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to query
    ///
    /// # Returns
    ///
    /// `Some(u32)` if the resource exists, `None` otherwise.
    pub fn get_merge_quality(&self, resource_id: &str) -> Option<u32> {
        self.get_resource(resource_id).map(|resource| {
            let state = resource.read();
            state.crdt.merge_quality()
        })
    }
}

impl Clone for ResourceStateManager {
    /// Clone a reference to the same resource registry.
    ///
    /// Cloning creates a new handle to the same underlying resources. Multiple
    /// clones can be distributed to different tasks and will all see the same
    /// document state.
    fn clone(&self) -> Self {
        Self {
            resources: Arc::clone(&self.resources),
        }
    }
}

impl Default for ResourceStateManager {
    /// Create a new resource manager (equivalent to `new()`)
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_resource() {
        let manager = ResourceStateManager::new();
        let resource = manager.get_or_create_resource("doc1", "alice");
        assert!(resource.read().crdt.is_empty());
    }

    #[test]
    fn test_get_nonexistent_resource() {
        let manager = ResourceStateManager::new();
        let resource = manager.get_resource("nonexistent");
        assert!(resource.is_none());
    }

    #[test]
    fn test_apply_update() {
        let manager = ResourceStateManager::new();
        let result = manager.apply_update("doc1", "hello", "alice");
        assert!(result.is_ok());

        let state = manager.get_resource_state("doc1");
        assert!(state.is_some());
        assert_eq!(state.unwrap()["content"], "hello");
    }

    #[test]
    fn test_concurrent_updates() {
        let manager = ResourceStateManager::new();
        let _ = manager.apply_remote_insert("doc1", "alice", 0, "hello");
        let _ = manager.apply_remote_insert("doc1", "bob", 5, " world");

        let state = manager.get_resource_state("doc1");
        assert!(state.is_some());
        assert_eq!(state.unwrap()["content"], "hello world");
    }

    #[test]
    fn test_merge_quality() {
        let manager = ResourceStateManager::new();
        let quality = manager.get_merge_quality("doc1");
        assert!(quality.is_none());

        let _ = manager.apply_update("doc1", "text", "alice");
        let quality = manager.get_merge_quality("doc1");
        assert_eq!(quality, Some(100));
    }

    #[test]
    fn test_list_resources() {
        let manager = ResourceStateManager::new();
        let _ = manager.apply_update("doc1", "text", "alice");
        let _ = manager.apply_update("doc2", "text", "bob");

        let resources = manager.list_resources();
        assert_eq!(resources.len(), 2);
    }

    #[test]
    fn test_clone_shares_state() {
        let manager1 = ResourceStateManager::new();
        let _ = manager1.apply_update("doc1", "original", "alice");

        let manager2 = manager1.clone();
        let state = manager2.get_resource_state("doc1");
        assert_eq!(state.unwrap()["content"], "original");
    }
}
