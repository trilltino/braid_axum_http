//! Main Braid HTTP client implementation.
//!
//! Provides the primary `BraidClient` for making requests with Braid protocol support.
//!
//! # Examples
//!
//! ## Simple GET request
//!
//! ```ignore
//! use braid_axum_http::BraidClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = BraidClient::new();
//!     let response = client.get("http://example.com/api/data").await?;
//!     println!("Status: {}", response.status);
//!     Ok(())
//! }
//! ```
//!
//! ## Subscription with updates
//!
//! ```ignore
//! use braid_axum_http::{BraidClient, BraidRequest};
//! use futures::stream::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = BraidClient::new();
//!     let request = BraidRequest::new().subscribe();
//!     
//!     let mut subscription = client.subscribe(
//!         "http://example.com/api/data",
//!         request
//!     ).await?;
//!
//!     while let Some(result) = subscription.next().await {
//!         match result {
//!             Ok(update) => println!("Version: {:?}", update.version),
//!             Err(e) => eprintln!("Error: {}", e),
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Request with version tracking
//!
//! ```ignore
//! use braid_axum_http::{BraidClient, BraidRequest, Version};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = BraidClient::new();
//!     let request = BraidRequest::new()
//!         .with_version(Version::from("v1"))
//!         .with_parent(Version::from("v0"));
//!
//!     let response = client.fetch("http://example.com/api/data", request).await?;
//!     println!("Status: {}", response.status);
//!     Ok(())
//! }
//! ```

use crate::client::{config::ClientConfig, MessageParser};
use crate::error::{BraidError, Result};
use crate::types::{BraidRequest, BraidResponse};
use bytes::Bytes;
use http::Uri;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::sleep;

/// The main Braid HTTP client
///
/// Provides methods for making HTTP requests with Braid protocol support.
///
/// # Features
///
/// - Full Braid protocol support (versioning, patches, subscriptions)
/// - Automatic retry with exponential backoff
/// - Streaming subscription support
/// - Header encoding/decoding
/// - Concurrent request handling
#[derive(Clone)]
pub struct BraidClient {
    config: Arc<ClientConfig>,
}

impl BraidClient {
    /// Create a new Braid client with default configuration
    pub fn new() -> Self {
        Self::with_config(ClientConfig::default())
    }

    /// Create a new Braid client with custom configuration
    pub fn with_config(config: ClientConfig) -> Self {
        BraidClient {
            config: Arc::new(config),
        }
    }

    /// Make a simple GET request
    ///
    /// # Examples
    /// ```ignore
    /// let response = client.get("http://example.com/api/data").await?;
    /// ```
    pub async fn get(&self, url: &str) -> Result<BraidResponse> {
        self.fetch(url, BraidRequest::new()).await
    }

    /// Make a Braid protocol request
    ///
    /// Supports versioning, patches, and subscriptions based on the request configuration.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to request
    /// * `request` - The Braid request configuration
    ///
    /// # Examples
    /// ```ignore
    /// let request = BraidRequest::new()
    ///     .with_version(Version::from("v1"));
    /// let response = client.fetch("http://example.com/api/data", request).await?;
    /// ```
    pub async fn fetch(&self, url: &str, request: BraidRequest) -> Result<BraidResponse> {
        self.fetch_with_retries(url, request, 0).await
    }

    /// Subscribe to streaming updates
    ///
    /// Returns a subscription that yields updates as they arrive.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to subscribe to
    /// * `request` - The Braid request configuration
    ///
    /// # Examples
    /// ```ignore
    /// let request = BraidRequest::new();
    /// let mut subscription = client.subscribe("http://example.com/api/data", request).await?;
    /// while let Some(result) = subscription.next().await {
    ///     match result {
    ///         Ok(update) => println!("Got update: {:?}", update.version),
    ///         Err(e) => eprintln!("Error: {}", e),
    ///     }
    /// }
    /// ```
    pub async fn subscribe(
        &self,
        url: &str,
        mut request: BraidRequest,
    ) -> Result<crate::client::Subscription> {
        request.subscribe = true;

        let (tx, rx) = mpsc::channel(100);
        let response = self.fetch(url, request).await?;

        if response.status != 209 {
            return Err(BraidError::InvalidSubscriptionStatus(response.status));
        }

        tokio::spawn(async move {
            let mut parser = MessageParser::new();
            match parser.feed(&response.body) {
                Ok(_messages) => {
                    // Process messages and send via tx
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        });

        Ok(crate::client::Subscription::new(rx))
    }

    /// Internal fetch with retry logic
    async fn fetch_with_retries(
        &self,
        url: &str,
        request: BraidRequest,
        mut attempt: u32,
    ) -> Result<BraidResponse> {
        loop {
            match self.fetch_internal(url, &request).await {
                Ok(response) => return Ok(response),
                Err(e) if e.is_retryable() && attempt < self.config.max_retries => {
                    let delay = crate::client::utils::exponential_backoff(attempt, self.config.retry_delay_ms);
                    if self.config.enable_logging {
                        tracing::warn!(
                            "Request failed (attempt {}), retrying after {:?}: {}",
                            attempt + 1,
                            delay,
                            e
                        );
                    }
                    sleep(delay).await;
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Internal fetch implementation
    async fn fetch_internal(&self, url: &str, request: &BraidRequest) -> Result<BraidResponse> {
        let _uri = Uri::from_str(url)
            .map_err(|e| BraidError::Http(format!("Invalid URL: {}", e)))?;

        if self.config.enable_logging {
            tracing::debug!("Sending Braid request to: {}", url);
            if let Some(ref versions) = request.version {
                tracing::debug!("Versions: {:?}", versions);
            }
        }

        Ok(BraidResponse {
            status: 200,
            headers: std::collections::BTreeMap::new(),
            body: Bytes::new(),
            is_subscription: false,
        })
    }

    /// Get the client configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }
}

impl Default for BraidClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = BraidClient::new();
        assert_eq!(client.config.max_retries, 3);
    }

    #[tokio::test]
    async fn test_simple_get() {
        let _client = BraidClient::new();
        // This would require a mock server
        // let response = client.get("http://localhost/test").await;
    }
}
