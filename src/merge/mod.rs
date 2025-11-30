//! Merge algorithms and CRDT implementations for conflict resolution.
//!
//! This module provides merge strategies for resolving concurrent edits to the same resource
//! when multiple clients make simultaneous mutations. The module includes built-in support
//! for CRDT-based merge algorithms.
//!
//! # Merge-Types in Braid-HTTP
//!
//! Per Section 2.2 of [draft-toomim-httpbis-braid-http-04], resources can declare a merge type
//! to specify how concurrent edits should be reconciled:
//!
//! | Merge Type | Description |
//! |------------|-------------|
//! | `"sync9"` | Simple synchronization (last-write-wins) |
//! | `"diamond"` | Diamond-types CRDT for text documents |
//! | Custom | Application-defined merge algorithms |
//!
//! # Key Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`DiamondCRDT`] | High-performance text CRDT |
//!
//! # Examples
//!
//! ## Basic Text Editing
//!
//! ```
//! use braid_axum_http::merge::DiamondCRDT;
//!
//! let mut doc = DiamondCRDT::new("session-1");
//! doc.add_insert(0, "hello");
//! doc.add_insert(5, " world");
//! assert_eq!(doc.content(), "hello world");
//! ```
//!
//! ## Concurrent Edits
//!
//! ```
//! use braid_axum_http::merge::DiamondCRDT;
//!
//! let mut doc = DiamondCRDT::new("editor-1");
//! doc.add_insert(0, "hello");
//!
//! // Concurrent edit from another client
//! doc.add_insert_remote("editor-2", 5, " world");
//!
//! // Automatically merged without conflict
//! assert_eq!(doc.content(), "hello world");
//! ```
//!
//! ## Delete Operations
//!
//! ```
//! use braid_axum_http::merge::DiamondCRDT;
//!
//! let mut doc = DiamondCRDT::new("session-1");
//! doc.add_insert(0, "hello world");
//! doc.add_delete(5..6);  // Delete the space
//! assert_eq!(doc.content(), "helloworld");
//! ```
//!
//! ## Version Tracking
//!
//! ```
//! use braid_axum_http::merge::DiamondCRDT;
//!
//! let mut doc = DiamondCRDT::new("session-1");
//! let v1 = doc.get_version();
//!
//! doc.add_insert(0, "text");
//! let v2 = doc.get_version();
//!
//! assert_ne!(v1, v2);
//! ```
//!
//! ## Export and Checkpoint
//!
//! ```
//! use braid_axum_http::merge::DiamondCRDT;
//!
//! let mut doc = DiamondCRDT::new("session-1");
//! doc.add_insert(0, "hello");
//!
//! // Export operations as JSON
//! let export = doc.export_operations();
//! assert_eq!(export["content"], "hello");
//!
//! // Create a checkpoint
//! let checkpoint = doc.checkpoint();
//! assert_eq!(checkpoint["content"], "hello");
//! ```
//!
//! # Specification
//!
//! - [draft-toomim-httpbis-braid-http-04] Section 2.2 (Merge-Types)
//! - [Diamond-Types](https://docs.rs/diamond-types/) - High-performance CRDT for text
//! - [CRDT Overview](https://crdt.tech/) - Conflict-free Replicated Data Types
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

pub mod diamond;

pub use diamond::DiamondCRDT;
