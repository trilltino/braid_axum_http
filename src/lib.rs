#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

//! # Braid-HTTP: Synchronization for HTTP
//!
//! This crate implements Braid-HTTP, a set of extensions that generalize HTTP from a state
//! *transfer* protocol into a full state *synchronization* protocol.
//!
//! Based on [draft-toomim-httpbis-braid-http-04](https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http)
//!
//! ## Overview
//!
//! Braid is composed of four independent extensions to HTTP:
//!
//! 1. **Versioning** - Resource history with a directed acyclic graph (DAG) of versions
//! 2. **Updates as Patches** - Send and receive updates as patches or snapshots
//! 3. **Subscriptions** - Long-lived subscriptions to receive updates over time
//! 4. **Merge-Types** - Specify OT (Operational Transform) or CRDT behavior for conflict resolution
//!
//! Together, these extensions enable multiple clients and servers to make simultaneous mutations
//! to arbitrary resources under arbitrary network conditions, while guaranteeing consistency.
//!
//! ## Key Features
//!
//! - **Version DAG Support**: Track resource history as a directed acyclic graph with multiple parents
//! - **Patch-based Updates**: Efficient incremental updates with Content-Range specifications
//! - **Subscriptions**: Server-push updates via HTTP 209 status code
//! - **Merge-Type Specification**: Declare conflict resolution strategies (OT, CRDT, etc.)
//! - **Current-Version Header**: Subscription catch-up signaling
//! - **Multi-Patch Responses**: Send multiple patches in a single response with individual length headers
//! - **HTTP Status Codes**: 
//!   - `200 OK` - Standard response
//!   - `206 Partial Content` - Range-based patches
//!   - `209 Subscription` - Subscription update
//!   - `293 Merge Conflict` - Version conflicts detected
//!   - `410 Gone` - History dropped, client must restart
//!   - `416 Range Not Satisfiable` - Invalid range request
//!
//! ## Client Usage
//!
//! ```ignore
//! use braid_axum_http::{BraidClient, BraidRequest, Version};
//! use futures::stream::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = BraidClient::new();
//!
//!     // Subscribe to updates
//!     let request = BraidRequest::new().subscribe();
//!     let mut subscription = client.subscribe("http://localhost/resource", request).await?;
//!
//!     // Receive updates
//!     while let Some(result) = subscription.next().await {
//!         match result {
//!             Ok(update) => println!("Version: {:?}, Body: {:?}", update.version, update.body),
//!             Err(e) => eprintln!("Error: {}", e),
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Server Usage
//!
//! ```ignore
//! use axum::{
//!     routing::get,
//!     Router,
//!     extract::Extension,
//! };
//! use braid_axum_http::{BraidLayer, BraidState, Update, Version, DiamondCRDT};
//! use std::sync::Arc;
//!
//! async fn get_resource(
//!     Extension(braid): Extension<Arc<BraidState>>,
//! ) -> Update {
//!     Update::snapshot(Version::new("v1"), "{\"data\": \"value\"}")
//!         .with_merge_type("sync9")
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let app = Router::new()
//!         .route("/resource", get(get_resource))
//!         .layer(BraidLayer::new().middleware());
//!
//!     let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
//!     axum::serve(listener, app).await.unwrap();
//! }
//! ```
//!
//! ## Collaborative Text Editing with Diamond-Types CRDT
//!
//! ```ignore
//! use braid_axum_http::{Update, Version, DiamondCRDT};
//!
//! // Create a CRDT for collaborative text editing
//! let mut crdt = DiamondCRDT::new("editor-session-id");
//!
//! // Make local edits
//! crdt.add_insert(0, "hello");
//! crdt.add_insert(5, " world");
//!
//! // Merge remote edits automatically
//! crdt.add_insert_remote("other-client", 0, ">> ");
//!
//! // Send to peers via Braid-HTTP with Diamond merge type
//! let update = Update::snapshot(
//!     Version::new("v1"),
//!     crdt.content()
//! ).with_merge_type("diamond");
//!
//! assert_eq!(crdt.content(), ">> hello world");
//! ```
//!
//! ## Module Structure
//!
//! - **[types]** - Core Braid protocol types (Version, Update, Patch, etc.)
//! - **[error]** - Error types and result handling
//! - **[client]** - HTTP client with Braid support
//! - **[server]** - HTTP server (Axum) integration
//! - **[merge]** - Merge algorithms and CRDTs (Diamond-Types integration)
//! - **[protocol]** - Protocol constants, header parsing, and status codes

pub mod client;
pub mod error;
pub mod types;
pub mod server;
pub mod merge;
pub mod protocol;

pub use client::BraidClient;
pub use error::{BraidError, Result};
pub use types::{BraidRequest, BraidResponse, Update, Patch, Version, ContentRange};
pub use server::{BraidLayer, BraidState, ServerConfig};
pub use merge::DiamondCRDT;

#[cfg(test)]
mod tests;
