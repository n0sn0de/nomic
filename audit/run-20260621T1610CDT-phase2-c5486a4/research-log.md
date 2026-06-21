# Research Log

Run: `run-20260621T1610CDT-phase2-c5486a4`

Target source commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Current branch during run: `security-audit`

Current HEAD during run: `5fd66a8fb858a73eb2a22e997ac3dcb868470b04`

## Starting Context

- Continued from Phase 1 artifact directory `audit/run-20260621T1244CDT-phase1-c5486a4/`.
- Read Phase 1 new candidate leads and suggested Phase 2 section.
- Confirmed current working tree initially had only unrelated untracked root workspace files before this run's audit directory was added.
- Confirmed `git diff --name-only c5486a4a9b41de5474f991891ae22f734b1d5aec..HEAD` listed audit artifacts, not source edits.

## Artifacts Created

- `README.md`
- `phase2-executable-invariants.md`
- `research-log.md`
- `models/common.py`
- `models/run_all.py`
- `models/mutation_persistence_model.py`
- `models/header_reorg_mutation_model.py`
- `models/deposit_outpoint_poison_model.py`
- `models/withdrawal_debit_before_validation_model.py`
- `models/frost_stale_iteration_model.py`
- `models/cosmos_relay_op_key_temporal_model.py`
- Captured outputs under `results/`

## Commands Captured

- `python3 audit/run-20260621T1610CDT-phase2-c5486a4/models/run_all.py`
- `python3 -m py_compile audit/run-20260621T1610CDT-phase2-c5486a4/models/*.py`
- `git status --short`
- `git rev-parse HEAD`
- `git diff --name-only c5486a4a9b41de5474f991891ae22f734b1d5aec..HEAD`
- Artifact file listing

## Results

- Model suite passed with exit code 0.
- Python syntax compilation passed with exit code 0.
- `results/run-all-models.err` and `results/py-compile.err` are empty.

## Leads Updated

- P1-001: source-order failed-call persistence modeled and control-compared.
- P1-002: header reorg source-order hazard modeled; Phase 2 added the `current_work` inflation refinement because `pop_back_to` does not decrement `current_work`.
- P1-003: disabled-deposit processed-outpoint poisoning modeled.
- P1-004: invalid withdrawal debit-before-validation modeled.
- P1-005: stale FROST share poisoning modeled.
- P1-006: temporal/type-url validation gap modeled as hardening/correctness issue, not exploit proof.

## Tooling Note

An initial `apply_patch` used relative paths while the session cwd was `/home/nitro/.openclaw/workspace`, so the model files were first created under the workspace instead of `/home/nitro/repos/nomic`. The files were mechanically copied into the intended repo audit directory. The accidental workspace copy was moved to `/home/nitro/.openclaw/workspace/.trash/run-20260621T1610CDT-phase2-c5486a4-accidental-copy`.

The error was logged through the self-improvement skill in `/home/nitro/.openclaw/workspace/.learnings/ERRORS.md`.

## Safety Notes

No public networks, public RPCs, public Nomic infrastructure, Bitcoin nodes, IBC relayers, signers, validators, frontends, explorers, or third-party services were contacted. No real keys, BTC, or user data were used.
