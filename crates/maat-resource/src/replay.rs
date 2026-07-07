//! Replay protection.
//!
//! [`ReplayStore`] is the contract the resource implements with its own
//! storage backend (Postgres, sqlite, Redis, anything with atomic
//! insert-if-absent semantics). [`MemoryReplayStore`] is for tests.
//!
//! ## Atomicity contract
//!
//! `try_consume(receipt_id)` MUST be atomic: if two concurrent calls
//! present the same `receipt_id`, exactly one MUST receive `Ok(true)`
//! (newly consumed) and the other `Ok(false)` (replay). Backed-by-
//! database implementations typically use
//! `INSERT ... ON CONFLICT DO NOTHING` or equivalent.

use std::collections::HashSet;

use async_trait::async_trait;
use parking_lot::Mutex;

use maat_sdk_core::SdkError;

/// Storage for the set of receipt IDs the resource has already
/// processed.
#[async_trait]
pub trait ReplayStore: Send + Sync {
    /// Mark the given receipt ID as consumed.
    ///
    /// Returns:
    /// - `Ok(true)` if the ID was newly inserted (first sighting).
    /// - `Ok(false)` if the ID was already present (replay attempt).
    /// - `Err(_)` on storage failure.
    async fn try_consume(&self, receipt_id: &[u8]) -> Result<bool, SdkError>;
}

/// In-memory replay store. For tests and short-lived resources.
#[derive(Debug, Default)]
pub struct MemoryReplayStore {
    seen: Mutex<HashSet<Vec<u8>>>,
}

impl MemoryReplayStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ReplayStore for MemoryReplayStore {
    async fn try_consume(&self, receipt_id: &[u8]) -> Result<bool, SdkError> {
        let mut seen = self.seen.lock();
        Ok(seen.insert(receipt_id.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn first_consume_succeeds_second_reports_replay() {
        let store = MemoryReplayStore::new();
        let id = b"receipt-1";
        assert!(store.try_consume(id).await.unwrap());
        assert!(!store.try_consume(id).await.unwrap());
    }

    #[tokio::test]
    async fn distinct_ids_are_independent() {
        let store = MemoryReplayStore::new();
        assert!(store.try_consume(b"a").await.unwrap());
        assert!(store.try_consume(b"b").await.unwrap());
    }
}
