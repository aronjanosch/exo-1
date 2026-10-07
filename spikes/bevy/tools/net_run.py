#!/usr/bin/env python3
"""Spike 10 live runs: one host and N-1 clients as headless processes on localhost.

  python3 tools/net_run.py --players 8 --seconds 40 --tag p8 [--delay 150 --jitter 20 --loss 10]
          [--planets 0,1,0,...] [--force-shift 20,10000,10000,-10000] [--rate 30] [--buffer 150]

Needs a built exo_app (release). Results: results/net/<tag>-slot<N>.json and summary-<tag>.json.
Standard library only.
"""
import argparse, json, os, subprocess, sys, time, statistics

here = os.path.dirname(os.path.abspath(__file__))
root = os.path.dirname(here)
ap = argparse.ArgumentParser()
ap.add_argument("--bin", default=os.environ.get("EXO_BIN") or os.path.join(os.environ.get("CARGO_TARGET_DIR", os.path.join(root, "target")), "release", "exo_app"))
ap.add_argument("--players", type=int, default=2)
ap.add_argument("--seconds", type=float, default=30)
ap.add_argument("--tag", default="run")
ap.add_argument("--port", type=int, default=17441)
ap.add_argument("--rate", type=float, default=30)
ap.add_argument("--buffer", type=float, default=150)
ap.add_argument("--delay", type=float, default=0)
ap.add_argument("--jitter", type=float, default=0)
ap.add_argument("--loss", type=float, default=0)
ap.add_argument("--planets", default="")
ap.add_argument("--force-shift", default="")
ap.add_argument("--out", default=os.path.join(root, "results", "net"))
ap.add_argument("--extra", default="", help="extra arguments for every process")
a = ap.parse_args()
os.makedirs(a.out, exist_ok=True)
planets = [int(x) for x in a.planets.split(",")] if a.planets else [0] * a.players
common = [f"--rate={a.rate}", f"--buffer={a.buffer}", f"--delay={a.delay}", f"--jitter={a.jitter}", f"--loss={a.loss}",
          f"--tag={a.tag}", f"--net-out={a.out}", f"--port={a.port}", f"--out={a.out}", "--headless", "--bot"] + a.extra.split()
if a.force_shift:
    common.append(f"--force-shift={a.force_shift}")
procs = []
logs = []
def launch(slot, extra, secs):
    log = open(os.path.join(a.out, f"{a.tag}-slot{slot}.log"), "w")
    logs.append(log)
    cmd = [a.bin] + common + [f"--slot={slot}", f"--planet={planets[slot - 1]}", f"--seconds={secs}"] + extra
    procs.append((slot, subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT)))
# The host outlives the clients by a few seconds so nobody talks to a closed port.
launch(1, ["--net-host"], a.seconds + 6)
time.sleep(2.0)
for slot in range(2, a.players + 1):
    launch(slot, [f"--net-connect=127.0.0.1:{a.port}"], a.seconds)
    time.sleep(0.25)
codes = {}
for slot, p in procs:
    codes[slot] = p.wait(timeout=a.seconds + 120)
for l in logs:
    l.close()
res = {}
for slot in range(1, a.players + 1):
    path = os.path.join(a.out, f"{a.tag}-slot{slot}.json")
    if os.path.exists(path):
        res[slot] = json.load(open(path))
def agg(key, slots):
    v = [res[s][key] for s in slots if s in res]
    return (round(statistics.mean(v), 4), round(min(v), 4), round(max(v), 4)) if v else None
clients = [s for s in res if s != 1]
summary = {"players": a.players, "seconds": a.seconds, "exit_codes": codes, "args": vars(a)}
# True clock error of each client: wall-clock start difference against the estimated offset.
errs = []
for s in clients:
    true_offset = res[s]["start_epoch_s"] - res[1]["start_epoch_s"]  # server_now = local + offset
    errs.append(abs(res[s]["clock_offset_s"] - true_offset) * 1000)
summary["clock_error_ms_max"] = round(max(errs), 3) if errs else None
for key in ["hold_percent", "net_pre_ms_mean", "net_pre_ms_p95", "net_post_ms_mean", "physics_step_ms_mean", "process_cpu_ms_per_tick",
            "payload_tx_kB_s", "payload_rx_kB_s", "wire_tx_kB_s", "wire_rx_kB_s", "invalid", "injected_dropped", "input_response_ticks",
            "render_error_near_max_mm", "render_error_far_max_mm", "render_shift_jump_max_mm", "shift_frames_seen", "max_remotes"]:
    summary["clients_" + key] = agg(key, clients)
    summary["host_" + key] = res[1][key] if 1 in res else None
json.dump(summary, open(os.path.join(a.out, f"summary-{a.tag}.json"), "w"), indent=1)
print(json.dumps(summary, indent=1))
sys.exit(0 if all(c == 0 for c in codes.values()) else 1)
