#!/usr/bin/env bash
# Spike 10 live suite (steps 3 and 4): sequential runs, nothing else on the machine.
set -u
cd "$(dirname "$0")/.."
run() { python3 tools/net_run.py "$@" > /dev/null || echo "FAILED: $*"; }
run --players 2 --seconds 40 --tag live-2 --extra="--origin-shift=200"
run --players 2 --seconds 40 --tag live-2-impaired --delay 150 --jitter 20 --loss 5 --extra="--origin-shift=200"
run --players 8 --seconds 40 --tag live-8 --extra="--origin-shift=200"
run --players 8 --seconds 40 --tag live-8-impaired --delay 150 --jitter 20 --loss 10 --extra="--origin-shift=200"
run --players 2 --seconds 40 --tag two-planets --planets 0,1 --force-shift 15,10000,10000,-10000 --extra="--origin-shift=200"
run --players 8 --seconds 40 --tag eight-two-planets --planets 0,1,0,1,0,1,0,1 --force-shift 15,10000,10000,-10000 --extra="--origin-shift=200"
echo SUITE DONE
