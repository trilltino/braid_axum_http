//! Braid HTTP client implementation.
//!
//! This module provides a complete HTTP client with Braid protocol support, enabling
//! clients to:
//!
//! - **Track resource versions** and history as a DAG
//! - **Send and receive patches** (incremental updates)
//! - **Subscribe to streaming updates** via HTTP 209
//! - **Handle version conflicts** with merge types
//! - **Automatically retry** failed requests with exponential backoff
//!
//! # Module Organization
//!
//! ```text
//! client/
//! ├── fetch        - BraidClient and HTTP operations
//! ├── headers      - Braid-specific header encoding/decoding
//! ├── parser       - Streaming message parser
//! ├── subscription - Long-lived subscription handling
//! ├── config       - Client configuration
//! └── utils        - Utility functions
//! ```
//!
//! # Key Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`BraidClient`] | Main HTTP client with Braid support |
//! | [`BraidHeaders`] | Braid protocol headers builder |
//! | [`MessageParser`] | Streaming protocol message parser |
//! | [`Subscription`] | Long-lived update subscription |
//! | [`ClientConfig`] | Client configuration options |
//!
//! # Examples
//!
//! ## Creating a Client
//!
//! ```
//! use braid_axum_http::client::{BraidClient, ClientConfig};
//!
//! // Default configuration
//! let client = BraidClient::new();
//!
//! // Custom configuration
//! let config = ClientConfig {
//!     max_retries: 5,
//!     retry_delay_ms: 2000,
//!     ..Default::default()
//! };
//! let client = BraidClient::with_config(config);
//! ```
//!
//! ## Building Headers
//!
//! ```
//! use braid_axum_http::client::BraidHeaders;
//! use braid_axum_http::Version;
//!
//! let headers = BraidHeaders::new()
//!     .with_version(Version::new("v1"))
//!     .with_parent(Version::new("v0"))
//!     .with_subscribe()
//!     .with_merge_type("diamond");
//! ```
//!
//! ## Parsing Messages
//!
//! ```
//! use braid_axum_http::client::{MessageParser, ParseState};
//!
//! let mut parser = MessageParser::new();
//! assert_eq!(parser.state(), ParseState::WaitingForHeaders);
//!
//! // Feed data incrementally
//! let data = b"Content-Length: 5\r\n\r\nHello";
//! let messages = parser.feed(data).unwrap();
//! assert_eq!(messages.len(), 1);
//! ```
//!
//! ## Utility Functions
//!
//! ```
//! use braid_axum_http::client::{
//!     exponential_backoff, is_retryable_status,
//! };
//! use std::time::Duration;
//!
//! // Check if status is retryable
//! assert!(is_retryable_status(503));
//! assert!(!is_retryable_status(404));
//!
//! // Calculate backoff delay
//! let delay = exponential_backoff(2, 100);
//! assert_eq!(delay, Duration::from_millis(400));
//! ```
//!
//! # Specification
//!
//! Based on [draft-toomim-httpbis-braid-http-04]:
//!
//! - **Section 2**: Versioning with DAG support
//! - **Section 3**: Updates as patches or snapshots
//! - **Section 4**: Subscriptions with HTTP 209 status
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

mod config;
mod fetch;
mod headers;
mod parser;
mod subscription;
mod utils;

pub use config::ClientConfig;
pub use fetch::BraidClient;
pub use headers::{BraidHeaders, HeaderParser};
pub use parser::{Message, MessageParser, ParseState};
pub use subscription::{Subscription, SubscriptionStream};
pub use utils::*;
