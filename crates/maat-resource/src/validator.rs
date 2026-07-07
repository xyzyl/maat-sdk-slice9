//! Receipt validation.
//!
//! [`ReceiptValidator`] runs the protocol-level checks that don't require
//! resource-specific state:
//!
//! 1. Parse receipt JSON.
//! 2. Confirm signed by the trusted executor (constant-time pubkey compare).
//! 3. Verify receipt signature.
//! 4. Decode the action descriptor from `receipt.action.value`.
//! 5. Check `executed_at` not in future; not older than `max_age`.
//! 6. Atomically mark as consumed via the [`ReplayStore`].
//!
//! Returns [`ValidatedReceipt`] on success: the parsed receipt and the
//! typed descriptor. The operator does the resource-specific match
//! (cart total, recipient allowlist, deploy target permissions,
//! whatever applies).
//!
//! ## What this validator does NOT do
//!
//! - Compare the descriptor against the resource's own state. The
//!   validator doesn't know what your cart/queue/repo looks like.
//! - Re-verify the delegation chain. The gateway already did that when
//!   it issued the receipt; the receipt's signature is the gateway's
//!   attestation that the chain checked out. (The gateway issuing a
//!   receipt for an invalid chain would itself be a protocol violation.)
//! - Fulfill the action. That's operator code that runs after `validate`
//!   returns Ok.
//!
//! ## Generic over descriptor
//!
//! ```no_run
//! use std::time::Duration;
//! use maat::ValueClaim;
//! use maat_resource::{ReceiptValidator, ExecutorKey, MemoryReplayStore};
//!
//! # async fn run(executor_key: ExecutorKey) -> Result<(), Box<dyn std::error::Error>> {
//! let validator = ReceiptValidator::<ValueClaim, _>::new(
//!     executor_key,
//!     MemoryReplayStore::new(),
//! )
//! .with_max_age(Duration::from_secs(60));
//!
//! # let receipt_bytes: Vec<u8> = vec![];
//! let validated = validator.validate(&receipt_bytes).await?;
//! // validated.descriptor is a ValueClaim — typed, ready to compare against your cart.
//! # Ok(())
//! # }
//! ```

use std::marker::PhantomData;
use std::time::Duration;

use maat::Receipt;
use maat_sdk_core::{ActionDescriptor, Clock, SystemClock};
use thiserror::Error;

use crate::executor_key::ExecutorKey;
use crate::replay::ReplayStore;

const DEFAULT_MAX_AGE_SECS: u64 = 60;

/// Validates incoming receipts.
///
/// Generic over [`ActionDescriptor`] (the agent's commitment shape),
/// [`ReplayStore`] (operator-supplied storage), and [`Clock`]
/// (defaults to `SystemClock`; tests inject `FixedClock`).
pub struct ReceiptValidator<D, S, C = SystemClock>
where
    D: ActionDescriptor,
    S: ReplayStore,
    C: Clock,
{
    executor_key: ExecutorKey,
    replay: S,
    clock: C,
    max_age_secs: u64,
    _descriptor: PhantomData<D>,
}

impl<D, S> ReceiptValidator<D, S, SystemClock>
where
    D: ActionDescriptor,
    S: ReplayStore,
{
    /// Construct a validator with system clock and default 60-second
    /// `max_age`.
    pub fn new(executor_key: ExecutorKey, replay: S) -> Self {
        ReceiptValidator {
            executor_key,
            replay,
            clock: SystemClock,
            max_age_secs: DEFAULT_MAX_AGE_SECS,
            _descriptor: PhantomData,
        }
    }
}

impl<D, S, C> ReceiptValidator<D, S, C>
where
    D: ActionDescriptor,
    S: ReplayStore,
    C: Clock,
{
    /// Override the maximum receipt age. Default is 60 seconds.
    /// Receipts whose `executed_at` is older than `now - max_age`
    /// are rejected as stale.
    pub fn with_max_age(mut self, max_age: Duration) -> Self {
        self.max_age_secs = max_age.as_secs();
        self
    }

    /// Override the clock. Tests inject `FixedClock`; production uses
    /// the default `SystemClock`.
    pub fn with_clock<C2: Clock>(self, clock: C2) -> ReceiptValidator<D, S, C2> {
        ReceiptValidator {
            executor_key: self.executor_key,
            replay: self.replay,
            clock,
            max_age_secs: self.max_age_secs,
            _descriptor: PhantomData,
        }
    }

    /// Run the full validation pipeline. See module-level docs for the
    /// six checks performed.
    pub async fn validate(&self, receipt_bytes: &[u8]) -> Result<ValidatedReceipt<D>, ValidationError> {
        // 1. Parse.
        let receipt: Receipt = serde_json::from_slice(receipt_bytes)
            .map_err(|e| ValidationError::Malformed(e.to_string()))?;

        // 2. Trusted executor (constant-time pubkey compare).
        if !pubkey_constant_time_eq(
            &receipt.executor.key_data,
            &self.executor_key.key.key_data,
        ) || receipt.executor.algorithm != self.executor_key.key.algorithm
        {
            return Err(ValidationError::UntrustedExecutor);
        }

        // 3. Signature.
        receipt
            .verify_signature()
            .map_err(|e| ValidationError::InvalidSignature(e.to_string()))?;

        // 4. Decode descriptor from action.value.
        let descriptor = D::from_action_value(&receipt.action.value)
            .map_err(|e| ValidationError::MalformedDescriptor(e.to_string()))?;

        // 5. Freshness.
        let now = self.clock.now();
        let executed_at = receipt.executed_at;
        if executed_at > now {
            return Err(ValidationError::FutureTimestamp {
                executed_at,
                now,
            });
        }
        let age = now - executed_at;
        if age > self.max_age_secs {
            return Err(ValidationError::TooOld {
                age_seconds: age,
                max_age_seconds: self.max_age_secs,
            });
        }

        // 6. Replay protection.
        let newly_consumed = self
            .replay
            .try_consume(&receipt.id.0)
            .await
            .map_err(|e| ValidationError::Storage(e.to_string()))?;
        if !newly_consumed {
            return Err(ValidationError::Replay);
        }

        Ok(ValidatedReceipt {
            receipt,
            descriptor,
        })
    }
}

/// Result of a successful validation. Caller does the resource-specific
/// match against `descriptor` before fulfilling.
#[derive(Debug, Clone)]
pub struct ValidatedReceipt<D> {
    pub receipt: Receipt,
    pub descriptor: D,
}

/// Validation outcome on the failure path.
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("receipt JSON malformed: {0}")]
    Malformed(String),
    #[error("receipt signed by an executor we don't trust")]
    UntrustedExecutor,
    #[error("receipt signature is invalid: {0}")]
    InvalidSignature(String),
    #[error("descriptor decoding failed: {0}")]
    MalformedDescriptor(String),
    #[error("receipt executed_at ({executed_at}) is after now ({now})")]
    FutureTimestamp { executed_at: u64, now: u64 },
    #[error("receipt too old: {age_seconds}s (limit {max_age_seconds}s)")]
    TooOld { age_seconds: u64, max_age_seconds: u64 },
    #[error("receipt already used (replay attempt)")]
    Replay,
    #[error("storage error: {0}")]
    Storage(String),
}

/// Constant-time byte slice comparison for public keys. Defends against
/// timing oracles when distinguishing trusted from untrusted issuers.
fn pubkey_constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use maat::{
        Action, Anchor, Contingency, Delegation, Keypair, Outcome, Receipt, ScopeExpr, ValueClaim,
    };
    use maat_sdk_core::time::FixedClock;

    use crate::replay::MemoryReplayStore;

    /// Helper: construct a freshly signed receipt for tests.
    fn mint_test_receipt(
        executor: &Keypair,
        principal: &Keypair,
        agent: &Keypair,
        executed_at: u64,
        action_value: Vec<u8>,
    ) -> Receipt {
        let d = Delegation::builder(
            agent.public_key.clone(),
            ScopeExpr::new(vec!["test:action".into()]),
        )
        .not_before(executed_at - 100)
        .not_after(executed_at + 3600)
        .build(principal)
        .unwrap();

        let anchor = Anchor::builder(d.id.clone())
            .max_staleness(300)
            .contingency(Contingency::Abort)
            .created_at(executed_at - 5)
            .build(agent)
            .unwrap();

        let action = Action {
            scope_used: "test:action".into(),
            description: "test".into(),
            value: action_value,
        };

        Receipt::new_deterministic(
            executor,
            d.id.clone(),
            anchor.id.clone(),
            action,
            Outcome::Success,
            None,
            vec![],
            maat::Nonce([0u8; 16]),
            executed_at,
        )
        .expect("receipt build")
    }

    fn make_validator<D: ActionDescriptor>(
        executor_pubkey: maat::PublicKey,
        clock_now: u64,
        max_age: u64,
    ) -> ReceiptValidator<D, MemoryReplayStore, FixedClock> {
        ReceiptValidator::<D, _>::new(
            ExecutorKey {
                key: executor_pubkey,
                key_id: "test-key".into(),
                tenant_id: uuid::Uuid::nil(),
            },
            MemoryReplayStore::new(),
        )
        .with_max_age(Duration::from_secs(max_age))
        .with_clock(FixedClock(clock_now))
    }

    #[tokio::test]
    async fn happy_path_validates() {
        let executor = Keypair::generate();
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let now = 2_000_000;

        let claim = ValueClaim {
            currency: "USD".into(),
            amount: 1234,
            decimals: 2,
        };
        let action_value = claim.to_action_value().unwrap();

        let receipt = mint_test_receipt(&executor, &principal, &agent, now - 1, action_value);
        let bytes = serde_json::to_vec(&receipt).unwrap();

        let validator = make_validator::<ValueClaim>(executor.public_key.clone(), now, 60);
        let validated = validator.validate(&bytes).await.unwrap();
        assert_eq!(validated.descriptor.amount, 1234);
    }

    #[tokio::test]
    async fn rejects_untrusted_executor() {
        let real_executor = Keypair::generate();
        let attacker = Keypair::generate();
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let now = 2_000_000;

        let claim = ValueClaim {
            currency: "USD".into(),
            amount: 100,
            decimals: 2,
        };
        // Attacker mints a receipt with their own key.
        let receipt = mint_test_receipt(
            &attacker,
            &principal,
            &agent,
            now - 1,
            claim.to_action_value().unwrap(),
        );
        let bytes = serde_json::to_vec(&receipt).unwrap();

        // Validator only trusts `real_executor`.
        let validator = make_validator::<ValueClaim>(real_executor.public_key.clone(), now, 60);
        let result = validator.validate(&bytes).await;
        assert!(matches!(result, Err(ValidationError::UntrustedExecutor)));
    }

    #[tokio::test]
    async fn rejects_old_receipt() {
        let executor = Keypair::generate();
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let now = 2_000_000;

        let claim = ValueClaim {
            currency: "USD".into(),
            amount: 100,
            decimals: 2,
        };
        // Receipt is 200 seconds old; max_age is 60.
        let receipt = mint_test_receipt(
            &executor,
            &principal,
            &agent,
            now - 200,
            claim.to_action_value().unwrap(),
        );
        let bytes = serde_json::to_vec(&receipt).unwrap();

        let validator = make_validator::<ValueClaim>(executor.public_key.clone(), now, 60);
        let result = validator.validate(&bytes).await;
        assert!(matches!(result, Err(ValidationError::TooOld { .. })));
    }

    #[tokio::test]
    async fn rejects_future_timestamp() {
        let executor = Keypair::generate();
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let now = 2_000_000;

        let claim = ValueClaim {
            currency: "USD".into(),
            amount: 100,
            decimals: 2,
        };
        // executed_at is 100s in the future.
        let receipt = mint_test_receipt(
            &executor,
            &principal,
            &agent,
            now + 100,
            claim.to_action_value().unwrap(),
        );
        let bytes = serde_json::to_vec(&receipt).unwrap();

        let validator = make_validator::<ValueClaim>(executor.public_key.clone(), now, 60);
        let result = validator.validate(&bytes).await;
        assert!(matches!(result, Err(ValidationError::FutureTimestamp { .. })));
    }

    #[tokio::test]
    async fn rejects_replay() {
        let executor = Keypair::generate();
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let now = 2_000_000;

        let claim = ValueClaim {
            currency: "USD".into(),
            amount: 100,
            decimals: 2,
        };
        let receipt = mint_test_receipt(
            &executor,
            &principal,
            &agent,
            now - 1,
            claim.to_action_value().unwrap(),
        );
        let bytes = serde_json::to_vec(&receipt).unwrap();

        let validator = make_validator::<ValueClaim>(executor.public_key.clone(), now, 60);
        // First use succeeds.
        validator.validate(&bytes).await.unwrap();
        // Second use is a replay.
        let result = validator.validate(&bytes).await;
        assert!(matches!(result, Err(ValidationError::Replay)));
    }

    #[tokio::test]
    async fn rejects_malformed_descriptor() {
        let executor = Keypair::generate();
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let now = 2_000_000;

        // Action.value is bytes that don't parse as ValueClaim.
        let receipt = mint_test_receipt(
            &executor,
            &principal,
            &agent,
            now - 1,
            b"not a value claim".to_vec(),
        );
        let bytes = serde_json::to_vec(&receipt).unwrap();

        let validator = make_validator::<ValueClaim>(executor.public_key.clone(), now, 60);
        let result = validator.validate(&bytes).await;
        assert!(matches!(result, Err(ValidationError::MalformedDescriptor(_))));
    }

    #[test]
    fn pubkey_constant_time_eq_correct() {
        assert!(pubkey_constant_time_eq(b"abc", b"abc"));
        assert!(!pubkey_constant_time_eq(b"abc", b"abd"));
        assert!(!pubkey_constant_time_eq(b"abc", b"abcd"));
        assert!(!pubkey_constant_time_eq(b"", b"a"));
        assert!(pubkey_constant_time_eq(b"", b""));
    }
}
