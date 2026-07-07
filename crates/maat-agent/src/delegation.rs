//! Delegation issuance and storage.
//!
//! This module collects three concerns:
//!
//! - [`DelegationStore`] — trait for persisting "the delegation this agent
//!   currently operates under." Operators implement against their database;
//!   the SDK ships [`MemoryDelegationStore`] for tests.
//! - [`DelegationBuilder`] — fluent wrapper over the protocol library's
//!   builder, integrated with [`ConstraintSet`] and [`ScopeBuilder`] from
//!   `maat-sdk-core` so issuance reads as one composition.
//! - [`SubDelegationBuilder`] — for hierarchical delegation. Issues a
//!   child delegation that attenuates a parent. Validates attenuation
//!   rules at build time.

use std::time::Duration;

use async_trait::async_trait;
use parking_lot::RwLock;

use maat::{Delegation, PublicKey, ScopeExpr, Signer};
use maat_sdk_core::{ConstraintSet, SdkError};

// ─── DelegationStore trait ────────────────────────────────────────────────

/// Persistent store for the delegation an agent is currently operating
/// under.
///
/// Implementations supply atomic semantics: if `set` returns Ok, a
/// subsequent `current` MUST observe the new delegation.
#[async_trait]
pub trait DelegationStore: Send + Sync {
    async fn current(&self) -> Result<Option<Delegation>, SdkError>;
    async fn set(&self, delegation: Delegation) -> Result<(), SdkError>;
    async fn clear(&self) -> Result<(), SdkError>;
}

/// In-memory delegation store. For tests and short-lived agents.
#[derive(Debug, Default)]
pub struct MemoryDelegationStore {
    inner: RwLock<Option<Delegation>>,
}

impl MemoryDelegationStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DelegationStore for MemoryDelegationStore {
    async fn current(&self) -> Result<Option<Delegation>, SdkError> {
        Ok(self.inner.read().clone())
    }

    async fn set(&self, delegation: Delegation) -> Result<(), SdkError> {
        *self.inner.write() = Some(delegation);
        Ok(())
    }

    async fn clear(&self) -> Result<(), SdkError> {
        *self.inner.write() = None;
        Ok(())
    }
}

// ─── DelegationBuilder ─────────────────────────────────────────────────────

/// Fluent builder for a top-level delegation.
///
/// ## Example
///
/// ```no_run
/// use std::time::Duration;
/// use maat::Keypair;
/// use maat_agent::{DelegationBuilder};
/// use maat_sdk_core::{ConstraintSet, ScopeBuilder};
///
/// let principal = Keypair::generate();
/// let agent = Keypair::generate();
///
/// let delegation = DelegationBuilder::new(agent.public_key.clone())
///     .scope(ScopeBuilder::new().grant(["commerce:purchase:execute"]).build())
///     .constraints(ConstraintSet::new().max_value("USD", 5000, 2))
///     .valid_for(Duration::from_secs(3600))
///     .build(&principal)
///     .unwrap();
/// ```
pub struct DelegationBuilder {
    agent: PublicKey,
    scope: Option<ScopeExpr>,
    constraints: Option<ConstraintSet>,
    not_before: Option<u64>,
    not_after: Option<u64>,
    max_depth: Option<u8>,
}

impl DelegationBuilder {
    /// Start building a delegation for the given agent public key.
    pub fn new(agent: PublicKey) -> Self {
        DelegationBuilder {
            agent,
            scope: None,
            constraints: None,
            not_before: None,
            not_after: None,
            max_depth: None,
        }
    }

    /// Set the scope expression. Required.
    pub fn scope(mut self, scope: ScopeExpr) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Set the constraint set. Defaults to empty.
    pub fn constraints(mut self, constraints: ConstraintSet) -> Self {
        self.constraints = Some(constraints);
        self
    }

    /// Set `not_before` to a specific Unix-second timestamp.
    pub fn not_before(mut self, t: u64) -> Self {
        self.not_before = Some(t);
        self
    }

    /// Set `not_after` to a specific Unix-second timestamp.
    pub fn not_after(mut self, t: u64) -> Self {
        self.not_after = Some(t);
        self
    }

    /// Convenience: set `not_before = now()` and `not_after = now() + duration`.
    /// Reads the system clock at build time.
    pub fn valid_for(mut self, duration: Duration) -> Self {
        let now = maat::types::now().expect("system clock");
        self.not_before = Some(now);
        self.not_after = Some(now + duration.as_secs());
        self
    }

    /// Set the maximum sub-delegation depth.
    pub fn max_depth(mut self, depth: u8) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Build and sign with the given principal signer.
    pub fn build<S: Signer>(self, principal: &S) -> Result<Delegation, SdkError> {
        let scope = self
            .scope
            .ok_or_else(|| SdkError::Config("scope is required".into()))?;
        let constraints = self
            .constraints
            .map(ConstraintSet::into_vec)
            .unwrap_or_default();

        let mut b = Delegation::builder(self.agent, scope).constraints(constraints);
        if let Some(t) = self.not_before {
            b = b.not_before(t);
        }
        if let Some(t) = self.not_after {
            b = b.not_after(t);
        }
        if let Some(d) = self.max_depth {
            b = b.max_depth(d);
        }

        b.build(principal)
            .map_err(|e| SdkError::Protocol(e.to_string()))
    }
}

// ─── SubDelegationBuilder ──────────────────────────────────────────────────

/// Fluent builder for a sub-delegation.
///
/// A sub-delegation must attenuate its parent: scope must be contained,
/// constraints must be at-least-as-strict, validity window must be within
/// the parent's. The protocol library validates these at build time;
/// errors surface as [`SdkError::Protocol`].
///
/// Use the parent delegation as the seed and pass the chain (the full
/// root-to-parent chain) so the sub-delegation can be verified by anyone
/// who has the root.
pub struct SubDelegationBuilder {
    parent: Delegation,
    parent_chain: Vec<Delegation>,
    sub_agent: PublicKey,
    scope: Option<ScopeExpr>,
    constraints: Option<ConstraintSet>,
    not_before: Option<u64>,
    not_after: Option<u64>,
    max_depth: Option<u8>,
}

impl SubDelegationBuilder {
    /// Start building a sub-delegation. `parent` is the immediate
    /// parent; `parent_chain` is the full root-to-parent chain
    /// (including parent).
    pub fn new(parent: Delegation, parent_chain: Vec<Delegation>, sub_agent: PublicKey) -> Self {
        SubDelegationBuilder {
            parent,
            parent_chain,
            sub_agent,
            scope: None,
            constraints: None,
            not_before: None,
            not_after: None,
            max_depth: None,
        }
    }

    pub fn scope(mut self, scope: ScopeExpr) -> Self {
        self.scope = Some(scope);
        self
    }

    pub fn constraints(mut self, constraints: ConstraintSet) -> Self {
        self.constraints = Some(constraints);
        self
    }

    pub fn not_before(mut self, t: u64) -> Self {
        self.not_before = Some(t);
        self
    }

    pub fn not_after(mut self, t: u64) -> Self {
        self.not_after = Some(t);
        self
    }

    pub fn valid_for(mut self, duration: Duration) -> Self {
        let now = maat::types::now().expect("system clock");
        self.not_before = Some(now);
        self.not_after = Some(now + duration.as_secs());
        self
    }

    pub fn max_depth(mut self, depth: u8) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Inherit the parent's scope and constraints. Use with `narrow_*`
    /// helpers (or further `constraints` calls) to attenuate.
    pub fn inherit_from_parent(mut self) -> Self {
        if self.scope.is_none() {
            self.scope = Some(self.parent.scope.clone());
        }
        if self.constraints.is_none() {
            // Wrap the parent's constraint vector back into a ConstraintSet.
            let mut cs = ConstraintSet::new();
            for c in &self.parent.constraints {
                cs = cs.add_raw(c.clone());
            }
            self.constraints = Some(cs);
        }
        self
    }

    /// Build the sub-delegation, signed by the parent's agent.
    /// The parent's agent IS the issuer of the sub-delegation —
    /// the parent agent acts as the principal for the child.
    pub fn build<S: Signer>(self, parent_agent_signer: &S) -> Result<Delegation, SdkError> {
        let scope = self
            .scope
            .ok_or_else(|| SdkError::Config("scope is required".into()))?;
        let constraints = self
            .constraints
            .map(ConstraintSet::into_vec)
            .unwrap_or_default();

        let mut b = Delegation::builder(self.sub_agent, scope)
            .constraints(constraints)
            .parent(self.parent.id.clone(), self.parent_chain);

        if let Some(t) = self.not_before {
            b = b.not_before(t);
        }
        if let Some(t) = self.not_after {
            b = b.not_after(t);
        }
        if let Some(d) = self.max_depth {
            b = b.max_depth(d);
        }

        b.build(parent_agent_signer)
            .map_err(|e| SdkError::Protocol(e.to_string()))
    }
}

// ─── Acceptance: validating an inbound delegation ──────────────────────────

/// Validate a delegation an integrator wants to accept.
///
/// Confirms:
/// 1. The delegation's signature is valid.
/// 2. The delegation's `agent` public key matches the recipient's identity.
/// 3. The delegation has not yet expired.
///
/// On success, the delegation is safe to persist via [`DelegationStore::set`].
/// Returns an [`SdkError`] otherwise.
pub fn accept_delegation(
    delegation: &Delegation,
    recipient_pubkey: &PublicKey,
) -> Result<(), SdkError> {
    if delegation.agent.key_data != recipient_pubkey.key_data {
        return Err(SdkError::Config(
            "delegation agent pubkey does not match recipient identity".into(),
        ));
    }

    let now = maat::types::now().map_err(|e| SdkError::Protocol(e.to_string()))?;
    if delegation.not_after <= now {
        return Err(SdkError::Protocol(format!(
            "delegation has already expired (not_after={}, now={})",
            delegation.not_after, now
        )));
    }

    delegation
        .verify_signature()
        .map_err(|e| SdkError::Crypto(format!("delegation signature invalid: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentIdentity;
    use maat::Keypair;
    use maat_sdk_core::ScopeBuilder;

    #[tokio::test]
    async fn memory_store_round_trips() {
        let store = MemoryDelegationStore::new();
        assert!(store.current().await.unwrap().is_none());

        // Set / get / clear.
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let scope = ScopeBuilder::new().grant(["test:foo"]).build();
        let d = DelegationBuilder::new(agent.public_key.clone())
            .scope(scope)
            .valid_for(Duration::from_secs(3600))
            .build(&principal)
            .unwrap();

        store.set(d.clone()).await.unwrap();
        assert!(store.current().await.unwrap().is_some());

        store.clear().await.unwrap();
        assert!(store.current().await.unwrap().is_none());
    }

    #[test]
    fn delegation_builder_requires_scope() {
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let result = DelegationBuilder::new(agent.public_key.clone()).build(&principal);
        assert!(result.is_err());
    }

    #[test]
    fn delegation_builder_composes_constraints() {
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let d = DelegationBuilder::new(agent.public_key.clone())
            .scope(ScopeBuilder::new().grant(["test:action"]).build())
            .constraints(
                ConstraintSet::new()
                    .max_value("USD", 1000, 2)
                    .max_rate(10, Duration::from_secs(60)),
            )
            .valid_for(Duration::from_secs(3600))
            .build(&principal)
            .unwrap();
        assert_eq!(d.constraints.len(), 2);
    }

    #[test]
    fn accept_delegation_rejects_wrong_agent() {
        let principal = Keypair::generate();
        let agent = Keypair::generate();
        let other = Keypair::generate();

        let d = DelegationBuilder::new(agent.public_key.clone())
            .scope(ScopeBuilder::new().grant(["test:foo"]).build())
            .valid_for(Duration::from_secs(3600))
            .build(&principal)
            .unwrap();

        // The delegation is for `agent`; trying to accept as `other` must fail.
        let id_other = AgentIdentity::from_seed(other.secret_seed());
        let result = accept_delegation(&d, id_other.public_key());
        assert!(matches!(result, Err(SdkError::Config(_))));
    }

    #[test]
    fn accept_delegation_rejects_expired() {
        let principal = Keypair::generate();
        let agent = Keypair::generate();

        // Build a delegation that expired in the past.
        let now = maat::types::now().unwrap();
        let d = DelegationBuilder::new(agent.public_key.clone())
            .scope(ScopeBuilder::new().grant(["test:foo"]).build())
            .not_before(now - 100)
            .not_after(now - 1)
            .build(&principal)
            .unwrap();

        let id_agent = AgentIdentity::from_seed(agent.secret_seed());
        let result = accept_delegation(&d, id_agent.public_key());
        assert!(matches!(result, Err(SdkError::Protocol(_))));
    }

    #[test]
    fn sub_delegation_inherits_from_parent() {
        let principal = Keypair::generate();
        let agent_a = Keypair::generate();
        let agent_b = Keypair::generate();

        let parent = DelegationBuilder::new(agent_a.public_key.clone())
            .scope(ScopeBuilder::new().grant(["test:action"]).build())
            .constraints(ConstraintSet::new().max_value("USD", 1000, 2))
            .valid_for(Duration::from_secs(3600))
            .build(&principal)
            .unwrap();

        let sub = SubDelegationBuilder::new(
            parent.clone(),
            vec![parent.clone()],
            agent_b.public_key.clone(),
        )
        .inherit_from_parent()
        .valid_for(Duration::from_secs(1800))
        .build(&agent_a)
        .unwrap();

        // Sub should match parent's scope and constraints.
        assert_eq!(sub.scope.grant, parent.scope.grant);
        assert_eq!(sub.constraints.len(), parent.constraints.len());
    }
}
