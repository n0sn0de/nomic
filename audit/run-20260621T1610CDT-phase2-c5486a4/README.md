# Phase 2 Audit Run

Run: `run-20260621T1610CDT-phase2-c5486a4`

Target source commit: `c5486a4a9b41de5474f991891ae22f734b1d5aec`

Purpose: executable invariant models for the highest-value Phase 1 leads. The models are local-only, deterministic, and intentionally small. They are not production tests and do not connect to any network.

Layout for this and future phases:

- `models/`: generated local model and harness code.
- `results/`: captured stdout/stderr from model runs and future tests.
- `notes/`: command notes, source anchors, and scratch observations.
- `phase2-executable-invariants.md`: primary report for this run.
- `research-log.md`: concise activity log.

Run all models:

```bash
python3 audit/run-20260621T1610CDT-phase2-c5486a4/models/run_all.py
```
