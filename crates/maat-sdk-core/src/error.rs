//! Shared error taxonomy.
//!
//! Both `maat-agent` and `maat-resource` re-export [`SdkError`] and add
//! their own crate-specific error types where the taxonomy doesn't fit.

use thiserror::Error;

use crate::descriptor::DescriptorError;

/// Errors raised by SDK operations.
#[derive(Debug, Error)]
pub enum SdkError {
    /// HTTP transport failure: network unreachable, timeout, malformed
    /// response body, etc.
    #[error("transport error: {0}")]
    Transport(String),

    /// The peer (gateway or resource) returned a structured error response.
    /// `status` is the HTTP status code; `reason` is a short token (e.g.,
    /// "constraint_violated"); `detail` is human-readable detail.
    #[error("rejected ({status}): {reason}: {detail}")]
    Rejected {
        status: u16,
        reason: String,
        detail: String,
    },

    /// Local cryptographic failure: signature verification failed, key
    /// could not be parsed, etc.
    #[error("crypto error: {0}")]
    Crypto(String),

    /// Configuration error: bad URL, malformed credentials, missing
    /// required setting.
    #[error("configuration error: {0}")]
    Config(String),

    /// Storage backend error (delegation store, replay store, etc.).
    #[error("storage error: {0}")]
    Storage(String),

    /// Descriptor encoding or decoding failed.
    #[error(transparent)]
    Descriptor(#[from] DescriptorError),

    /// Protocol-level violation surfaced by the `maat` library
    /// (e.g., invalid attenuation when issuing a sub-delegation).
    #[error("protocol error: {0}")]
    Protocol(String),
}
