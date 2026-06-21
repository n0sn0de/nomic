# Phase 3 Audit Run

Run: `run-20260621T1626CDT-phase3-c5486a4`

Target source commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Purpose: move the strongest Phase 2 source-order models toward executable Rust harnesses while keeping all generated code under this audit run directory.

Primary outcome: Phase 3 runtime-confirmed failed `DeliverTx` mutation persistence in the pinned Orga source order, rejected `HeaderQueue` replacement mutation, and stale `current_work` inflation after the rejected replacement.

Layout:

- `harnesses/orga_persistence/`: audit-local Rust harness for the pinned Orga failed-call persistence source order.
- `harnesses/header_queue_probe/`: audit-local Rust harness using real `nomic::bitcoin::header_queue` types and a no-network header fixture.
- `results/`: captured command outputs.
- `notes/`: source anchors and follow-up notes.
- `phase3-runtime-harnesses.md`: primary report.
- `research-log.md`: chronology and limitations.
