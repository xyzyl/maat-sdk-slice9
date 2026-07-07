//! # `maat-agent` — issuance and verification verbs
//!
//! Agent-side primitives for the Maat protocol. Use this crate when you
//! are building an autonomous agent that requests authorizations from a
//! Maat gateway, OR a principal-side tool that issues delegations.
//!
//! ## Module layout
//!
//! - [`identity`] — the agent's cryptographic identity ([`AgentIdentity`]).
//! - [`delegation`] — issuance ([`DelegationBuilder`], [`SubDelegationBuilder`]),
//!   acceptance ([`accept_delegation`]), and storage ([`DelegationStore`]).
//! - [`anchor`] — anchor construction ([`AnchorBuilder`], [`StateBinding`]).
//! - [`gateway`] — the HTTP client to the gateway ([`GatewayClient`]).
//!
//! ## Quick example
//!
//! ```no_run
//! use std::time::Duration;
//! use maat::verify::ActionRequest;
//! use maat::ValueClaim;
//! use maat_agent::{AgentIdentity, AnchorBuilder, GatewayClient, MemoryDelegationStore, DelegationStore};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! // One-time setup.
//! let identity = AgentIdentity::generate();
//! let store = MemoryDelegationStore::new();
//! let gateway = GatewayClient::new("http://gateway:8080", "tenant-api-key");
//!
//! // ... operator pastes a delegation and the agent accepts it ...
//! # let delegation: maat::Delegation = unimplemented!();
//! store.set(delegation.clone()).await?;
//!
//! // Build an anchor and an action request.
//! let anchor = AnchorBuilder::new(delegation.id.clone())
//!     .max_staleness(Duration::from_secs(60))
//!     .build(&identity)?;
//!
//! let request = ActionRequest {
//!     delegation: delegation.clone(),
//!     delegation_chain: vec![delegation.clone()],
//!     anchor,
//!     action_scope: "commerce:purchase:execute".into(),
//!     domain: None,
//!     value_claim: Some(ValueClaim {
//!         currency: "USD".into(),
//!         amount: 1999,
//!         decimals: 2,
//!     }),
//!     action_value: None,
//! };
//!
//! // Verify with the gateway. The descriptor (here the value claim itself)
//! // is encoded into the receipt.
//! let descriptor = ValueClaim {
//!     currency: "USD".into(),
//!     amount: 1999,
//!     decimals: 2,
//! };
//! let verified = gateway.verify_action(&request, descriptor).await?;
//!
//! // verified.receipt_json is signed bytes ready to forward to the resource.
//! # Ok(())
//! # }
//! ```

pub mod anchor;
pub mod delegation;
pub mod gateway;
pub mod identity;

pub use anchor::{AnchorBuilder, StateBinding};
pub use delegation::{
    accept_delegation, DelegationBuilder, DelegationStore, MemoryDelegationStore,
    SubDelegationBuilder,
};
pub use gateway::{GatewayClient, ReceiptFilter, VerifiedReceipt};
pub use identity::AgentIdentity;

// Re-export the most-used core types so integrators can `use maat_agent::*;`
// for the common path.
pub use maat_sdk_core::{
    ActionDescriptor, ConstraintSet, CustomConstraint, JsonDescriptor, ScopeBuilder, SdkError,
};
