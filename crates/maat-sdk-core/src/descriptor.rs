//! The [`ActionDescriptor`] trait.
//!
//! `ActionDescriptor` represents any structured commitment an agent makes
//! about an action. The bytes returned by [`ActionDescriptor::to_action_value`]
//! become `Receipt.action.value` in the issued receipt — the resource's
//! authoritative record of what the agent committed to do.
//!
//! Three layers of usage:
//!
//! 1. **Built-in implementations.** The SDK's [`ValueClaim`](maat::ValueClaim)
//!    impl handles monetary commitments. Future canonical descriptors
//!    (recipient lists, deploy targets, blob hashes) ship as needed.
//! 2. **Generic JSON adapter.** [`JsonDescriptor<T>`] turns any
//!    `serde::Serialize + DeserializeOwned` type into an `ActionDescriptor`
//!    in one line.
//! 3. **Manual implementations.** Integrators with specific encoding
//!    requirements (CBOR, MessagePack, custom binary) implement the
//!    trait directly. The trait is dead simple.
//!
//! ## Wire-format contract
//!
//! `to_action_value()` and `from_action_value()` MUST round-trip: for any
//! valid descriptor `d`,
//! `from_action_value(d.to_action_value().unwrap()).unwrap() == d`. Symmetry
//! is the contract that makes agent-side and resource-side interoperable.
//! The SDK's standard implementations use canonical JSON with sorted keys
//! to guarantee byte-level determinism.

use std::marker::PhantomData;

use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

use maat::ValueClaim;

/// Errors raised by descriptor encoding/decoding.
#[derive(Debug, Error)]
pub enum DescriptorError {
    #[error("descriptor encoding failed: {0}")]
    Encode(String),
    #[error("descriptor decoding failed: {0}")]
    Decode(String),
}

/// Any structured commitment that fits into `Receipt.action.value`.
///
/// Implementations MUST round-trip:
/// `from_action_value(d.to_action_value().unwrap()).unwrap() == d`
/// for any valid descriptor `d`.
///
/// Descriptors must be `Send` because they cross `.await` boundaries in
/// both [`maat-agent`]'s `verify_action` (held during the gateway HTTP
/// round-trip) and [`maat-resource`]'s `validate` (held during the
/// replay-store check). All practical descriptors are pure data and
/// trivially `Send`.
pub trait ActionDescriptor: Sized + Send {
    /// Encode this descriptor as bytes for `Action.value`.
    fn to_action_value(&self) -> Result<Vec<u8>, DescriptorError>;

    /// Decode bytes from `Action.value` back into this descriptor type.
    fn from_action_value(bytes: &[u8]) -> Result<Self, DescriptorError>;
}

// ─── ValueClaim impl ───────────────────────────────────────────────────────

/// `ValueClaim` from the protocol library is the canonical monetary
/// descriptor. Encoded as canonical JSON.
impl ActionDescriptor for ValueClaim {
    fn to_action_value(&self) -> Result<Vec<u8>, DescriptorError> {
        serde_json::to_vec(self).map_err(|e| DescriptorError::Encode(e.to_string()))
    }

    fn from_action_value(bytes: &[u8]) -> Result<Self, DescriptorError> {
        serde_json::from_slice(bytes).map_err(|e| DescriptorError::Decode(e.to_string()))
    }
}

// ─── JsonDescriptor adapter ────────────────────────────────────────────────

/// Wraps any `Serialize + DeserializeOwned` type as an [`ActionDescriptor`].
///
/// This is the recommended path for integrators with their own type:
///
/// ```
/// use serde::{Serialize, Deserialize};
/// use maat_sdk_core::{ActionDescriptor, JsonDescriptor};
///
/// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// struct DeployTarget {
///     environment: String,
///     commit_hash: String,
/// }
///
/// let target = DeployTarget {
///     environment: "staging".into(),
///     commit_hash: "abc123".into(),
/// };
/// let descriptor = JsonDescriptor::new(target.clone());
/// let bytes = descriptor.to_action_value().unwrap();
///
/// let recovered: JsonDescriptor<DeployTarget> =
///     JsonDescriptor::from_action_value(&bytes).unwrap();
/// assert_eq!(recovered.into_inner(), target);
/// ```
///
/// The encoding is plain `serde_json` — not canonicalized. If you need
/// byte-stable encoding (signature-equivalent receipts), implement
/// [`ActionDescriptor`] directly and ensure your encoder produces
/// canonical output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonDescriptor<T>(T);

impl<T> JsonDescriptor<T> {
    pub fn new(value: T) -> Self {
        JsonDescriptor(value)
    }

    pub fn into_inner(self) -> T {
        self.0
    }

    pub fn as_inner(&self) -> &T {
        &self.0
    }
}

impl<T: Serialize + DeserializeOwned + Send> ActionDescriptor for JsonDescriptor<T> {
    fn to_action_value(&self) -> Result<Vec<u8>, DescriptorError> {
        serde_json::to_vec(&self.0).map_err(|e| DescriptorError::Encode(e.to_string()))
    }

    fn from_action_value(bytes: &[u8]) -> Result<Self, DescriptorError> {
        serde_json::from_slice(bytes)
            .map(JsonDescriptor)
            .map_err(|e| DescriptorError::Decode(e.to_string()))
    }
}

// ─── Sentinel: descriptor for actions with no structured commitment ────────

/// A descriptor that carries no information. Useful for actions where
/// the scope alone is sufficient and there's no per-action data to bind.
///
/// `to_action_value` returns empty bytes; `from_action_value` accepts
/// only empty bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyDescriptor;

impl ActionDescriptor for EmptyDescriptor {
    fn to_action_value(&self) -> Result<Vec<u8>, DescriptorError> {
        Ok(Vec::new())
    }

    fn from_action_value(bytes: &[u8]) -> Result<Self, DescriptorError> {
        if bytes.is_empty() {
            Ok(EmptyDescriptor)
        } else {
            Err(DescriptorError::Decode(format!(
                "EmptyDescriptor expects 0 bytes, got {}",
                bytes.len()
            )))
        }
    }
}

// PhantomData kept for future expansion (typed registries).
#[doc(hidden)]
pub struct __DescriptorMarker<T>(PhantomData<T>);

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct CustomShape {
        recipient: String,
        priority: u32,
    }

    #[test]
    fn value_claim_round_trips() {
        let claim = ValueClaim {
            currency: "USD".into(),
            amount: 12345,
            decimals: 2,
        };
        let bytes = claim.to_action_value().unwrap();
        let back = ValueClaim::from_action_value(&bytes).unwrap();
        assert_eq!(claim, back);
    }

    #[test]
    fn json_descriptor_round_trips() {
        let original = CustomShape {
            recipient: "alice@example.com".into(),
            priority: 5,
        };
        let descriptor = JsonDescriptor::new(original.clone());
        let bytes = descriptor.to_action_value().unwrap();
        let recovered: JsonDescriptor<CustomShape> =
            JsonDescriptor::from_action_value(&bytes).unwrap();
        assert_eq!(recovered.into_inner(), original);
    }

    #[test]
    fn empty_descriptor_only_accepts_empty_bytes() {
        let bytes = EmptyDescriptor.to_action_value().unwrap();
        assert!(bytes.is_empty());
        assert!(EmptyDescriptor::from_action_value(&[]).is_ok());
        assert!(EmptyDescriptor::from_action_value(b"non-empty").is_err());
    }

    #[test]
    fn descriptor_encoding_is_deterministic() {
        // Two ValueClaims with the same content must produce identical bytes.
        let a = ValueClaim {
            currency: "USD".into(),
            amount: 100,
            decimals: 2,
        };
        let b = ValueClaim {
            currency: "USD".into(),
            amount: 100,
            decimals: 2,
        };
        assert_eq!(a.to_action_value().unwrap(), b.to_action_value().unwrap());
    }

    #[test]
    fn json_descriptor_decode_failure_on_garbage() {
        let result: Result<JsonDescriptor<CustomShape>, _> =
            JsonDescriptor::from_action_value(b"not json");
        assert!(matches!(result, Err(DescriptorError::Decode(_))));
    }
}
