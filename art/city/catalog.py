"""Contact sheets: every exported city model, lined up by family with its id on the ground.

One render per family, all under the same light, so a whole family can be reviewed at a glance.
Writes nothing into the repo. Also proves every .glb imports.

Run headless after the model scripts:
    blender -b -P art/city/catalog.py -- --renders DIR [--content content/city]

In a live Blender (runpy, see art/AGENTS.md) it lays the families out in the collection `catalog`,
south of the preview city, and renders nothing.
"""

import argparse
import glob
import math
import os
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402

LANDMARKS = {"company_hq", "beacon_spire", "saucer_diner", "pod_tower", "bubble_shop", "charge_stop"}
FAMILIES = [
    ("buildings", lambda m: m.startswith(("row_", "corner_"))),
    ("landmarks", lambda m: m in LANDMARKS),
    ("homes", lambda m: m.startswith("bungalow_")),
    ("street", lambda m: m.startswith(("lane_", "plaza_", "parking_"))),
    ("vehicles", lambda m: m.startswith("hover_")),
    ("flora", lambda m: m.startswith(("tree_", "bush_", "grass_"))),
    ("props", lambda m: True),
]
ROW_START, ROW_STEP = -170.0, -45.0   # live: rows of families south of the city


def family_of(mid):
    return next(name for name, test in FAMILIES if test(mid))


def bounds(objs):
    pts = [o.matrix_world @ Vector(c) for o in objs if o.type == "MESH" for c in o.bound_box]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    return lo, hi


def label(col, text, at):
    """The model's id written flat on the ground in front of it. Returns the label object."""
    curve = bpy.data.curves.new(f"label_{text}", "FONT")
    curve.body = text
    curve.size = 0.7
    curve.align_x = "CENTER"
    curve.extrude = 0.01
    obj = bpy.data.objects.new(f"label_{text}", curve)
    obj.location = at
    mat = bpy.data.materials.get("label") or bpy.data.materials.new("label")
    mat.diffuse_color = (0.05, 0.05, 0.08, 1)
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = (0.05, 0.05, 0.08, 1)
    curve.materials.append(mat)
    col.objects.link(obj)
    return obj


def lay_out(content, col, y_of_family):
    """Import every model and line each family up along X.
    Returns {family: (lo, hi)} and {family: [objects]}."""
    ids = sorted(os.path.basename(f)[:-4] for f in glob.glob(f"{content}/*.glb"))
    cursor, rows, members = {}, {}, {}
    for mid in ids:
        fam = family_of(mid)
        y = y_of_family(fam)
        new = kit.place(content, mid, 0, y, 0, 0, col=col)
        bpy.context.view_layer.update()
        lo, hi = bounds(new)
        # Next free x for the model and for its label (about 0.45 m per character at this size).
        model_x, label_x = cursor.get(fam, (0.0, 0.0))
        half_label = len(mid) * 0.45 / 2
        shift = max(model_x, label_x + half_label - (hi.x - lo.x) / 2) - lo.x
        for o in new:
            if o.parent is None:
                o.location.x += shift
        bpy.context.view_layer.update()
        lo, hi = bounds(new)
        cx = (lo.x + hi.x) / 2
        text = label(col, mid, (cx, min(lo.y, y) - 1.6, 0.02))
        members.setdefault(fam, []).extend(new + [text])
        gap = 3.0 if fam in ("buildings", "landmarks", "street") else 1.5
        cursor[fam] = (hi.x + gap, cx + half_label + 0.8)
        flo, fhi = rows.get(fam, (lo, hi))
        rows[fam] = (Vector(map(min, flo, lo)), Vector(map(max, fhi, hi)))
    return rows, members


def frame(col, path, lo, hi):
    """A camera in front and above, far enough that the whole row fits the 35 mm lens across."""
    c = (lo + hi) / 2
    w, h = hi.x - lo.x, hi.z - lo.z
    aspect = max(1.5, min(4.0, (w + 6) / (h * 1.3 + 8)))
    half_fov = math.atan(18 / 35)
    dist = max((w / 2 + 3) / math.tan(half_fov), (h * 0.9 + 4) * aspect / math.tan(half_fov))
    eye = c + Vector((0, -1, 0.45)).normalized() * dist
    kit.shoot(col, path, eye, (c.x, c.y, h * 0.3), lens=35, size=(2400, int(2400 / aspect)))


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--renders", required=True)
    ap.add_argument("--content", default="content/city")
    args = ap.parse_args(argv)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    col = kit.fresh_collection("catalog")
    names = [name for name, _ in FAMILIES]
    rows, members = lay_out(args.content, col, lambda fam: names.index(fam) * ROW_STEP)
    kit.stage(col)
    os.makedirs(args.renders, exist_ok=True)
    for fam, (lo, hi) in rows.items():
        # Only this family in the picture.
        for other, objs in members.items():
            for o in objs:
                o.hide_render = other != fam
        path = f"{args.renders.rstrip('/')}/catalog_{fam}.png"
        frame(col, path, lo, hi)
        print(f"CATALOG {fam}: {path}")


def live():
    content = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "content", "city")
    col = kit.fresh_collection("catalog")
    names = [name for name, _ in FAMILIES]
    rows, _ = lay_out(content, col, lambda fam: ROW_START + names.index(fam) * ROW_STEP)
    for fam, (lo, hi) in rows.items():
        print(f"CATALOG {fam}: x {lo.x:.0f}..{hi.x:.0f}, y {lo.y:.0f}..{hi.y:.0f}")


if bpy.app.background:
    main()
else:
    live()
