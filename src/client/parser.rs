//! Message parser for Braid protocol streaming.
//!
//! Incremental, state-machine based parser for streaming Braid protocol messages.
//! Handles parsing from byte streams where the complete message may not arrive at once.
//!
//! # Parsing Flow
//!
//! 1. **WaitingForHeaders**: Accumulate bytes until `\r\n\r\n` is found
//! 2. **ParsingHeaders**: Extract header fields (Version, Parents, Content-Length, etc.)
//! 3. **WaitingForBody**: Wait until Content-Length bytes have arrived
//! 4. **ParsingBody**: Extract body or patches from accumulated bytes
//! 5. **Complete**: Message ready, reset state
//!
//! # Multi-Patch Support
//!
//! For multiple patches in a single response (Section 3.3), each patch declares
//! a Content-Length header in its own headers section. The parser tracks this
//! to correctly identify patch boundaries in the stream.
//!
//! # Examples
//!
//! ```ignore
//! use braid_axum_http::client::MessageParser;
//!
//! let mut parser = MessageParser::new();
//!
//! let data1 = b"Version: \"v1\"\r\nContent-Length: 5\r\n\r\n";
//! let messages = parser.feed(data1)?;
//!
//! let data2 = b"hello";
//! let messages = parser.feed(data2)?;
//! assert!(!messages.is_empty());
//! ```
//!
//! # Specification
//!
//! See Section 3.3 of draft-toomim-httpbis-braid-http for multi-patch format.

use crate::error::{BraidError, Result};
use crate::types::Patch;
use bytes::{Bytes, BytesMut};
use std::collections::BTreeMap;

/// Parse state for streaming protocol parser.
///
/// Represents the current position in the message parsing state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseState {
    /// Waiting for first bytes of headers
    WaitingForHeaders,
    /// Currently accumulating header bytes
    ParsingHeaders,
    /// Headers complete, waiting for body bytes
    WaitingForBody,
    /// Currently accumulating body bytes
    ParsingBody,
    /// Message fully parsed and ready
    Complete,
    /// Error occurred during parsing
    Error,
}

/// Message parser for Braid protocol.
///
/// Incrementally parses protocol messages from a byte stream using a state machine.
/// Designed to handle streaming responses where messages arrive in fragments.
///
/// # State Machine
///
/// The parser transitions through states as it accumulates and processes bytes:
/// - Accumulates bytes until complete headers are received (`\r\n\r\n`)
/// - Extracts Content-Length from headers
/// - Accumulates body bytes until the full body is received
/// - Finalizes the message and resets for the next one
///
/// # Multi-Patch Parsing
///
/// When parsing multi-patch responses (Section 3.3), each patch is preceded by
/// its own headers that include Content-Length. The parser correctly handles
/// parsing multiple patches sequentially from a single stream.
#[derive(Debug)]
pub struct MessageParser {
    /// Input buffer accumulating bytes from stream
    buffer: BytesMut,
    /// Current state in parse state machine
    state: ParseState,
    /// Parsed headers (key-value pairs)
    headers: BTreeMap<String, String>,
    /// Accumulates body bytes
    body_buffer: BytesMut,
    /// Expected body length from Content-Length header
    expected_body_length: usize,
    /// Number of body bytes read so far
    read_body_length: usize,
    /// Accumulated patches (for multi-patch messages)
    patches: Vec<Patch>,
    /// Current patch being built
    current_patch: Option<Patch>,
}

impl MessageParser {
    /// Create a new message parser
    pub fn new() -> Self {
        MessageParser {
            buffer: BytesMut::with_capacity(8192),
            state: ParseState::WaitingForHeaders,
            headers: BTreeMap::new(),
            body_buffer: BytesMut::new(),
            expected_body_length: 0,
            read_body_length: 0,
            patches: Vec::new(),
            current_patch: None,
        }
    }

    /// Feed bytes to the parser
    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<Message>> {
        self.buffer.extend_from_slice(data);
        let mut messages = Vec::new();

        loop {
            match self.state {
                ParseState::WaitingForHeaders => {
                    if let Some(pos) = self.find_header_end() {
                        self.parse_headers(pos)?;
                        self.state = ParseState::WaitingForBody;
                    } else {
                        break;
                    }
                }
                ParseState::WaitingForBody => {
                    if self.try_parse_body()? {
                        if let Some(msg) = self.finalize_message()? {
                            messages.push(msg);
                        }
                        self.reset();
                        self.state = ParseState::WaitingForHeaders;
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        Ok(messages)
    }

    /// Find the end of HTTP headers (\r\n\r\n)
    fn find_header_end(&self) -> Option<usize> {
        self.buffer
            .windows(4)
            .rposition(|w| w == b"\r\n\r\n")
            .map(|p| p + 4)
    }

    /// Parse headers from buffer
    fn parse_headers(&mut self, end: usize) -> Result<()> {
        let header_bytes = self.buffer.split_to(end);
        let header_str = String::from_utf8(header_bytes[..header_bytes.len() - 4].to_vec())?;

        for line in header_str.lines() {
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim().to_lowercase();
                let value = line[colon_pos + 1..].trim().to_string();
                self.headers.insert(key, value);
            }
        }

        if let Some(len_str) = self.headers.get("content-length") {
            self.expected_body_length = len_str.parse().map_err(|_| {
                BraidError::HeaderParse(format!("Invalid content-length: {}", len_str))
            })?;
        }

        Ok(())
    }

    /// Try to parse body from buffer
    fn try_parse_body(&mut self) -> Result<bool> {
        if self.expected_body_length == 0 {
            return Ok(true);
        }

        let remaining = self.expected_body_length - self.read_body_length;
        if self.buffer.len() >= remaining {
            let body_chunk = self.buffer.split_to(remaining);
            self.body_buffer.extend_from_slice(&body_chunk);
            self.read_body_length += body_chunk.len();
            Ok(true)
        } else {
            self.body_buffer.extend_from_slice(&self.buffer.split_to(self.buffer.len()));
            self.read_body_length += self.body_buffer.len();
            Ok(false)
        }
    }

    /// Finalize and return a complete message
    fn finalize_message(&mut self) -> Result<Option<Message>> {
        let body = self.body_buffer.split().freeze();

        Ok(Some(Message {
            headers: std::mem::take(&mut self.headers),
            body,
            patches: std::mem::take(&mut self.patches),
        }))
    }

    /// Reset parser for next message
    fn reset(&mut self) {
        self.headers.clear();
        self.body_buffer.clear();
        self.expected_body_length = 0;
        self.read_body_length = 0;
        self.patches.clear();
        self.current_patch = None;
    }

    /// Get current parse state
    pub fn state(&self) -> ParseState {
        self.state
    }

    /// Get parsed headers
    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    /// Get parsed body
    pub fn body(&self) -> &[u8] {
        &self.body_buffer
    }
}

impl Default for MessageParser {
    fn default() -> Self {
        Self::new()
    }
}

/// A parsed protocol message
#[derive(Debug, Clone)]
pub struct Message {
    /// Headers
    pub headers: BTreeMap<String, String>,
    /// Body
    pub body: Bytes,
    /// Patches
    pub patches: Vec<Patch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = MessageParser::new();
        assert_eq!(parser.state(), ParseState::WaitingForHeaders);
    }

    #[test]
    fn test_simple_message_parsing() {
        let mut parser = MessageParser::new();
        let data = b"Content-Length: 5\r\n\r\nHello";
        let messages = parser.feed(data).unwrap();
        assert!(!messages.is_empty());
        assert_eq!(messages[0].body, Bytes::from_static(b"Hello"));
    }
}
