#!/usr/bin/env python3
from pathlib import Path
import subprocess
import sys


def main():
    root = Path(__file__).resolve().parent
    scripts = [
        "header_queue_reorg_model.py",
        "deposit_outpoint_order_model.py",
        "ibc_transfer_failure_model.py",
        "threshold_effective_power_model.py",
    ]
    for script in scripts:
        print(f"== {script} ==")
        subprocess.run([sys.executable, str(root / script)], check=True)


if __name__ == "__main__":
    main()
