"""Review scene: imports the exported city .glb files and lines them up along a hover lane.

Writes nothing into the repo; it renders only. Also proves the .glb files import cleanly.

Run headless after the model scripts:
    blender -b -P art/city/street_preview.py -- --renders DIR [--content content/city]
"""

import argparse
import math
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402

# (id, x, y, z, turn): buildings north of the lane face it as built (-Y), south ones turn 180 degrees.
STREET = [
    ("lane_straight", -20, 0, 0, 90), ("lane_straight", -10, 0, 0, 90), ("lane_crossing", 0, 0, 0, 0),
    ("lane_straight", 10, 0, 0, 90), ("lane_straight", 20, 0, 0, 90),
    ("saucer_diner", -15, 11, 0, 0), ("bubble_shop", -5, 9, 0, 0), ("pod_tower", 14, 11, 0, 0),
    ("pod_tower", -18, -11, 0, 180), ("bubble_shop", -7, -9, 0, 180), ("saucer_diner", 15, -11, 0, 180),
    ("hover_car_red", -9, -1.5, 1.2, 90), ("hover_car_teal", 6, 1.5, 1.6, -90),
    ("hover_car_teal", 21.6, 11, 4.9 + 6 * 3.5 + 0.4, 30),
]


def place(content, mid, x, y, z, turn):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=f"{content}/{mid}.glb")
    new = [o for o in bpy.data.objects if o not in before]
    # Blender's glTF import leaves COLOR_0 unused (Bevy multiplies it in); put the kit materials back.
    for o in new:
        if o.type != "MESH":
            continue
        if o.data.color_attributes:
            o.data.color_attributes[0].name = "Col"
        for slot in o.material_slots:
            key = slot.material.name.split(".")[0]
            if not slot.material.get("exo_kit"):
                slot.material.name = f"{key}_gltf"
            slot.material = kit.material(key)
    for o in new:
        if o.parent is None:
            o.location = (x, y, z)
            o.rotation_mode = "XYZ"
            o.rotation_euler = (0, 0, math.radians(turn))
    return new


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--renders", required=True)
    ap.add_argument("--content", default="content/city")
    args = ap.parse_args(argv)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for row in STREET:
        place(args.content, *row)
    col = kit.fresh_collection("preview")
    os.makedirs(args.renders, exist_ok=True)
    d = args.renders.rstrip("/")
    kit.stage(col)
    kit.shoot(col, f"{d}/street_overview.png", (38, -42, 30), (0, 0, 6), lens=30)
    kit.shoot(col, f"{d}/street_eye.png", (-24, -3.5, 1.7), (6, 1, 6), lens=18)
    for o in [o for o in col.objects if o.type == "LIGHT" or o.name.startswith("Plane")]:
        bpy.data.objects.remove(o)
    kit.stage(col, night=True)
    kit.shoot(col, f"{d}/street_night.png", (-24, -3.5, 1.7), (6, 1, 6), lens=18)


main()
