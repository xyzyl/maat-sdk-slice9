//! Constraint composition.
//!
//! The protocol's [`maat::Constraint`] enum is the source of truth.
//! [`ConstraintSet`] is the SDK's ergonomic wrapper for fluently composing
//! a set of constraints when issuing delegations.
//!
//! ## Example
//!
//! ```
//! use std::time::Duration;
//! use maat_sdk_core::ConstraintSet;
//!
//! let constraints = ConstraintSet::new()
//!     .max_value("USD", 5000, 2)              // $50.00 cap
//!     .max_rate(10, Duration::from_secs(60))   // 10 actions / minute
//!     .domain_allow(["api.acme.com"])
//!     .require_anchor_freshness(Duration::from_secs(60))
//!     .into_vec();
//!
//! assert_eq!(constraints.len(), 4);
//! ```
//!
//! ## Custom constraints
//!
//! The protocol's `Custom { type_uri, value }` variant allows
//! domain-specific constraints not covered by the standard vocabulary.
//! Implement [`CustomConstraint`] for your type and use
//! [`ConstraintSet::custom`].
//!
//! **Important caveat:** the gateway does not yet evaluate custom
//! constraints (it correctly fails closed on unknown ones per protocol).
//! Issuing a delegation with a custom constraint will result in the
//! gateway rejecting any action against that delegation. Custom-constraint
//! enforcement at the gateway is a future slice. The trait exists in this
//! SDK so integrators can define their constraint shapes today, and so
//! resource-side code that wants to inspect custom constraints (e.g., for
//! audit display) has a typed surface.

use std::time::Duration;

use maat::Constraint;

use crate::descriptor::DescriptorError;

/// Fluent builder for a set of [`maat::Constraint`]s.
#[derive(Debug, Default, Clone)]
pub struct ConstraintSet {
    constraints: Vec<Constraint>,
}

impl ConstraintSet {
    pub fn new() -> Self {
        ConstraintSet {
            constraints: Vec::new(),
        }
    }

    /// `max_value`: the agent must include a `value_claim` whose
    /// `currency`, `decimals` match exactly and `amount` does not exceed
    /// `amount`.
    pub fn max_value(
        mut self,
        currency: impl Into<String>,
        amount: u64,
        decimals: u8,
    ) -> Self {
        self.constraints.push(Constraint::MaxValue {
            currency: currency.into(),
            amount,
            decimals,
        });
        self
    }

    /// `max_rate`: at most `count` actions in any sliding window of
    /// `period`.
    pub fn max_rate(mut self, count: u32, period: Duration) -> Self {
        self.constraints.push(Constraint::MaxRate {
            count,
            period_seconds: period.as_secs(),
        });
        self
    }

    /// `domain_allow`: action's `domain` field MUST be in this list.
    /// If a `domain_allow` constraint is present and no domain is
    /// provided in the request, the request fails closed.
    pub fn domain_allow(
        mut self,
        domains: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.constraints.push(Constraint::DomainAllow {
            domains: domains.into_iter().map(Into::into).collect(),
        });
        self
    }

    /// `domain_deny`: action's `domain` field MUST NOT be in this list.
    pub fn domain_deny(
        mut self,
        domains: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.constraints.push(Constraint::DomainDeny {
            domains: domains.into_iter().map(Into::into).collect(),
        });
        self
    }

    /// `require_anchor_freshness`: anchor must be no older than `max_age`.
    pub fn require_anchor_freshness(mut self, max_age: Duration) -> Self {
        self.constraints.push(Constraint::RequireAnchorFreshness {
            max_age_seconds: max_age.as_secs(),
        });
        self
    }

    /// `require_human_confirm`: agent must obtain explicit human
    /// confirmation, threshold-bound.
    pub fn require_human_confirm(mut self, threshold: impl Into<String>) -> Self {
        self.constraints.push(Constraint::RequireHumanConfirm {
            threshold: threshold.into(),
        });
        self
    }

    /// Add a custom constraint. The constraint type's evaluation is the
    /// verifier's responsibility; verifiers that don't recognize the
    /// `type_uri` MUST fail closed per protocol.
    pub fn custom<C: CustomConstraint>(mut self, constraint: C) -> Result<Self, DescriptorError> {
        self.constraints.push(constraint.into_constraint()?);
        Ok(self)
    }

    /// Append a raw [`Constraint`]. Escape hatch for cases where the
    /// builder methods don't fit (e.g., constructing constraints from
    /// dynamically-sourced data).
    pub fn add_raw(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Consume and return the underlying constraint vector.
    pub fn into_vec(self) -> Vec<Constraint> {
        self.constraints
    }

    /// Borrow the constraint vector.
    pub fn as_slice(&self) -> &[Constraint] {
        &self.constraints
    }

    /// Return the number of constraints in the set.
    pub fn len(&self) -> usize {
        self.constraints.len()
    }

    /// True when no constraints have been added.
    pub fn is_empty(&self) -> bool {
        self.constraints.is_empty()
    }
}

// ─── CustomConstraint trait ────────────────────────────────────────────────

/// Domain-specific constraint that ships in `Constraint::Custom`.
///
/// Implementations specify a `TYPE_URI` (a stable identifier — convention
/// is reverse-DNS or a URI), and define how to encode/decode the
/// constraint's parameters as bytes.
///
/// ## Example
///
/// ```
/// use serde::{Serialize, Deserialize};
/// use maat_sdk_core::{ConstraintSet, CustomConstraint, DescriptorError};
/// use maat::Constraint;
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct OrderMaxAge {
///     max_age_days: u32,
/// }
///
/// impl CustomConstraint for OrderMaxAge {
///     const TYPE_URI: &'static str = "com.acme/order_max_age/v1";
///
///     fn to_value(&self) -> Result<Vec<u8>, DescriptorError> {
///         serde_json::to_vec(self)
///             .map_err(|e| DescriptorError::Encode(e.to_string()))
///     }
///
///     fn from_value(bytes: &[u8]) -> Result<Self, DescriptorError> {
///         serde_json::from_slice(bytes)
///             .map_err(|e| DescriptorError::Decode(e.to_string()))
///     }
/// }
///
/// let constraints = ConstraintSet::new()
///     .custom(OrderMaxAge { max_age_days: 30 })
///     .unwrap()
///     .into_vec();
/// assert_eq!(constraints.len(), 1);
/// ```
pub trait CustomConstraint: Sized {
    /// Stable identifier for this constraint type.
    const TYPE_URI: &'static str;

    /// Encode this constraint's parameters as bytes.
    fn to_value(&self) -> Result<Vec<u8>, DescriptorError>;

    /// Decode bytes back into this constraint.
    fn from_value(bytes: &[u8]) -> Result<Self, DescriptorError>;

    /// Convert into the protocol's `Constraint::Custom` variant.
    fn into_constraint(self) -> Result<Constraint, DescriptorError> {
        let value = self.to_value()?;
        Ok(Constraint::Custom {
            type_uri: Self::TYPE_URI.to_string(),
            value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[test]
    fn empty_set_works() {
        let cs = ConstraintSet::new();
        assert!(cs.is_empty());
        assert_eq!(cs.into_vec(), Vec::new());
    }

    #[test]
    fn fluent_composition() {
        let cs = ConstraintSet::new()
            .max_value("USD", 5000, 2)
            .max_rate(10, Duration::from_secs(60))
            .domain_allow(["api.acme.com", "api.zapier.com"])
            .domain_deny(["api.bad.com"])
            .require_anchor_freshness(Duration::from_secs(60))
            .require_human_confirm("amount > 1000");
        let v = cs.into_vec();
        assert_eq!(v.len(), 6);
    }

    #[test]
    fn max_rate_converts_period_correctly() {
        let cs = ConstraintSet::new().max_rate(5, Duration::from_secs(120));
        match &cs.as_slice()[0] {
            Constraint::MaxRate { count, period_seconds } => {
                assert_eq!(*count, 5);
                assert_eq!(*period_seconds, 120);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TickerAllowList {
        tickers: Vec<String>,
    }

    impl CustomConstraint for TickerAllowList {
        const TYPE_URI: &'static str = "test.example/ticker_allow_list/v1";

        fn to_value(&self) -> Result<Vec<u8>, DescriptorError> {
            serde_json::to_vec(self).map_err(|e| DescriptorError::Encode(e.to_string()))
        }

        fn from_value(bytes: &[u8]) -> Result<Self, DescriptorError> {
            serde_json::from_slice(bytes).map_err(|e| DescriptorError::Decode(e.to_string()))
        }
    }

    #[test]
    fn custom_constraint_round_trips() {
        let original = TickerAllowList {
            tickers: vec!["AAPL".into(), "GOOG".into()],
        };
        let bytes = original.to_value().unwrap();
        let recovered = TickerAllowList::from_value(&bytes).unwrap();
        assert_eq!(original, recovered);
    }

    #[test]
    fn custom_constraint_into_protocol_form() {
        let custom = TickerAllowList {
            tickers: vec!["TSLA".into()],
        };
        let cs = ConstraintSet::new().custom(custom).unwrap();
        let v = cs.into_vec();
        assert_eq!(v.len(), 1);
        match &v[0] {
            Constraint::Custom { type_uri, value: _ } => {
                assert_eq!(type_uri, "test.example/ticker_allow_list/v1");
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn add_raw_works_for_escape_hatch() {
        let cs = ConstraintSet::new().add_raw(Constraint::MaxValue {
            currency: "EUR".into(),
            amount: 100,
            decimals: 2,
        });
        assert_eq!(cs.len(), 1);
    }
}
