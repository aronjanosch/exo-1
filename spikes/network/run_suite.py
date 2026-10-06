#!/usr/bin/env python3
"""Run just this worktree, isolated user://, bounded child processes; no shell."""
import argparse
import json
import ipaddress
import os
from pathlib import Path
import subprocess
import time
from mcp_bridge import request

ROOT = Path(__file__).resolve().parents[2]
OUT = Path("/tmp/exo-spike4-results")
DATA = Path("/tmp/exo-spike4-data")
ENV = {**os.environ, "XDG_DATA_HOME": str(DATA)}
ENGINE = ["godot", "--headless", "--path", str(ROOT)]


def checked_output(process, log, timeout):
    code = process.wait(timeout=timeout)
    content = log.read_text()
    if code or "SCRIPT ERROR" in content or "ERROR:" in content:
        raise RuntimeError(f"{log}: exit {code}\n{content[-6000:]}")
    return content


def matrix():
    log = OUT / "matrix.log"
    with log.open("w") as stream:
        process = subprocess.Popen(ENGINE + ["--fixed-fps", "60", "--script",
            "res://spikes/network/test.gd"], env=ENV, stdout=stream, stderr=subprocess.STDOUT)
        try:
            checked_output(process, log, 90)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
    source = next(DATA.rglob("spike4-matrix.json"))
    result = json.loads(source.read_text())
    (OUT / "matrix.json").write_text(json.dumps(result, indent=2) + "\n")
    assert result["failures"] == 0 and len(result["cases"]) == 96
    print("PASS matrix: 96 configurations, real ship, cabin and contacts", flush=True)


def live(name, count, port, seconds, extra=(), planets=False):
    processes = []
    handles = []
    try:
        for slot in range(1, count + 1):
            log = OUT / f"{name}-{slot}.log"
            handle = log.open("w")
            handles.append(handle)
            args = ENGINE + ["res://spikes/network/main.tscn", "--", "--bot",
                f"--slot={slot}", f"--port={port}",
                f"--seconds={seconds + 2 if slot == 1 else seconds}",
                "--shift=10", *extra]
            if name in ["client-2", "two-planets"]:
                args.append(f"--agent-port={18440 + slot}")
            if slot == 1:
                args.extend(["--host", "--bind=0.0.0.0" if name == "lan-bind" else "--bind=127.0.0.1"])
            if planets and slot == 2:
                args.append("--planet=1")
            processes.append((subprocess.Popen(args, env=ENV, stdout=handle,
                stderr=subprocess.STDOUT), log))
            if slot == 1:
                time.sleep(0.35)
        if name == "client-2":
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                try:
                    state = request(18442, {"op": "get_state"})["state"]
                    if state["remotes"]:
                        break
                except (OSError, KeyError):
                    pass
                time.sleep(0.05)
            assert state["remotes"] and state["ready"]
            assert not request(18442, {"op": "do_action", "action": "pilot", "args": {"movement": ["bad", 0, 0]}})["ok"]
            messages = [
                {"jsonrpc": "2.0", "id": 1, "method": "initialize"},
                {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
                    "name": "do_action", "arguments": {"action": "board_remote", "args": {"owner": 1}}}},
                {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "get_state"}},
            ]
            response = subprocess.run(["python3", str(ROOT / "spikes/network/mcp_bridge.py"), "--port", "18442"],
                input="".join(json.dumps(m) + "\n" for m in messages), text=True, capture_output=True, timeout=5, check=True)
            replies = [json.loads(line) for line in response.stdout.splitlines()]
            assert len(replies) == 4 and all("result" in reply for reply in replies)
            assert json.loads(replies[2]["result"]["content"][0]["text"])["ok"]
            assert json.loads(replies[3]["result"]["content"][0]["text"])["state"]["frame_id"] == 1
            request(18442, {"op": "do_action", "action": "shift", "args": {}})
            time.sleep(0.4)
            state = request(18442, {"op": "get_state"})["state"]
            assert state["walking"] and state["frame_id"] == 1 and state["walker_on_floor"]
            (OUT / "mcp-cabin.json").write_text(json.dumps({"responses": replies, "after_shift": state}, indent=2) + "\n")
            print("PASS MCP: initialize/list/read/action; live foreign cabin after local shift", flush=True)
        if name == "two-planets":
            deadline = time.monotonic() + 3
            state = {}
            while time.monotonic() < deadline:
                try:
                    state = request(18442, {"op": "get_state"})["state"]
                    if state["remotes"]:
                        break
                except (OSError, KeyError):
                    pass
                time.sleep(0.05)
            assert state["planet"] == 1 and state["remotes"][0]["planet"] == 0
            shifted = request(18442, {"op": "do_action", "action": "shift", "args": {}})["state"]
            for remote in shifted["remotes"]:
                expected = [remote["position"][i] - shifted["origin"][i] for i in range(3)]
                assert max(abs(remote["world_position"][i] - expected[i]) for i in range(3)) < .02
            assert shifted["origin"][0] >= 200000 and shifted["shifts"] > state["shifts"]
            (OUT / "two-planets-frame.json").write_text(json.dumps(shifted, indent=2) + "\n")
            print("PASS two planets: live shared frame survives independent 10 km shift", flush=True)
        results = []
        # Clients finish first so the host does not disconnect a running client.
        for process, log in processes[1:] + processes[:1]:
            content = checked_output(process, log, seconds + 12)
            line = next(line for line in content.splitlines() if line.startswith("SPIKE4 RESULT "))
            result = json.loads(line.removeprefix("SPIKE4 RESULT "))
            assert result["invalid"] == 0 and not result["mouse_captured"]
            # By the time the host ends, peers have left; check observed count below.
            if not result["host"]:
                assert result["maximum_remotes"] >= count - 1
                assert result["receive_payload_bytes"] > 144 * 20
            results.append(result)
        (OUT / f"{name}.json").write_text(json.dumps(results, indent=2) + "\n")
        print(f"PASS {name}: {count} real ENet instances", flush=True)
        return results
    finally:
        for process, _ in processes:
            if process.poll() is None:
                process.terminate()
        for process, _ in processes:
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        for handle in handles:
            handle.close()


def reconnect():
    processes = []
    handles = []
    try:
        def start(label, options):
            log = OUT / f"reconnect-{label}.log"
            handle = log.open("w")
            handles.append(handle)
            process = subprocess.Popen(ENGINE + ["res://spikes/network/main.tscn", "--",
                "--bot", "--port=17448", *options], env=ENV, stdout=handle, stderr=subprocess.STDOUT)
            processes.append(process)
            return process, log
        host, host_log = start("host", ["--host", "--bind=127.0.0.1", "--seconds=8", "--agent-port=18441"])
        time.sleep(0.35)
        first, first_log = start("first", ["--slot=2", "--seconds=2"])
        first_text = checked_output(first, first_log, 10)
        second, second_log = start("second", ["--slot=2", "--seconds=3"])
        time.sleep(0.8)
        state = request(18441, {"op": "get_state"})["state"]
        remote = next(item for item in state["remotes"] if item["owner"] == 2)
        assert remote["sequence"] < 30 and remote["position"][1] - 5000 < 15
        second_text = checked_output(second, second_log, 8)
        host_text = checked_output(host, host_log, 10)
        (OUT / "reconnect.json").write_text(json.dumps({"after_rejoin": state,
            "first": json.loads(next(line.removeprefix("SPIKE4 RESULT ") for line in first_text.splitlines() if line.startswith("SPIKE4 RESULT "))),
            "second": json.loads(next(line.removeprefix("SPIKE4 RESULT ") for line in second_text.splitlines() if line.startswith("SPIKE4 RESULT "))),
            "host": json.loads(next(line.removeprefix("SPIKE4 RESULT ") for line in host_text.splitlines() if line.startswith("SPIKE4 RESULT ")))}, indent=2) + "\n")
        print("PASS reconnect: same owner rejoins before expiry, fresh sequence/pose", flush=True)
    finally:
        for process in processes:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
        for handle in handles:
            handle.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--matrix-only", action="store_true")
    parser.add_argument("--live-only", action="store_true")
    parser.add_argument("--seconds", type=float, default=8)
    parser.add_argument("--case", choices=["client-2", "client-8", "client-2-impaired", "host-reference", "two-planets", "lan-bind", "reconnect"])
    args = parser.parse_args()
    OUT.mkdir(exist_ok=True)
    DATA.mkdir(exist_ok=True)
    if not args.live_only and not args.case:
        matrix()
    if not args.matrix_only:
        cases = [
            ("client-2", 2, 17441, (), False),
            ("client-8", 8, 17442, ("--delay=150", "--jitter=20", "--loss=10"), False),
            ("client-2-impaired", 2, 17445, ("--delay=150", "--jitter=20", "--loss=5"), False),
            ("host-reference", 2, 17443, ("--reference", "--delay=150", "--jitter=20", "--loss=5"), False),
            ("two-planets", 2, 17444, ("--delay=50", "--jitter=20", "--loss=5"), True),
        ]
        # Exercise configurable IP and all-interface bind on this machine's
        # private LAN address, without copying real addresses into test logs.
        lan_address = None
        try:
            interfaces = json.loads(subprocess.check_output(["ip", "-j", "-4", "address", "show", "scope", "global"], text=True))
            for interface in interfaces:
                if interface["ifname"].startswith(("docker", "veth", "br-", "tailscale")):
                    continue
                for address in interface.get("addr_info", []):
                    candidate = ipaddress.ip_address(address["local"])
                    if candidate.is_private and not candidate.is_loopback:
                        lan_address = str(candidate)
                        break
                if lan_address:
                    break
        except (OSError, ValueError, subprocess.SubprocessError):
            pass
        cases.append(("lan-bind", 2, 17447, (f"--connect={lan_address or '127.0.0.1'}",), False))
        for name, count, port, extra, planets in cases:
            if not args.case or name == args.case:
                live(name, count, port, args.seconds, extra, planets)
                if name == "lan-bind":
                    (OUT / "lan-bind-target.json").write_text(json.dumps({
                        "all_interfaces": True, "non_loopback_private_address": bool(lan_address),
                        "physical_computers": 1, "udp_port": 17447}, indent=2) + "\n")
        if not args.case or args.case == "reconnect":
            reconnect()
    print(f"Results: {OUT}", flush=True)


if __name__ == "__main__":
    main()
