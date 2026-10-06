#!/usr/bin/env python3
"""Two unfocused windows on the configured agent workspace; capture then exit."""
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
ENV = {**os.environ, "XDG_DATA_HOME": "/tmp/exo-spike4-data", "GODOT_AGENT_WORKSPACE": "7"}
processes = []
try:
    for slot in [1, 2]:
        args = ["godot-agent", "--path", str(ROOT), "--log-file", f"/tmp/exo-spike4-visual-{slot}.log", "res://spikes/network/main.tscn", "--",
                "--bot", "--shot", "--observe", f"--slot={slot}", "--port=17446",
                f"--seconds={8 if slot == 1 else 6}", "--delay=150", "--jitter=20", "--loss=10"]
        if slot == 1:
            args.extend(["--host", "--bind=127.0.0.1"])
        processes.append(subprocess.Popen(args, env=ENV))
        if slot == 1:
            time.sleep(1)
    for process in reversed(processes):
        if process.wait(timeout=20):
            raise RuntimeError("visual instance failed")
finally:
    for process in processes:
        if process.poll() is None:
            process.terminate()
