#!/usr/bin/env python3
"""Run all Phase 2 audit models."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


MODEL_DIR = Path(__file__).resolve().parent

MODELS = [
    "mutation_persistence_model.py",
    "header_reorg_mutation_model.py",
    "deposit_outpoint_poison_model.py",
    "withdrawal_debit_before_validation_model.py",
    "frost_stale_iteration_model.py",
    "cosmos_relay_op_key_temporal_model.py",
]


def main() -> int:
    results = []
    for model in MODELS:
        path = MODEL_DIR / model
        proc = subprocess.run(
            [sys.executable, str(path)],
            cwd=str(MODEL_DIR),
            text=True,
            capture_output=True,
            check=False,
        )
        results.append(
            {
                "model": model,
                "returncode": proc.returncode,
                "stdout": proc.stdout,
                "stderr": proc.stderr,
            }
        )

    failed = [item for item in results if item["returncode"] != 0]
    print(json.dumps({"models": results, "failed": [f["model"] for f in failed]}, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
