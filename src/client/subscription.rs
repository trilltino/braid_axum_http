//! Subscription handling for Braid protocol.
//!
//! This module provides types and implementations for managing long-lived
//! subscriptions that receive streamed updates from Braid-HTTP servers.
//!
//! # Overview
//!
//! Subscriptions enable real-time updates using HTTP 209 status code responses.
//! The server keeps the connection open and streams updates as they occur,
//! allowing clients to receive changes immediately without polling.
//!
//! # Types
//!
//! - **Subscription**: Simple subscription type with async `next()` method
//! - **SubscriptionStream**: Implements `Stream` trait for use with `StreamExt`
//!
//! # Examples
//!
//! ## Using Subscription
//!
//! ```ignore
//! use braid_axum_http::{BraidClient, BraidRequest};
//!
//! let client = BraidClient::new();
//! let request = BraidRequest::new().subscribe();
//! let mut subscription = client.subscribe("http://example.com/resource", request).await?;
//!
//! while let Some(result) = subscription.next().await {
//!     match result {
//!         Ok(update) => println!("Received update: {:?}", update.version),
//!         Err(e) => eprintln!("Subscription error: {}", e),
//!     }
//! }
//! ```
//!
//! ## Using SubscriptionStream
//!
//! ```ignore
//! use braid_axum_http::{BraidClient, BraidRequest};
//! use futures::stream::StreamExt;
//!
//! let client = BraidClient::new();
//! let request = BraidRequest::new().subscribe();
//! let mut stream = client.subscribe("http://example.com/resource", request).await?;
//!
//! while let Some(result) = stream.next().await {
//!     // Handle update
//! }
//! ```
//!
//! # Specification
//!
//! See Section 4 of draft-toomim-httpbis-braid-http for subscription details.

use crate::error::Result;
use crate::types::Update;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// A subscription to Braid-HTTP updates.
///
/// Represents a long-lived connection that receives streaming updates
/// from a Braid-HTTP server. Updates are delivered asynchronously
/// through the `next()` method.
///
/// # Lifecycle
///
/// 1. Created via `BraidClient::subscribe()`
/// 2. Receives updates through `next().await`
/// 3. Automatically closed when the server connection terminates
///
/// # Error Handling
///
/// The subscription may return errors if:
/// - The connection is lost
/// - The server sends invalid data
/// - Protocol violations occur
///
/// Clients should handle errors appropriately and may need to
/// re-establish the subscription.
pub struct Subscription {
    receiver: mpsc::Receiver<Result<Update>>,
}

impl Subscription {
    /// Create a new subscription from a receiver channel.
    ///
    /// This is typically called internally by `BraidClient::subscribe()`.
    /// Users should not need to call this directly.
    ///
    /// # Arguments
    ///
    /// * `receiver` - An MPSC receiver channel that will receive updates
    pub fn new(receiver: mpsc::Receiver<Result<Update>>) -> Self {
        Subscription { receiver }
    }

    /// Receive the next update from the subscription.
    ///
    /// Returns `None` when the subscription is closed (either by the server
    /// or by dropping the sender). Returns `Some(Ok(update))` when an update
    /// is received, or `Some(Err(error))` when an error occurs.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// while let Some(result) = subscription.next().await {
    ///     match result {
    ///         Ok(update) => {
    ///             println!("Received update version: {:?}", update.version);
    ///             // Process the update
    ///         },
    ///         Err(e) => {
    ///             eprintln!("Subscription error: {}", e);
    ///             // Handle error, possibly re-subscribe
    ///             break;
    ///         },
    ///     }
    /// }
    /// ```
    ///
    /// # Returns
    ///
    /// - `Some(Ok(Update))` - An update was successfully received
    /// - `Some(Err(BraidError))` - An error occurred receiving the update
    /// - `None` - The subscription has been closed
    pub async fn next(&mut self) -> Option<Result<Update>> {
        self.receiver.recv().await
    }
}

impl Stream for Subscription {
    type Item = Result<Update>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// A streaming subscription that implements the `Stream` trait.
///
/// This type wraps a subscription in a `ReceiverStream`, allowing it to be
/// used with `StreamExt` combinators like `map`, `filter`, `take`, etc.
///
/// # Usage with StreamExt
///
/// ```ignore
/// use braid_axum_http::{BraidClient, BraidRequest};
/// use futures::stream::StreamExt;
///
/// let mut stream = client.subscribe(url, request).await?;
///
/// // Use StreamExt combinators
/// stream
///     .take(10)  // Limit to 10 updates
///     .filter(|result| result.is_ok())  // Filter errors
///     .for_each(|result| async {
///         // Process each update
///     })
///     .await;
/// ```
pub struct SubscriptionStream {
    receiver: ReceiverStream<Result<Update>>,
}

impl SubscriptionStream {
    /// Create a new subscription stream from a receiver channel.
    ///
    /// This is typically called internally by `BraidClient::subscribe()`.
    ///
    /// # Arguments
    ///
    /// * `receiver` - An MPSC receiver channel that will receive updates
    pub fn new(receiver: mpsc::Receiver<Result<Update>>) -> Self {
        SubscriptionStream {
            receiver: ReceiverStream::new(receiver),
        }
    }
}

impl Stream for SubscriptionStream {
    type Item = Result<Update>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscription_creation() {
        let (tx, rx) = mpsc::channel(10);
        let mut subscription = Subscription::new(rx);

        let update = Update::snapshot(
            crate::types::Version::String("v1".to_string()),
            "test",
        );

        tx.send(Ok(update)).await.unwrap();
        let received = subscription.next().await;
        assert!(received.is_some());
    }
}
