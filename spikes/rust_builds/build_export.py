#!/usr/bin/env python3
"""Spike 7: build the Rust extension for Linux and Windows and export a small check project.

Usage: build_export.py [linux] [windows] [--no-cargo]
Linux: cargo release build. Windows: cross build from Linux with llvm-mingw
(target x86_64-pc-windows-gnullvm, linker from mise: github:mstorsjo/llvm-mingw),
or on a Windows runner set LLVM_MINGW="" and CARGO_TARGET to build natively.
Output: build/spike7/<platform>/ (repo root, gitignored). Needs Godot 4.7.2 export templates.
"""
from pathlib import Path
import os
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
RUST = ROOT / "spikes/gen_bench/rust"
OUT = ROOT / "build/spike7"
GODOT = os.environ.get("GODOT", "godot")
ARGS = sys.argv[1:]
PLATFORMS = [p for p in ARGS if p in ("linux", "windows")] or ["linux", "windows"]
WIN_TARGET = os.environ.get("WIN_TARGET", "x86_64-pc-windows-gnullvm")

PROJECT = """config_version=5

[application]
config/name="EXO Spike 7"
run/main_scene="res://check.tscn"

[rendering]
renderer/rendering_method="forward_plus"
"""

SCENE = """[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://check.gd" id="1"]

[node name="Check" type="Node"]
script = ExtResource("1")
"""

GDEXT = """[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = 4.7
reloadable = false

[libraries]
linux.debug.x86_64 = "res://bin/libgen_godot.so"
linux.release.x86_64 = "res://bin/libgen_godot.so"
windows.debug.x86_64 = "res://bin/gen_godot.dll"
windows.release.x86_64 = "res://bin/gen_godot.dll"

[dependencies]
windows.x86_64 = { "res://bin/libunwind.dll": "" }
"""

PRESETS = """[preset.0]
name="linux"
platform="Linux"
runnable=true
export_filter="all_resources"
include_filter=""
exclude_filter=""
export_path=""

[preset.0.options]
binary_format/embed_pck=true
binary_format/architecture="x86_64"

[preset.1]
name="windows"
platform="Windows Desktop"
runnable=true
export_filter="all_resources"
include_filter=""
exclude_filter=""
export_path=""

[preset.1.options]
binary_format/embed_pck=true
binary_format/architecture="x86_64"
codesign/enable=false
application/modify_resources=false
"""


def run(args, cwd=None, env=None):
    print("+", " ".join(str(a) for a in args), flush=True)
    t0 = time.time()
    subprocess.run([str(a) for a in args], cwd=cwd, env=env, check=True)
    return time.time() - t0


def llvm_mingw_bin():
    if os.environ.get("LLVM_MINGW") is not None:
        return os.environ["LLVM_MINGW"]
    out = subprocess.run(["mise", "where", "github:mstorsjo/llvm-mingw@20260922"],
                         capture_output=True, text=True, check=True).stdout.strip()
    return str(Path(out) / "bin")


times = {}
proj = OUT / "project"
shutil.rmtree(proj, ignore_errors=True)
(proj / "bin").mkdir(parents=True)

if "linux" in PLATFORMS:
    if "--no-cargo" not in ARGS:
        times["cargo_linux_s"] = run(["cargo", "build", "--release", "-p", "gen_godot"], cwd=RUST)
    shutil.copy(RUST / "target/release/libgen_godot.so", proj / "bin")
if "windows" in PLATFORMS:
    env = dict(os.environ)
    mingw = llvm_mingw_bin()
    if mingw:
        env["PATH"] = mingw + os.pathsep + env["PATH"]
    if "--no-cargo" not in ARGS:
        times["cargo_windows_s"] = run(["cargo", "build", "--release", "-p", "gen_godot", "--target", WIN_TARGET],
                                       cwd=RUST, env=env)
    shutil.copy(RUST / f"target/{WIN_TARGET}/release/gen_godot.dll", proj / "bin")
    if WIN_TARGET.endswith("gnullvm"):
        shutil.copy(Path(mingw).parent / "x86_64-w64-mingw32/bin/libunwind.dll", proj / "bin")
    else:
        (proj / "bin/libunwind.dll").write_bytes(b"")  # keeps the dependency entry valid

(proj / "project.godot").write_text(PROJECT)
(proj / "check.tscn").write_text(SCENE)
shutil.copy(HERE / "check.gd", proj)
(proj / "exo_gen.gdextension").write_text(GDEXT)
(proj / "export_presets.cfg").write_text(PRESETS)
# The headless import can abort on exit (SIGABRT seen once); what counts is the registered extension.
rc = subprocess.run([GODOT, "--headless", "--path", str(proj), "--import"], capture_output=True).returncode
if "exo_gen.gdextension" not in (proj / ".godot/extension_list.cfg").read_text():
    sys.exit(f"import failed (exit {rc}), extension not registered")
print("import exit code", rc, flush=True)

names = {"linux": "exo-spike7.x86_64", "windows": "exo-spike7.exe"}
for p in PLATFORMS:
    target = OUT / p
    shutil.rmtree(target, ignore_errors=True)
    target.mkdir(parents=True)
    times[f"export_{p}_s"] = run([GODOT, "--headless", "--path", proj, "--export-release", p, target / names[p]])
    sizes = {f.name: f.stat().st_size for f in target.iterdir()}
    print(p, "files:", sizes, flush=True)

print("times:", {k: round(v, 1) for k, v in times.items()})
