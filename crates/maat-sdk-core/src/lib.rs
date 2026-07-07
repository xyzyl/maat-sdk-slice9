//! # `maat-sdk-core` — the grammar
//!
//! Shared abstractions used by both [`maat-agent`] (issuance + verification
//! verbs) and [`maat-resource`] (validation primitives). This crate has no
//! network, no I/O, no async runtime. It is the protocol's grammar made
//! ergonomic.
//!
//! Four modules:
//!
//! - [`descriptor`] — the [`ActionDescriptor`] trait. Any structured
//!   commitment an agent makes about an action. The protocol's `value_claim`
//!   is one impl; integrators define their own for non-monetary actions
//!   (recipient lists, deploy targets, refund order IDs, anything).
//! - [`constraints`] — the [`ConstraintSet`] builder. Fluent composition
//!   of the protocol's full constraint vocabulary plus a [`CustomConstraint`]
//!   trait for domain-specific extensions.
//! - [`scope`] — the [`ScopeBuilder`]. Hierarchical scope expressions for
//!   delegation issuance.
//! - [`error`] — [`SdkError`], the shared error taxonomy used by both
//!   leaf crates.
//! - [`time`] — the [`Clock`] trait. Allows tests to inject deterministic
//!   time without changing API shape.
//!
//! ## Design lens
//!
//! This SDK does not bake in any particular shape of authorization. The
//! protocol is a grammar; this crate exposes its compositional primitives.
//! Integrators write their own sentences in that grammar, including ones
//! the SDK's authors have not imagined.

pub mod constraints;
pub mod descriptor;
pub mod error;
pub mod scope;
pub mod time;

pub use constraints::{ConstraintSet, CustomConstraint};
pub use descriptor::{ActionDescriptor, DescriptorError, JsonDescriptor};
pub use error::SdkError;
pub use scope::ScopeBuilder;
pub use time::{Clock, FixedClock, SystemClock};
