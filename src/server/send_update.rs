//! Send update response implementation for Braid protocol.
//!
//! This module provides utilities for sending Braid protocol updates to clients
//! over HTTP responses, handling version headers, merge types, patches, and
//! subscription status codes.
//!
//! # Response Format
//!
//! Braid updates are sent as HTTP responses with specific headers:
//! - **Version**: The version ID(s) of the update
//! - **Parents**: The parent version ID(s) in the DAG
//! - **Current-Version**: The latest version (for catch-up signaling)
//! - **Merge-Type**: The conflict resolution strategy
//! - **Content-Range**: Range specification for patches
//! - **Content-Length**: Length of response body
//!
//! # Status Codes
//!
//! - `200 OK` - Standard update response
//! - `206 Partial Content` - Range-based patches
//! - `209 Subscription` - Subscription update (Section 4)
//! - `293 Merge Conflict` - Conflict detected
//! - `410 Gone` - History dropped
//! - `416 Range Not Satisfiable` - Invalid range
//!
//! # Specification
//!
//! See Sections 2, 3, and 4 of draft-toomim-httpbis-braid-http.

use crate::error::Result;
use crate::protocol;
use crate::types::{Update, Version};
use axum::{
    response::{IntoResponse, Response},
    body::Body,
    http::{StatusCode, HeaderValue, header},
};
use crate::protocol::constants::headers;
use bytes::Bytes;
use std::collections::BTreeMap;
use futures::{Stream, StreamExt};

/// Extension trait for Axum responses to send Braid updates.
///
/// Provides methods to encode and send Braid protocol updates as HTTP responses.
/// This trait is implemented for types that can produce HTTP responses.
pub trait SendUpdateExt {
    /// Send a Braid update to the client.
    ///
    /// Encodes the update with appropriate headers and status code,
    /// returning it as an HTTP response.
    fn send_update(&mut self, update: &Update) -> Result<()>;

    /// Send raw bytes as response body.
    ///
    /// Sends raw bytes directly in the response body.
    fn send_body(&mut self, body: &[u8]) -> Result<()>;
}

/// Builder for creating update responses.
///
/// Provides a fluent API for constructing Braid protocol responses with
/// appropriate headers and status codes.
///
/// # Examples
///
/// ```ignore
/// use braid_axum_http::server::UpdateResponse;
/// use braid_axum_http::Version;
///
/// let response = UpdateResponse::new(200)
///     .with_version(vec![Version::new("v2")])
///     .with_parents(vec![Version::new("v1")])
///     .with_header("Merge-Type".to_string(), "sync9".to_string())
///     .with_body("{\"data\": \"updated\"}")
///     .build();
/// ```
pub struct UpdateResponse {
    /// HTTP status code
    status: u16,
    /// Response headers
    headers: BTreeMap<String, String>,
    /// Response body
    body: Option<Bytes>,
}

impl UpdateResponse {
    /// Create a new update response builder with the given status code.
    ///
    /// Use HTTP 209 for subscription updates (Section 4).
    pub fn new(status: u16) -> Self {
        UpdateResponse {
            status,
            headers: BTreeMap::new(),
            body: None,
        }
    }

    /// Set version header(s).
    ///
    /// Specifies the version ID(s) of this update (Section 2).
    pub fn with_version(mut self, versions: Vec<Version>) -> Self {
        let version_str = protocol::format_version_header(&versions);
        self.headers.insert(headers::VERSION.as_str().to_string(), version_str);
        self
    }

    /// Set parents header
    pub fn with_parents(mut self, parents: Vec<Version>) -> Self {
        let parents_str = protocol::format_version_header(&parents);
        self.headers.insert(headers::PARENTS.as_str().to_string(), parents_str);
        self
    }

    /// Set body
    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Set custom header
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }

    /// Build the response
    pub fn build(self) -> Response {
        let mut response = match self.status {
            200 => Response::builder().status(StatusCode::OK),
            209 => Response::builder().status(StatusCode::from_u16(209).unwrap()),
            404 => Response::builder().status(StatusCode::NOT_FOUND),
            500 => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR),
            _ => Response::builder().status(StatusCode::from_u16(self.status).unwrap()),
        };

        for (key, value) in &self.headers {
            if let Ok(header_value) = value.parse::<HeaderValue>() {
                response = response.header(key, header_value);
            }
        }

        if let Some(body) = self.body {
            response
                .header(header::CONTENT_LENGTH, body.len())
                .body(Body::from(body))
                .unwrap_or_else(|_| Response::default())
        } else {
            response
                .body(Body::empty())
                .unwrap_or_else(|_| Response::default())
        }
    }
}

/// Convert Update to HTTP response
impl IntoResponse for Update {
    fn into_response(self) -> Response {
        let mut response_builder = UpdateResponse::new(self.status);

        if !self.version.is_empty() {
            response_builder = response_builder.with_version(self.version.clone());
        }

        if !self.parents.is_empty() {
            response_builder = response_builder.with_parents(self.parents.clone());
        }

        for (key, value) in &self.extra_headers {
            response_builder = response_builder.with_header(key.clone(), value.clone());
        }

        if let Some(body) = &self.body {
            response_builder = response_builder.with_body(body.clone());
        } else if let Some(patches) = &self.patches {
            let patches_str = patches.len().to_string();
            response_builder = response_builder.with_header(
                headers::PATCHES.as_str().to_string(),
                patches_str,
            );

            if !patches.is_empty() {
                let first_patch = &patches[0];
                let content_range = format!("{} {}", first_patch.unit, first_patch.range);
                response_builder = response_builder.with_header(
                    headers::CONTENT_RANGE.as_str().to_string(),
                    content_range,
                );
                response_builder = response_builder.with_body(first_patch.content.clone());
            }
        }

        response_builder.build()
    }
}

/// HTTP response status codes
pub mod status {
    use axum::http::StatusCode;

    /// 209 Subscription
    #[allow(dead_code)]
    pub const SUBSCRIPTION: u16 = 209;

    /// 293 Responded via Multiplexer
    #[allow(dead_code)]
    pub const RESPONDED_VIA_MULTIPLEX: u16 = 293;

    #[allow(dead_code)]
    pub fn subscription_response() -> StatusCode {
        StatusCode::from_u16(SUBSCRIPTION).unwrap()
    }

    #[allow(dead_code)]
    pub fn multiplex_response() -> StatusCode {
        StatusCode::from_u16(RESPONDED_VIA_MULTIPLEX).unwrap()
    }
}

/// Builder for subscription responses.
///
/// Creates a streaming response with HTTP 209 status code.
pub struct SubscriptionResponse<S> {
    stream: S,
    headers: BTreeMap<String, String>,
}

impl<S> SubscriptionResponse<S>
where
    S: Stream<Item = Result<Update>> + Send + 'static,
{
    /// Create a new subscription response from a stream of updates.
    pub fn new(stream: S) -> Self {
        SubscriptionResponse {
            stream,
            headers: BTreeMap::new(),
        }
    }

    /// Add a custom header.
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }
}

impl<S> IntoResponse for SubscriptionResponse<S>
where
    S: Stream<Item = Result<Update>> + Send + 'static,
{
    fn into_response(self) -> Response {
        let stream = self.stream.map(|result| {
            match result {
                Ok(update) => {
                    protocol::format_update(&update)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                }
                Err(e) => {
                    // Log error or send error frame if protocol supports it
                    // For now, just terminate stream with error
                    Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                }
            }
        });

        let mut builder = Response::builder()
            .status(StatusCode::from_u16(209).unwrap());

        for (key, value) in self.headers {
            if let Ok(header_value) = value.parse::<HeaderValue>() {
                builder = builder.header(key, header_value);
            }
        }

        builder
            .body(Body::from_stream(stream))
            .unwrap_or_else(|_| Response::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_response_builder() {
        let response = UpdateResponse::new(200)
            .with_version(vec![Version::from("v1")])
            .with_header("Custom".to_string(), "value".to_string())
            .build();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
