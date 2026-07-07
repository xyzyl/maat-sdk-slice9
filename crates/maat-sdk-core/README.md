# `maat-sdk-core`

Shared core for the Maat SDK. Action descriptors, constraint composition, scope expressions, shared errors. No network, no I/O.

Both [`maat-agent`](../maat-agent) and [`maat-resource`](../maat-resource) depend on this crate.

## What's here

- **[`ActionDescriptor`](src/descriptor.rs)** — the trait. Any structured commitment an agent makes about an action. The protocol's `ValueClaim` is one impl. `JsonDescriptor<T>` is a generic adapter for any `serde::Serialize + DeserializeOwned` type. Manual implementations are also straightforward.
- **[`ConstraintSet`](src/constraints.rs)** — fluent builder over the protocol's constraint vocabulary (`max_value`, `max_rate`, `domain_allow`, `domain_deny`, `require_anchor_freshness`, `require_human_confirm`, `custom`).
- **[`CustomConstraint`](src/constraints.rs)** — trait for domain-specific constraints. Note: gateway-side enforcement of custom constraints is a future slice; currently the gateway fails closed on unknown ones.
- **[`ScopeBuilder`](src/scope.rs)** — fluent builder over `ScopeExpr`.
- **[`SdkError`](src/error.rs)** — the shared error taxonomy.
- **[`Clock`](src/time.rs)** — a trait with `SystemClock` default and `FixedClock` for tests.

## Quick example

```rust
use std::time::Duration;
use maat_sdk_core::{ActionDescriptor, ConstraintSet, JsonDescriptor, ScopeBuilder};
use serde::{Serialize, Deserialize};

// Compose a constraint set for "send up to 100 emails/day,
// only to acme.com domains, message hash bound at issue time."
let constraints = ConstraintSet::new()
    .max_rate(100, Duration::from_secs(86_400))
    .domain_allow(["acme.com"])
    .into_vec();

// Compose a scope expression.
let scope = ScopeBuilder::new()
    .grant(["messaging:email:send"])
    .build();

// Define a custom action descriptor.
#[derive(Serialize, Deserialize, Clone)]
struct EmailDescriptor {
    recipients: Vec<String>,
    content_hash: [u8; 32],
}

let descriptor = JsonDescriptor::new(EmailDescriptor {
    recipients: vec!["alice@acme.com".into()],
    content_hash: [0; 32],
});
let bytes = descriptor.to_action_value().unwrap();
```

These primitives compose with the agent and resource crates to express any action shape you care about.
