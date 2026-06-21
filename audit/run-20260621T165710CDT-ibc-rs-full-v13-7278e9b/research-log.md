# Research Log - ibc-rs Full v13.0.0 / 7278e9b

Target: `7278e9b8d8a5550306cdb11980cb0d0c51fcfe4e` / `v13.0.0`

Source checkout: `/home/nitro/repos/nomic-v13-audit-7278e9b`

## Steps

- Captured IBC dependency tree and reverse dependency paths from the v13 lockfile and metadata.
- Followed Nomic IBC entrypoints in `src/app.rs`.
- Followed the pinned Orga IBC wrapper and transfer execution context at rev `e539a53e1eed9c3363062d419ea84d8d0d56e1c7`.
- Followed ibc-rs 0.54.0 send, acknowledgement, and timeout source paths under the local Cargo registry.
- Reviewed host consensus state lookup behavior in pinned Orga.

## Non-Actions

- Did not use public Nomic RPC, public testnets, relayers, validators, or real assets.
- Did not alter target source.
