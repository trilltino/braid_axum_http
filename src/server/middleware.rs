//! Axum middleware for Braid protocol support.
//!
//! Provides an Axum layer that extracts Braid protocol information from incoming
//! requests and makes it available to handlers.
//!
//! # Usage
//!
//! ```ignore
//! use axum::{Router, routing::get};
//! use braid_axum_http::BraidLayer;
//!
//! let app = Router::new()
//!     .route("/resource", get(handler))
//!     .layer(BraidLayer::new().middleware());
//! ```
//!
//! # How It Works
//!
//! The middleware:
//! 1. Extracts Braid headers from incoming requests (Version, Parents, Subscribe, etc.)
//! 2. Parses them into structured Rust types
//! 3. Attaches the `BraidState` to request extensions
//! 4. Handlers can extract `BraidState` to access the information
//!
//! # Specification
//!
//! See draft-toomim-httpbis-braid-http sections 2, 3, and 4 for protocol details.

use crate::protocol::{self, constants::headers};
use crate::types::Version;
use axum::{
    middleware::Next,
    response::Response,
    extract::Request,
};
use std::sync::Arc;
use std::collections::BTreeMap;
use super::resource_state::ResourceStateManager;

/// Braid protocol state extracted from HTTP request headers.
///
/// This immutable state contains all parsed Braid protocol information from the incoming request.
/// It's automatically extracted by the Axum middleware and made available to handlers via
/// request extensions.
///
/// # Fields
///
/// - **subscribe**: Client requested subscription (HTTP 209 streaming)
/// - **version**: Requested version ID(s) from `Version` header (Section 2.3)
/// - **parents**: Parent version(s) from `Parents` header for history (Section 2.4)
/// - **peer**: Peer identifier from `Peer` header (for idempotent updates)
/// - **heartbeat**: Desired heartbeat interval in seconds (Section 4.1)
/// - **merge_type**: Requested conflict resolution strategy (Section 2.2)
/// - **content_range**: Range specification for patch operations (Section 3)
/// - **headers**: Complete set of HTTP headers (keys normalized to lowercase)
///
/// # Examples
///
/// ```ignore
/// use axum::extract::Extension;
/// use braid_axum_http::BraidState;
/// use std::sync::Arc;
///
/// async fn handle_resource(
///     Extension(braid): Extension<Arc<BraidState>>,
/// ) -> String {
///     if let Some(versions) = &braid.version {
///         format!("Client requested version(s): {:?}", versions)
///     } else {
///         "Client requested current version".to_string()
///     }
/// }
/// ```
///
/// # Specification
///
/// All fields correspond to Braid-HTTP draft (draft-toomim-httpbis-braid-http).
/// See the specification for header format and semantics details.
#[derive(Clone, Debug)]
pub struct BraidState {
    /// Whether client explicitly requested subscription via `Subscribe: true`
    pub subscribe: bool,

    /// Parsed `Version` header (list of requested versions or None for current)
    pub version: Option<Vec<Version>>,

    /// Parsed `Parents` header (for retrieving history between versions)
    pub parents: Option<Vec<Version>>,

    /// `Peer` identifier for detecting duplicate requests from the same client
    pub peer: Option<String>,

    /// Heartbeat interval in seconds from `Heartbeats` header
    pub heartbeat: Option<u64>,

    /// Merge type strategy (e.g., "diamond", "sync9", "ot") for conflict resolution
    pub merge_type: Option<String>,

    /// `Content-Range` specification for partial content requests
    pub content_range: Option<String>,

    /// Complete HTTP headers map (all keys normalized to lowercase)
    pub headers: BTreeMap<String, String>,
}

impl BraidState {
    /// Parse and create BraidState from HTTP request headers.
    ///
    /// Extracts all recognized Braid protocol headers and stores both parsed
    /// values and the raw header map for inspection.
    #[must_use]
    pub fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        let mut braid_state = BraidState {
            subscribe: false,
            version: None,
            parents: None,
            peer: None,
            heartbeat: None,
            merge_type: None,
            content_range: None,
            headers: BTreeMap::new(),
        };

        for (name, value) in headers.iter() {
            if let Ok(value_str) = value.to_str() {
                let name_lower = name.to_string().to_lowercase();
                braid_state.headers.insert(name_lower.clone(), value_str.to_string());

                if name_lower == headers::SUBSCRIBE.as_str() {
                    braid_state.subscribe = value_str.to_lowercase() == "true";
                } else if name_lower == headers::VERSION.as_str() {
                    braid_state.version = protocol::parse_version_header(value_str).ok();
                } else if name_lower == headers::PARENTS.as_str() {
                    braid_state.parents = protocol::parse_version_header(value_str).ok();
                } else if name_lower == headers::PEER.as_str() {
                    braid_state.peer = Some(value_str.to_string());
                } else if name_lower == headers::HEARTBEATS.as_str() {
                    braid_state.heartbeat = protocol::parse_heartbeat(value_str).ok();
                } else if name_lower == headers::MERGE_TYPE.as_str() {
                    braid_state.merge_type = Some(value_str.to_string());
                } else if name_lower == headers::CONTENT_RANGE.as_str() {
                    braid_state.content_range = Some(value_str.to_string());
                }
            }
        }

        braid_state
    }
}



/// Axum middleware layer for Braid protocol support.
///
/// `BraidLayer` processes incoming requests, extracts Braid protocol information,
/// and makes it available to handlers via request extensions. It also manages
/// per-resource CRDT state for collaborative editing features.
///
/// # Features
///
/// - Extracts Braid protocol headers from requests
/// - Manages collaborative document state via Diamond-Types CRDT
/// - Configurable subscription limits and heartbeat intervals
/// - Thread-safe, shareable across async tasks
///
/// # Configuration
///
/// The layer can be customized via `ServerConfig`:
///
/// ```ignore
/// use braid_axum_http::ServerConfig;
///
/// let config = ServerConfig {
///     enable_subscriptions: true,
///     max_subscriptions: 1000,
///     heartbeat_interval: 30,
///     enable_multiplex: false,
/// };
/// ```
///
/// # Usage
///
/// ```ignore
/// use axum::Router;
/// use braid_axum_http::BraidLayer;
///
/// let app = Router::new()
///     .route("/resource", get(handler))
///     .layer(BraidLayer::new().middleware());
/// ```
///
/// # Handlers
///
/// Handlers can extract the Braid state and resource manager:
///
/// ```ignore
/// use axum::extract::Extension;
/// use braid_axum_http::BraidState;
/// use std::sync::Arc;
///
/// async fn handler(
///     Extension(braid): Extension<Arc<BraidState>>,
/// ) -> Response {
///     // Use braid.version, braid.merge_type, etc.
/// }
/// ```
///
/// # Specification
///
/// Implements draft-toomim-httpbis-braid-http sections 2-4.
#[derive(Clone)]
pub struct BraidLayer {
    /// Server configuration for subscriptions, heartbeats, and limits
    config: super::config::ServerConfig,

    /// Shared resource state manager (CRDT instances per resource)
    pub resource_manager: Arc<ResourceStateManager>,
}

impl BraidLayer {
    /// Create a new Braid layer with default configuration.
    ///
    /// Uses reasonable defaults:
    /// - Subscriptions enabled
    /// - 1000 concurrent subscriptions
    /// - 30-second heartbeat
    /// - No multiplexing
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use braid_axum_http::BraidLayer;
    ///
    /// let layer = BraidLayer::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: super::config::ServerConfig::default(),
            resource_manager: Arc::new(ResourceStateManager::new()),
        }
    }

    /// Create a new Braid layer with custom configuration.
    ///
    /// Allows fine-tuning subscription limits, heartbeat intervals,
    /// and other server behavior.
    ///
    /// # Arguments
    ///
    /// * `config` - Custom server configuration
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use braid_axum_http::{BraidLayer, ServerConfig};
    ///
    /// let config = ServerConfig {
    ///     enable_subscriptions: true,
    ///     max_subscriptions: 500,
    ///     ..Default::default()
    /// };
    /// let layer = BraidLayer::with_config(config);
    /// ```
    #[must_use]
    pub fn with_config(config: super::config::ServerConfig) -> Self {
        Self {
            config,
            resource_manager: Arc::new(ResourceStateManager::new()),
        }
    }

    /// Get a reference to the layer's configuration.
    ///
    /// # Returns
    ///
    /// A reference to the server configuration used by this layer.
    #[inline]
    #[must_use]
    pub fn config(&self) -> &super::config::ServerConfig {
        &self.config
    }

    /// Create the middleware function for use with Axum.
    ///
    /// Returns a middleware function that extracts Braid protocol information
    /// from request headers and attaches it (and the resource manager) to
    /// request extensions.
    ///
    /// # Returns
    ///
    /// A middleware function compatible with `Router::layer()`.
    #[must_use]
    pub fn middleware(
        &self,
    ) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
             + Send
             + Sync
             + Clone {
        let resource_manager = self.resource_manager.clone();

        move |mut req: Request, next: Next| {
            let resource_manager = resource_manager.clone();
            Box::pin(async move {
                let braid_state = BraidState::from_headers(req.headers());
                req.extensions_mut().insert(Arc::new(braid_state));
                req.extensions_mut().insert(resource_manager);
                next.run(req).await
            })
        }
    }
}

impl Default for BraidLayer {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_header() {
        let result = protocol::parse_version_header("\"v1\", \"v2\", \"v3\"");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn test_parse_heartbeat() {
        assert!(protocol::parse_heartbeat("30s").is_ok());
        assert!(protocol::parse_heartbeat("30").is_ok());
        if let Ok(val) = protocol::parse_heartbeat("30s") {
            assert_eq!(val, 30);
        }
    }
}
