#!/usr/bin/env python3
"""Small portable Godot source project; never edit the worktree's root config."""
from pathlib import Path
import shutil
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
archive = HERE / "dist/exo-spike4-source.zip"
archive.parent.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix="exo-spike4-package-") as folder:
    project = Path(folder) / "exo-spike4-source"
    (project / "spikes/planet").mkdir(parents=True)
    (project / "spikes/network").mkdir()
    config = (ROOT / "project.godot").read_text().replace(
        'run/main_scene="res://spikes/planet/main.tscn"',
        'run/main_scene="res://spikes/network/main.tscn"').replace(
        'config/name="Planet Spike"', 'config/name="Network Spike 4"')
    (project / "project.godot").write_text(config)
    shutil.copyfile(ROOT / "LICENSE", project / "LICENSE")
    shutil.copyfile(HERE / "README.md", project / "README.md")
    for name in ["ship.gd", "player.gd", "spike_input.gd", "flight_hud.gd"]:
        shutil.copyfile(ROOT / "spikes/planet" / name, project / "spikes/planet" / name)
    for path in HERE.rglob("*"):
        if not path.is_file() or any(part in {"dist", "__pycache__"} for part in path.relative_to(HERE).parts):
            continue
        target = project / "spikes/network" / path.relative_to(HERE)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, target)
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as output:
        for path in sorted(project.rglob("*")):
            if path.is_file():
                output.write(path, path.relative_to(project.parent))
print(f"Source package: {archive} ({archive.stat().st_size} bytes)")
