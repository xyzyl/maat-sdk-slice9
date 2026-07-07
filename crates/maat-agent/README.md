# `maat-agent`

Agent-side primitives for the Maat protocol. Issue delegations, accept delegations, build anchors, talk to the gateway.

## What's here

- **[`AgentIdentity`](src/identity.rs)** — cryptographic identity for an autonomous agent. Wraps `maat::Keypair`. Persist via `secret_seed()`/`from_seed()`.
- **[`DelegationBuilder`](src/delegation.rs)** + **[`SubDelegationBuilder`](src/delegation.rs)** — fluent issuance, integrated with `ConstraintSet` and `ScopeBuilder` from `maat-sdk-core`.
- **[`accept_delegation`](src/delegation.rs)** — validate an inbound delegation (signature + agent pubkey match + expiry).
- **[`DelegationStore`](src/delegation.rs)** — trait for persisting the agent's current delegation. `MemoryDelegationStore` ships for testing.
- **[`AnchorBuilder`](src/anchor.rs)** — anchor construction with state-binding helpers (URL+hash, content hash, JSON document hash).
- **[`GatewayClient`](src/gateway.rs)** — HTTP client to `/v1/verify` and `/v1/receipts`. Generic over `ActionDescriptor`.

## Workflow

1. Generate or restore an `AgentIdentity`.
2. Operator (or parent agent) issues a delegation; agent accepts via `accept_delegation` and persists via `DelegationStore::set`.
3. For each action: build an anchor (`AnchorBuilder`), build an `ActionRequest`, call `gateway.verify_action(request, descriptor)`.
4. Forward `verified.receipt_json` to the resource using whatever transport it expects (HTTP header, gRPC metadata, message-queue payload).

## What this crate does NOT do

- Decide what action to take next (operator's strategy concern).
- Forward receipts to resources (transport varies; you write that part).
- Retry on gateway 5xx/429 (operator's policy; use `tower::retry` or similar).
