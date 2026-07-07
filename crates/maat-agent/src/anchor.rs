//! Anchor construction.
//!
//! [`AnchorBuilder`] wraps the protocol library's `Anchor::builder` with
//! the SDK's ergonomic conventions:
//! - State entries via [`StateBinding`] convenience constructors
//!   (URL+hash, content hash by label, JSON document hash).
//! - Freshness as a [`std::time::Duration`] parameter rather than raw seconds.
//! - Sensible defaults: 5-minute staleness, abort contingency.
//!
//! For richer cases the protocol's [`maat::Anchor::builder`] is fully
//! exposed — the SDK doesn't try to wrap every method.

use std::time::Duration;

use maat::{Anchor, Contingency, HashAlgorithm, ObjectId, Signer, StateEntry};
use maat_sdk_core::SdkError;

/// Fluent builder for an [`Anchor`].
///
/// ## Example: stateless freshness anchor
///
/// ```no_run
/// use std::time::Duration;
/// use maat::ObjectId;
/// use maat_agent::{AgentIdentity, AnchorBuilder};
///
/// let id = AgentIdentity::generate();
/// let delegation_id: ObjectId = ObjectId::zero();  // in real code, from your delegation
///
/// let anchor = AnchorBuilder::new(delegation_id)
///     .max_staleness(Duration::from_secs(60))
///     .build(&id)
///     .unwrap();
/// ```
///
/// ## Example: anchor bound to external state
///
/// ```no_run
/// use std::time::Duration;
/// use maat::ObjectId;
/// use maat_agent::{AgentIdentity, AnchorBuilder, StateBinding};
///
/// let id = AgentIdentity::generate();
/// let delegation_id: ObjectId = ObjectId::zero();
///
/// let anchor = AnchorBuilder::new(delegation_id)
///     .add_state(StateBinding::url_hash(
///         "https://api.example.com/orders/12345",
///         [0xAB; 32].to_vec(),
///         "order",
///     ))
///     .add_state(StateBinding::content_hash(
///         "git_commit",
///         [0xCD; 32].to_vec(),
///     ))
///     .max_staleness(Duration::from_secs(60))
///     .build(&id)
///     .unwrap();
/// ```
pub struct AnchorBuilder {
    delegation_id: ObjectId,
    state_entries: Vec<StateEntry>,
    max_staleness: Duration,
    contingency: Contingency,
    created_at: Option<u64>,
}

impl AnchorBuilder {
    pub fn new(delegation_id: ObjectId) -> Self {
        AnchorBuilder {
            delegation_id,
            state_entries: Vec::new(),
            max_staleness: Duration::from_secs(300),
            contingency: Contingency::Abort,
            created_at: None,
        }
    }

    /// Add a state binding. Multiple calls accumulate.
    pub fn add_state(mut self, binding: StateBinding) -> Self {
        self.state_entries.push(binding.0);
        self
    }

    /// Replace the state entries entirely.
    pub fn state_entries(mut self, entries: Vec<StateEntry>) -> Self {
        self.state_entries = entries;
        self
    }

    /// Set the maximum staleness window.
    pub fn max_staleness(mut self, max: Duration) -> Self {
        self.max_staleness = max;
        self
    }

    /// Set the contingency: what the resource should do if the anchor's
    /// referenced state has changed.
    pub fn contingency(mut self, contingency: Contingency) -> Self {
        self.contingency = contingency;
        self
    }

    /// Pin `created_at` to a specific Unix-second timestamp. Mainly for
    /// determinism in tests.
    pub fn created_at(mut self, t: u64) -> Self {
        self.created_at = Some(t);
        self
    }

    /// Build and sign the anchor.
    pub fn build<S: Signer>(self, signer: &S) -> Result<Anchor, SdkError> {
        let mut b = Anchor::builder(self.delegation_id)
            .state_entries(self.state_entries)
            .max_staleness(self.max_staleness.as_secs())
            .contingency(self.contingency);
        if let Some(t) = self.created_at {
            b = b.created_at(t);
        }
        b.build(signer)
            .map_err(|e| SdkError::Protocol(e.to_string()))
    }
}

// ─── StateBinding convenience ──────────────────────────────────────────────

/// Newtype wrapper around [`StateEntry`] with constructors for the most
/// common binding patterns. The protocol's `StateEntry` is fully exposed
/// for cases that need direct construction.
#[derive(Debug, Clone)]
pub struct StateBinding(pub StateEntry);

impl StateBinding {
    /// Bind to a URL whose content hashes to `state_hash`. Use SHA-256
    /// (the default for most resource integrations).
    pub fn url_hash(
        source_uri: impl Into<String>,
        state_hash: Vec<u8>,
        label: impl Into<String>,
    ) -> Self {
        StateBinding(StateEntry {
            source_uri: source_uri.into(),
            state_hash,
            hash_alg: HashAlgorithm::Sha256,
            label: label.into(),
        })
    }

    /// Bind to a labeled piece of state with a precomputed hash —
    /// useful when the state is identified by name (e.g., `git_commit`)
    /// rather than a URL.
    pub fn content_hash(label: impl Into<String>, hash: Vec<u8>) -> Self {
        StateBinding(StateEntry {
            source_uri: String::new(),
            state_hash: hash,
            hash_alg: HashAlgorithm::Sha256,
            label: label.into(),
        })
    }

    /// Bind to a serializable document — the SDK computes the SHA-256
    /// hash of the JSON serialization. Useful for binding anchors to
    /// query results, payload bodies, message contents, etc.
    pub fn json_document<T: serde::Serialize>(
        label: impl Into<String>,
        document: &T,
    ) -> Result<Self, SdkError> {
        use sha2_hash::digest;
        let bytes = serde_json::to_vec(document)
            .map_err(|e| SdkError::Config(format!("json serialize failed: {}", e)))?;
        let hash = digest(&bytes);
        Ok(StateBinding(StateEntry {
            source_uri: String::new(),
            state_hash: hash,
            hash_alg: HashAlgorithm::Sha256,
            label: label.into(),
        }))
    }

    /// Use a custom hash algorithm (advanced; the SDK's other
    /// constructors all use SHA-256).
    pub fn with_hash_alg(mut self, alg: HashAlgorithm) -> Self {
        self.0.hash_alg = alg;
        self
    }
}

// Tiny SHA-256 helper using the maat crate's transitively available
// dependency. We use the crate's existing crypto if exported; fall back
// to manual digest if not.
mod sha2_hash {
    pub fn digest(bytes: &[u8]) -> Vec<u8> {
        // Maat uses sha2 transitively via ed25519-dalek. We use the
        // protocol library's `crypto::sha256` helper.
        maat::crypto::sha256(bytes).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentIdentity;
    use maat::ObjectId;

    #[test]
    fn stateless_anchor_builds() {
        let id = AgentIdentity::generate();
        let did = ObjectId::zero();
        let anchor = AnchorBuilder::new(did)
            .max_staleness(Duration::from_secs(60))
            .build(&id)
            .unwrap();
        assert!(anchor.state_entries.is_empty());
        assert_eq!(anchor.max_staleness, 60);
    }

    #[test]
    fn anchor_with_url_state() {
        let id = AgentIdentity::generate();
        let did = ObjectId::zero();
        let anchor = AnchorBuilder::new(did)
            .add_state(StateBinding::url_hash(
                "https://api.example.com/x",
                vec![0; 32],
                "x",
            ))
            .build(&id)
            .unwrap();
        assert_eq!(anchor.state_entries.len(), 1);
        assert_eq!(anchor.state_entries[0].label, "x");
    }

    #[test]
    fn json_document_binding_hashes_deterministically() {
        let doc = serde_json::json!({ "a": 1, "b": 2 });
        let b1 = StateBinding::json_document("doc", &doc).unwrap();
        let b2 = StateBinding::json_document("doc", &doc).unwrap();
        assert_eq!(b1.0.state_hash, b2.0.state_hash);
        assert_eq!(b1.0.state_hash.len(), 32); // SHA-256
    }

    #[test]
    fn created_at_is_pinned_for_determinism() {
        let id = AgentIdentity::generate();
        let did = ObjectId::zero();
        let anchor = AnchorBuilder::new(did)
            .max_staleness(Duration::from_secs(60))
            .created_at(1_700_000_000)
            .build(&id)
            .unwrap();
        assert_eq!(anchor.created_at, 1_700_000_000);
    }
}
