# `maat-resource`

Resource-side primitives for the Maat protocol. Validate incoming receipts and unwrap typed action descriptors.

## What's here

- **[`fetch_executor_key`](src/executor_key.rs)** — call once at startup. Fetches and caches the gateway's trusted public key.
- **[`ReplayStore`](src/replay.rs)** — trait for atomic replay protection. `MemoryReplayStore` ships for tests; production uses your database.
- **[`ReceiptValidator<D>`](src/validator.rs)** — runs the protocol-level checks: signature, executor trust, freshness, replay, descriptor decode. Generic over the descriptor type `D`.

## What this crate does NOT do

- Compare the descriptor against the resource's own state. The validator returns a typed descriptor; matching it against your cart total / recipient allowlist / deploy permissions is operator code.
- Re-verify the delegation chain. The gateway already did that when it issued the receipt; the receipt's signature is the gateway's attestation.
- Fulfill the action. Operator code runs after `validate` returns Ok.

## Workflow

1. At startup, `fetch_executor_key(gateway_url, api_key)` once.
2. Construct a `ReceiptValidator` with the executor key and your `ReplayStore` impl.
3. Per inbound request: pull the receipt bytes from wherever the agent put them (HTTP header, message-queue payload, etc.), call `validator.validate(bytes)`.
4. On `Ok(validated)`, do your resource-specific match against `validated.descriptor`, then fulfill.
5. On `Err(_)`, reject the request.
