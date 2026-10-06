#!/usr/bin/env python3
"""Export tester builds (Linux, Windows, macOS) from the portable source package.

Needs the official Godot 4.7.2 export templates in the user's template folder.
Output: build/spike4/exo-spike4-<platform>.zip, each with host/client launchers
and a tester guide. Nothing here touches the worktree's project.godot.
"""
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
OUT = ROOT / "build/spike4"
GODOT = os.environ.get("GODOT", "godot")
PLATFORMS = [p for p in sys.argv[1:] if p in ("linux", "windows", "macos")] or ["linux", "windows", "macos"]

PRESETS = """[preset.0]
name="linux"
platform="Linux"
runnable=true
export_filter="all_resources"
include_filter=""
exclude_filter=""
export_path="exo-spike4.x86_64"

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
export_path="exo-spike4.exe"

[preset.1.options]
binary_format/embed_pck=true
binary_format/architecture="x86_64"
codesign/enable=false
application/modify_resources=false

[preset.2]
name="macos"
platform="macOS"
runnable=true
export_filter="all_resources"
include_filter=""
exclude_filter=""
export_path="exo-spike4.zip"

[preset.2.options]
binary_format/architecture="universal"
export/distribution_type=0
application/bundle_identifier="org.exo1.spike4"
application/short_version="0.4.0"
application/version="0.4.0"
codesign/codesign=1
notarization/notarization=0
"""

GUIDE = """EXO-1 Netzwerk-Spike 4, Testbuild
==================================

Ein Rechner ist der Host, die anderen verbinden sich dorthin. Alle müssen im
selben LAN sein. Der Host braucht offenen UDP-Port 17440 (Windows fragt beim
ersten Start nach Firewall-Freigabe: zulassen, auch für private Netze).

Starten
-------
Host:    host-Starter doppelklicken (host.bat, host.command oder host.sh).
Client:  client-Starter doppelklicken, Host-IP und Spieler-Nummer eingeben
         (jeder Client eine eigene Nummer: 2, 3, ...).
Die Host-IP ist die LAN-IPv4-Adresse des Host-Rechners (z. B. 192.168.x.y).

macOS: Beim ersten Start evtl. Rechtsklick auf "Network Spike 4.app", "Öffnen",
dann bestätigen. Der .command-Starter entfernt die Quarantäne selbst.
Windows: SmartScreen-Warnung mit "Weitere Informationen", "Trotzdem ausführen".

Erwartet: Auf allen Seiten steht "remotes" mit der Zahl der anderen Spieler.
Das eigene Schiff reagiert sofort; fremde Schiffe werden absichtlich etwa
150 ms verzögert gezeigt.

Steuerung
---------
WASD: Schiff vor/zurück/seitwärts; Space/Ctrl: hoch/runter.
Klick fängt die Maus, Maus dreht; Esc gibt sie frei. Q/E: rollen; H: Assist.
Tab: fremdes Schiff verfolgen / eigene Kamera zurück.
F: in der eigenen Kabine laufen / zurück auf den Sitz. B: fremde Kabine.
O: Test-Sprung der Welt um 10 km (beim anderen darf nichts springen).

Bitte testen und melden
-----------------------
1. Abwechselnd fliegen, drehen, starten, landen. Ruckelt das fremde Schiff?
2. Eine Seite drückt O. Springt beim anderen etwas?
3. Host fliegt; ein Client drückt B und läuft mit WASD im Host-Schiff.
   Steht die grüne Figur beim Host richtig in der Kabine? Dann Client O.
4. Client beenden (Fenster schließen) und mit derselben Nummer neu verbinden.
   Bleibt ein Geisterschiff länger als zwei Sekunden?
Bei Problemen: Betriebssystem, was passiert ist, und ob WLAN oder Kabel.
Ein Testprototyp: Zusammenstöße zweier Schiffe sind bekannt unfertig.
"""

LAUNCHERS = {
    "linux": {
        "host.sh": '#!/usr/bin/env bash\ncd "$(dirname "$0")"\n'
                   './exo-spike4.x86_64 -- --host --slot=1 --port=17440 "$@"\n',
        "client.sh": '#!/usr/bin/env bash\ncd "$(dirname "$0")"\n'
                     'ip="${1:-}"; [ -z "$ip" ] && read -rp "Host-IP (LAN-IPv4): " ip\n'
                     'slot="${2:-}"; [ -z "$slot" ] && read -rp "Spieler-Nummer (2, 3, ...) [2]: " slot\n'
                     './exo-spike4.x86_64 -- --connect="$ip" --slot="${slot:-2}" --port=17440\n',
    },
    "macos": {
        "host.command": '#!/usr/bin/env bash\ncd "$(dirname "$0")"\n'
                        'xattr -cr "Network Spike 4.app" 2>/dev/null\n'
                        '"./Network Spike 4.app/Contents/MacOS/Network Spike 4" -- --host --slot=1 --port=17440 "$@"\n',
        "client.command": '#!/usr/bin/env bash\ncd "$(dirname "$0")"\n'
                          'xattr -cr "Network Spike 4.app" 2>/dev/null\n'
                          'read -rp "Host-IP (LAN-IPv4): " ip\n'
                          'read -rp "Spieler-Nummer (2, 3, ...) [2]: " slot\n'
                          '"./Network Spike 4.app/Contents/MacOS/Network Spike 4" -- --connect="$ip" --slot="${slot:-2}" --port=17440\n',
    },
    "windows": {
        "host.bat": '@echo off\r\ncd /d "%~dp0"\r\n'
                    'exo-spike4.exe -- --host --slot=1 --port=17440\r\n',
        "client.bat": '@echo off\r\ncd /d "%~dp0"\r\n'
                      'set /p ip=Host-IP (LAN-IPv4): \r\n'
                      'set slot=2\r\nset /p slot=Spieler-Nummer (2, 3, ...) [2]: \r\n'
                      'exo-spike4.exe -- --connect=%ip% --slot=%slot% --port=17440\r\n',
    },
}


def run(*args, cwd):
    print("+", " ".join(str(a) for a in args), flush=True)
    subprocess.run([str(a) for a in args], cwd=cwd, check=True)


subprocess.run([sys.executable, HERE / "build_source.py"], check=True)
OUT.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="exo-spike4-export-") as folder:
    with zipfile.ZipFile(HERE / "dist/exo-spike4-source.zip") as source:
        source.extractall(folder)
    project = Path(folder) / "exo-spike4-source"
    (project / "export_presets.cfg").write_text(PRESETS)
    # Universal macOS binaries refuse to export without ETC2/ASTC textures.
    with (project / "project.godot").open("a") as config:
        config.write("\n[rendering]\n\ntextures/vram_compression/import_etc2_astc=true\n")
    run(GODOT, "--headless", "--path", project, "--import", cwd=project)
    for platform in PLATFORMS:
        stage = Path(folder) / f"stage-{platform}"
        stage.mkdir()
        target = {"linux": "exo-spike4.x86_64", "windows": "exo-spike4.exe", "macos": "exo-spike4.zip"}[platform]
        run(GODOT, "--headless", "--path", project, "--export-release", platform, stage / target, cwd=project)
        if platform == "macos":
            run("unzip", "-q", stage / target, "-d", stage, cwd=stage)
            (stage / target).unlink()
        for name, body in LAUNCHERS[platform].items():
            path = stage / name
            path.write_text(body)
            path.chmod(0o755)
        (stage / "ANLEITUNG.txt").write_text(GUIDE)
        shutil.copyfile(ROOT / "LICENSE", stage / "LICENSE")
        archive = OUT / f"exo-spike4-{platform}.zip"
        archive.unlink(missing_ok=True)
        run("zip", "-q", "-r", "-y", archive, ".", cwd=stage)
        print(f"Build: {archive} ({archive.stat().st_size // 1_000_000} MB)", flush=True)
