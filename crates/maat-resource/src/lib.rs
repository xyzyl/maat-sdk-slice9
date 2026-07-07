//! # `maat-resource` — validation primitives
//!
//! Resource-side primitives for the Maat protocol. Use this crate when you
//! are building a service that honors Maat receipts.
//!
//! ## Module layout
//!
//! - [`executor_key`] — fetch and cache the gateway's executor public key
//!   ([`ExecutorKey`], [`fetch_executor_key`]).
//! - [`replay`] — atomic replay protection ([`ReplayStore`] trait,
//!   [`MemoryReplayStore`] for tests).
//! - [`validator`] — the central [`ReceiptValidator`].
//!
//! ## Quick example
//!
//! ```no_run
//! use std::time::Duration;
//! use maat::ValueClaim;
//! use maat_resource::{ReceiptValidator, MemoryReplayStore, fetch_executor_key};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! // At startup:
//! let executor_key = fetch_executor_key("http://gateway:8080", "tenant-api-key").await?;
//! let validator = ReceiptValidator::<ValueClaim, _>::new(
//!     executor_key,
//!     MemoryReplayStore::new(),
//! )
//! .with_max_age(Duration::from_secs(60));
//!
//! // Per request:
//! # let receipt_bytes: &[u8] = &[];
//! let validated = validator.validate(receipt_bytes).await?;
//!
//! // Operator's own check: does the descriptor match the resource's state?
//! // (e.g., does validated.descriptor.amount equal the cart total?)
//! # Ok(())
//! # }
//! ```

pub mod executor_key;
pub mod replay;
pub mod validator;

pub use executor_key::{fetch_executor_key, ExecutorKey};
pub use replay::{MemoryReplayStore, ReplayStore};
pub use validator::{ReceiptValidator, ValidatedReceipt, ValidationError};

// Re-export the most-used core types so integrators can `use maat_resource::*;`
// for the common path.
pub use maat_sdk_core::{
    ActionDescriptor, Clock, JsonDescriptor, SdkError, SystemClock,
};
