# Secured BTC Node API Plan

## Problem

The explorer needs an honest node-derived answer to "how much BTC is secured by Nomic?"

The current public surfaces do not provide that cleanly:

- `rest/src/main.rs` exposes per-address `usat` balances, but not a total `usat` supply.
- `rest/src/main.rs` bank supply routes are wired only to `InnerApp::total_supply()`, which is NOM-only.
- The generic `/query/<hex>` route is not a stable public contract. It requires Orga-encoded query bytes and is not something the explorer should reverse-engineer.
- The relayer's `/pending_deposits` route is relayer-local state on port `8999`, not a documented node LCD/API.

## Source Of Truth

The correct source of truth for total BTC secured by the bridge is the Bitcoin checkpoint reserve state, not token balances.

Why:

- `src/bitcoin/mod.rs` already defines `Bitcoin::value_locked()` as "the amount of BTC in the reserve output of the most recent fully-signed checkpoint."
- `src/bitcoin/checkpoint.rs` defines the reserve output as the output that "contains all funds held in reserve by the network."
- `src/bitcoin/mod.rs` also tracks `reward_pool` and `fee_pool`.
- `fee_pool` is especially important: `give_miner_fee()` burns nBTC accounting and moves the amount into `fee_pool`, but the BTC remains in reserve. That means a token-supply-shaped answer would undercount secured BTC.

Practical implication:

- If the explorer wants the honest custody number, expose checkpoint reserve BTC.
- Do not model this as `/cosmos/bank/v1beta1/supply/usat`.

## Recommended API Surface

Use the existing node-facing REST/LCD shim in `rest/src/main.rs`, backed by a new typed query in `src/bitcoin/mod.rs`.

This is the least-bad honest option because:

- The codebase already exposes public HTTP this way.
- No state migration is required.
- No new protobuf/gRPC query stack exists for the Bitcoin module today.
- It avoids pushing explorer consumers onto unsupported ABCI/hex-query internals.

Recommended route:

- `GET /nomic/bitcoin/v1/secured`

Do not overload existing Cosmos bank supply routes. This is bridge custody data, not fungible bank supply semantics.

## Recommended Response

All integer values should be serialized as strings, matching the existing REST style.

```json
{
  "network": "bitcoin",
  "units_per_sat": "1000000",
  "last_completed": {
    "sats": "123456789",
    "checkpoint_index": "4242",
    "signed_at_btc_height": "887654",
    "bitcoin_confirmed": false
  },
  "confirmed": {
    "sats": "123000000",
    "checkpoint_index": "4241"
  },
  "signing": {
    "sats": "123500000",
    "checkpoint_index": "4243"
  },
  "unconfirmed": {
    "count": "1",
    "first_checkpoint_index": "4242"
  },
  "accounting": {
    "reward_pool_usat": "4200000",
    "fee_pool_usat": "1337000"
  }
}
```

Semantics:

- `last_completed` is the primary explorer number.
  - It comes from the reserve output of `checkpoints.last_completed()`.
  - This matches the module's existing `value_locked()` definition.
- `confirmed` is the last Bitcoin-confirmed reserve amount.
  - This is useful when consumers want a stricter settled view.
- `signing` is optional.
  - When present, it shows the next frozen checkpoint reserve output while signatures are in progress.
  - This should be treated as informative, not canonical.
- `unconfirmed` explains whether `last_completed` is ahead of Bitcoin confirmation.
- `accounting.reward_pool_usat` and `accounting.fee_pool_usat` are supporting fields only.
  - They help explain why account-visible nBTC and secured BTC are not identical.

If there is no completed checkpoint yet, return HTTP `200` with `last_completed: null`, `confirmed: null`, and zeroed `unconfirmed` metadata instead of a hard error. That is easier for indexers to consume.

## Why This Is Better Than A `usat` Supply Endpoint

Even if a future `usat` supply route were added, it would answer a different question.

- `rest/src/main.rs` bank supply routes currently expose only NOM supply through `InnerApp::total_supply()`.
- `Bitcoin::accounts` only covers native account balances, not the full bridge accounting picture.
- `reward_pool` is not a user balance.
- `fee_pool` is explicitly non-account nBTC accounting that still corresponds to BTC held in reserve.

So:

- `supply/usat` would mean "token supply/accounting"
- the explorer needs "BTC secured by custody"

Those are not the same number.

## Implementation Plan

### 1. Add a typed query response in `src/bitcoin/mod.rs`

Add small query-only structs, for example:

- `SecuredBtcSummary`
- `CheckpointReserveSummary`
- `UnconfirmedCheckpointSummary`
- `AccountingSummary`

These should be query-friendly types similar in spirit to `ChangeRates`.

### 2. Add a Bitcoin query method

Add a new `#[query]` method on `Bitcoin`, for example:

```rust
pub fn secured_btc_summary(&self) -> Result<SecuredBtcSummary>
```

It should derive data from:

- `self.network()`
- `self.config.units_per_sat`
- `self.checkpoints.last_completed_index()`
- `self.checkpoints.last_completed()`
- `self.checkpoints.confirmed_index`
- `self.checkpoints.signing()`
- `self.checkpoints.num_unconfirmed()`
- `self.checkpoints.first_unconfirmed_index()`
- `self.reward_pool.amount`
- `self.fee_pool`

Implementation notes:

- Keep `last_completed.sats` sourced from `reserve_output().value`.
- Do not compute the primary number from `accounts`, IBC escrow, or relayer deposit state.
- Treat `signing` as optional.
- Return nullable fields instead of erroring when the chain has not produced a completed checkpoint yet.

### 3. Expose it from `rest/src/main.rs`

Add a dedicated route:

- `#[get("/nomic/bitcoin/v1/secured")]`

The route should:

- query `app.bitcoin.secured_btc_summary()`
- serialize all integers as strings
- preserve the explicit distinction between `last_completed`, `confirmed`, and optional `signing`

### 4. Mount the route

Register the new route in the Rocket `routes![...]` list.

### 5. Document the route

Keep this doc or convert it into API docs once the endpoint is implemented.

## Related Breakdowns: What Is Realistically Derivable

Good candidates to expose now:

- total secured BTC from `last_completed`
- last Bitcoin-confirmed secured BTC
- optional signing-checkpoint reserve BTC
- unconfirmed checkpoint metadata
- `reward_pool_usat`
- `fee_pool_usat`

Not honest to expose without more state:

- per-channel BTC secured totals
- per-destination BTC secured totals
- "wallet-held" vs "idle" vs "reward-held" BTC as separate Bitcoin buckets

Why not:

- the reserve is pooled at the checkpoint layer
- destination/channel details exist for pending transfers and relayer deposit indexing, not as durable aggregate reserve partitions
- `reward_pool` and `fee_pool` are accounting categories, not separate Bitcoin UTXO buckets

## Backward Compatibility

This should be an additive change only.

- No existing route behavior needs to change.
- No state migration is needed.
- No explorer should be asked to parse internal ABCI/Orga query bytes.

## Validation Plan

Minimum useful validation once implemented:

- unit test the new summary query against synthetic checkpoint states:
  - no completed checkpoint yet
  - completed but unconfirmed checkpoint
  - confirmed checkpoint
  - active signing checkpoint
- verify `last_completed.sats` matches the existing `Bitcoin::value_locked()` result
- verify `confirmed.sats` comes from `checkpoints.confirmed_index`
- verify `fee_pool_usat` changes when `give_miner_fee()` is used
- add a narrow REST test or route-level smoke test that the JSON contract is stable and string-serializes numeric fields

## Non-Goals

- implementing a full protobuf/gRPC/gateway stack for the Bitcoin module
- pretending `usat` bank supply is the same thing as secured BTC
- exposing relayer-local pending deposit state as if it were stable node API
