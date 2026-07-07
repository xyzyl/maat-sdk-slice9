//! Scope builder for hierarchical scope expressions.
//!
//! The protocol's scope expressions are pairs of grant/deny atom lists.
//! [`ScopeBuilder`] is the SDK's ergonomic wrapper for fluently composing
//! a `ScopeExpr` when issuing delegations.
//!
//! ## Example
//!
//! ```
//! use maat_sdk_core::ScopeBuilder;
//!
//! let scope = ScopeBuilder::new()
//!     .grant(["commerce:purchase:execute", "commerce:catalog:read"])
//!     .deny(["commerce:purchase:execute:luxury"])
//!     .build();
//!
//! assert_eq!(scope.grant.len(), 2);
//! assert_eq!(scope.deny.len(), 1);
//! ```

use maat::ScopeExpr;

/// Fluent builder for a [`ScopeExpr`].
#[derive(Debug, Default, Clone)]
pub struct ScopeBuilder {
    grants: Vec<String>,
    denies: Vec<String>,
}

impl ScopeBuilder {
    pub fn new() -> Self {
        ScopeBuilder::default()
    }

    /// Add granted scope atoms. Multiple calls accumulate.
    pub fn grant(
        mut self,
        atoms: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.grants.extend(atoms.into_iter().map(Into::into));
        self
    }

    /// Add denied scope atoms (carve-outs from the granted set).
    /// Multiple calls accumulate.
    pub fn deny(
        mut self,
        atoms: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.denies.extend(atoms.into_iter().map(Into::into));
        self
    }

    /// Construct the final [`ScopeExpr`].
    pub fn build(self) -> ScopeExpr {
        ScopeExpr {
            grant: self.grants,
            deny: self.denies,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_builder_is_well_formed() {
        let s = ScopeBuilder::new().build();
        assert!(s.grant.is_empty());
        assert!(s.deny.is_empty());
    }

    #[test]
    fn grants_accumulate_across_calls() {
        let s = ScopeBuilder::new()
            .grant(["a:b:c"])
            .grant(["d:e:f", "g:h:i"])
            .build();
        assert_eq!(s.grant, vec!["a:b:c", "d:e:f", "g:h:i"]);
    }

    #[test]
    fn grant_and_deny_independent() {
        let s = ScopeBuilder::new()
            .grant(["commerce:purchase:execute"])
            .deny(["commerce:purchase:execute:luxury"])
            .build();
        assert_eq!(s.grant.len(), 1);
        assert_eq!(s.deny.len(), 1);
    }
}
