//! Shared header parsing and formatting for Braid-HTTP.
//!
//! This module provides utilities for parsing and formatting Braid protocol headers
//! according to [RFC 8941 Structured Headers] format.
//!
//! # Header Formats
//!
//! | Header | Format | Example |
//! |--------|--------|---------|
//! | Version | Quoted, comma-separated | `"v1", "v2"` |
//! | Parents | Quoted, comma-separated | `"v0"` |
//! | Content-Range | `{unit} {range}` | `json .field` |
//! | Heartbeats | Number with optional unit | `30s`, `500ms`, `30` |
//!
//! # Examples
//!
//! ```
//! use braid_axum_http::protocol::{
//!     parse_version_header, format_version_header,
//!     parse_content_range, format_content_range,
//!     parse_heartbeat,
//! };
//! use braid_axum_http::Version;
//!
//! // Version headers
//! let versions = parse_version_header(r#""v1", "v2""#).unwrap();
//! let header = format_version_header(&versions);
//!
//! // Content-Range headers
//! let (unit, range) = parse_content_range("json .field").unwrap();
//! let header = format_content_range("bytes", "0:100");
//!
//! // Heartbeat headers
//! let seconds = parse_heartbeat("30s").unwrap();
//! ```
//!
//! [RFC 8941 Structured Headers]: https://datatracker.ietf.org/doc/html/rfc8941

use crate::error::{BraidError, Result};
use crate::types::Version;

/// Parse version header value (comma-separated quoted strings).
///
/// Handles [RFC 8941 Structured Headers] format for version identifiers.
/// Supports both quoted strings (`"v1"`) and unquoted values (`v1`).
///
/// # Arguments
///
/// * `value` - The header value to parse
///
/// # Returns
///
/// A vector of [`Version`] values, or an empty vector if the input is empty.
///
/// # Examples
///
/// ```
/// use braid_axum_http::protocol::parse_version_header;
///
/// // Quoted versions
/// let versions = parse_version_header(r#""v1", "v2""#).unwrap();
/// assert_eq!(versions.len(), 2);
///
/// // Unquoted versions
/// let versions = parse_version_header("v1, v2").unwrap();
/// assert_eq!(versions.len(), 2);
///
/// // Empty input
/// let versions = parse_version_header("").unwrap();
/// assert!(versions.is_empty());
/// ```
///
/// [RFC 8941 Structured Headers]: https://datatracker.ietf.org/doc/html/rfc8941
pub fn parse_version_header(value: &str) -> Result<Vec<Version>> {
    if value.is_empty() {
        return Ok(Vec::new());
    }

    let mut versions = Vec::new();

    for part in value.split(',') {
        let trimmed = part.trim();
        let version_str = if trimmed.starts_with('"') && trimmed.ends_with('"') {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        };
        versions.push(Version::String(version_str.to_string()));
    }

    Ok(versions)
}

/// Format version header value (comma-separated quoted strings).
///
/// Converts a vector of versions to [RFC 8941 Structured Headers] format.
///
/// # Arguments
///
/// * `versions` - Slice of versions to format
///
/// # Examples
///
/// ```
/// use braid_axum_http::{Version, protocol::format_version_header};
///
/// let versions = vec![Version::new("v1"), Version::new("v2")];
/// let header = format_version_header(&versions);
/// assert_eq!(header, r#""v1", "v2""#);
/// ```
///
/// [RFC 8941 Structured Headers]: https://datatracker.ietf.org/doc/html/rfc8941
pub fn format_version_header(versions: &[Version]) -> String {
    versions
        .iter()
        .map(|v| format!("\"{}\"", v))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse Content-Range header.
///
/// Extracts unit and range from a Content-Range header value.
/// Format: `"{unit} {range}"` (e.g., `"json .field"` or `"bytes 0:100"`).
///
/// # Arguments
///
/// * `value` - The header value to parse
///
/// # Returns
///
/// A tuple of `(unit, range)` strings.
///
/// # Errors
///
/// Returns an error if the value doesn't contain a space separator.
///
/// # Examples
///
/// ```
/// use braid_axum_http::protocol::parse_content_range;
///
/// let (unit, range) = parse_content_range("json .field").unwrap();
/// assert_eq!(unit, "json");
/// assert_eq!(range, ".field");
///
/// let (unit, range) = parse_content_range("bytes 0:100").unwrap();
/// assert_eq!(unit, "bytes");
/// assert_eq!(range, "0:100");
/// ```
pub fn parse_content_range(value: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(BraidError::HeaderParse(format!(
            "Invalid Content-Range: expected 'unit range', got '{}'",
            value
        )));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Format Content-Range header.
///
/// Creates a Content-Range header value from unit and range components.
///
/// # Arguments
///
/// * `unit` - The addressing unit (e.g., `"json"`, `"bytes"`)
/// * `range` - The range specification
///
/// # Examples
///
/// ```
/// use braid_axum_http::protocol::format_content_range;
///
/// let header = format_content_range("json", ".field");
/// assert_eq!(header, "json .field");
///
/// let header = format_content_range("bytes", "0:100");
/// assert_eq!(header, "bytes 0:100");
/// ```
#[inline]
pub fn format_content_range(unit: &str, range: &str) -> String {
    format!("{} {}", unit, range)
}

/// Parse heartbeat interval.
///
/// Converts heartbeat header value to seconds.
///
/// # Supported Formats
///
/// | Format | Example | Result |
/// |--------|---------|--------|
/// | Seconds with suffix | `"5s"` | 5 |
/// | Milliseconds | `"500ms"` | 0 (rounded down) |
/// | Plain number | `"30"` | 30 |
///
/// # Arguments
///
/// * `value` - The header value to parse
///
/// # Returns
///
/// The interval in seconds.
///
/// # Errors
///
/// Returns an error if the value cannot be parsed as a number.
///
/// # Examples
///
/// ```
/// use braid_axum_http::protocol::parse_heartbeat;
///
/// assert_eq!(parse_heartbeat("5s").unwrap(), 5);
/// assert_eq!(parse_heartbeat("30").unwrap(), 30);
/// assert_eq!(parse_heartbeat("1000ms").unwrap(), 1);
/// ```
pub fn parse_heartbeat(value: &str) -> Result<u64> {
    let trimmed = value.trim();

    // Handle milliseconds
    if let Some(ms_str) = trimmed.strip_suffix("ms") {
        return ms_str
            .parse::<u64>()
            .map(|n| n / 1000)
            .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)));
    }

    // Handle seconds with 's' suffix
    if let Some(s_str) = trimmed.strip_suffix('s') {
        return s_str
            .parse()
            .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)));
    }

    // Plain number (assume seconds)
    trimmed
        .parse()
        .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_header() {
        let result = parse_version_header(r#""v1", "v2", "v3""#).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_parse_version_header_unquoted() {
        let result = parse_version_header("v1, v2").unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_version_header_empty() {
        let result = parse_version_header("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_version_header() {
        let versions = vec![Version::new("v1"), Version::new("v2")];
        let header = format_version_header(&versions);
        assert_eq!(header, r#""v1", "v2""#);
    }

    #[test]
    fn test_parse_content_range() {
        let (unit, range) = parse_content_range("json .field").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".field");
    }

    #[test]
    fn test_parse_content_range_complex() {
        let (unit, range) = parse_content_range("json .users[0].name").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".users[0].name");
    }

    #[test]
    fn test_parse_content_range_invalid() {
        let result = parse_content_range("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_content_range() {
        let result = format_content_range("bytes", "0:100");
        assert_eq!(result, "bytes 0:100");
    }

    #[test]
    fn test_parse_heartbeat_seconds() {
        assert_eq!(parse_heartbeat("5s").unwrap(), 5);
    }

    #[test]
    fn test_parse_heartbeat_plain() {
        assert_eq!(parse_heartbeat("30").unwrap(), 30);
    }

    #[test]
    fn test_parse_heartbeat_milliseconds() {
        assert_eq!(parse_heartbeat("1000ms").unwrap(), 1);
        assert_eq!(parse_heartbeat("500ms").unwrap(), 0);
    }

    #[test]
    fn test_parse_heartbeat_invalid() {
        assert!(parse_heartbeat("abc").is_err());
    }
}
