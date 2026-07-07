# Maat SDK

Three Rust crates that make the [Maat protocol](../maat/SPEC.md) integrable. The protocol library `maat` defines what's cryptographically true; this SDK exposes those truths as composable primitives.

## Crates

- **[`maat-sdk-core`](crates/maat-sdk-core)** — the grammar. Action descriptors, constraint composition, scope expressions, shared errors. No network, no I/O. Both leaf crates depend on this.
- **[`maat-agent`](crates/maat-agent)** — issuance and verification verbs. Identity, delegation builders (single-level and sub-delegation), anchor construction, the HTTP client to the gateway's `/v1/verify` endpoint.
- **[`maat-resource`](crates/maat-resource)** — validation primitives. Executor key fetcher, receipt validator (signature + freshness + replay), replay-store trait.

```
maat-sdk-core
   ↑                ↑
maat-agent     maat-resource
```

Neither leaf crate depends on the other.

## Design lens

The protocol is a grammar. Three primitives — Delegation, Anchor, Receipt — composed via an open vocabulary of constraints and structured action commitments. Many distinct production scenarios (commerce, refunds, ops deploys, messaging, data exports, treasury wires) are sentences in this grammar, not separate engineering problems.

The SDK exposes the grammar at maximum generality. It does not bake in commerce. It does not bake in any particular shape of authorization. It exposes the protocol's compositional primitives such that integrators write their own sentences — including ones that haven't been thought of yet.

## What this SDK does NOT do

- **Persistence backends.** Operators bring storage. SDK exposes traits (`DelegationStore`, `ReplayStore`) and ships in-memory implementations for testing.
- **Strategy or business logic.** What action to take next, when to retry, how to compose UI flows — operator concerns.
- **Resource-specific value matching.** The validator confirms cryptographic and protocol-level properties; matching a receipt's value claim against your cart total is operator code (the SDK has no idea what your cart looks like).
- **Multi-language ports.** Rust SDK is canonical. TypeScript and Python ports come later.

## Dependency on the protocol library

All three crates use `maat = { path = "../maat" }`. The expected directory layout:

```
~/code/
├── maat/                  # the protocol library, 0.4.0
└── maat-sdk-slice9/       # this workspace
    ├── Cargo.toml
    └── crates/
        ├── maat-sdk-core/
        ├── maat-agent/
        └── maat-resource/
```

Adjust the path in `Cargo.toml` if your layout differs.

## Status

Slice 9 in-workspace. Not yet published to crates.io. Demos that exercise this SDK across multiple authorization shapes (commerce, messaging, ops) come in Slice 10.
