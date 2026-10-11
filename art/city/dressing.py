"""Dressing for the far towns (slice 1): hand-placed kit models per town, written as data.

Plain Python, no Blender. Each town gets a list of placements (model, x, y, turn_deg[, folder])
in the place frame (x east, y north, metres; origin = the town's main pad centre). Writes
content/city/dress/<place>.json and prints PROBLEM lines for overlaps, keep-clear hits, missing
models and anything beyond 80 m.

Run from the repo root: python3 -I art/city/dressing.py

Facing: as in city_plan.py, a model's front is -Y (south) at turn 0; turn 90 faces +X, 180 faces
+Y, -90 faces -X. Rotation is counter-clockwise about z. Row houses north of a street use turn 0.

TODO(initiator): bent_spoon, dusty suburb: a ring of bungalows, a beacon tower by the inspect pad, a charge stop, market stalls, parking pads with hover cars.
TODO(initiator): noodle_town, market town: a street of row houses north of the pads, a saucer diner, a pod tower, stalls, a hover bus, a billboard.
TODO(initiator): gulch_freeport, smugglers' den without a tower: garage warehouses, wrecks and debris from props, crate stacks, a fence and a van.
"""
import json
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from export_plan import glb_bounds  # noqa: E402

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
TOWN_R = 80.0
KEEP_EXTRA = 8.0
TOL = 0.1
GROUND_PREFIX = "lane_"

TOWNS = {
    "bent_spoon": [
        ("beacon_spire", -28, -32, 0),
        ("bungalow_butterfly", 0, 50, 0),
        ("bungalow_dome", -52, 22, 90),
        ("bungalow_wedge", 60, 8, -90),
        ("bungalow_butterfly", -18, -60, 180),
        ("bungalow_dome", 0, -62, 180),
        ("bungalow_dome", 48, -44, -135),
        ("bungalow_wedge", -52, -18, 90),
        ("charge_stop", 38, 38, 0),
        ("market_stall", -14, 32, 0),
        ("market_stall", 14, 32, 0),
        ("market_stall", -30, -20, 0),
        ("market_stall", 28, -24, 0),
        ("parking_pads", -40, 40, 0),
        ("hover_car_red", -40, 50, 0),
        ("hover_car_teal", -28, 40, 90),
        ("parking_pads", 40, 58, 0),
        ("hover_car_red", 30, 60, 90),
        ("tree_bulb", -42, -42, 0),
        ("tree_spiral", 58, 36, 0),
        ("tree_spiral", -66, 4, 0),
        ("tree_bulb", -70, -30, 0),
        ("tree_bulb", 30, -62, 0),
        ("lamp_arc", -22, 40, 0),
        ("lamp_arc", 22, -46, 0),
        ("lamp_arc", 60, -20, 0),
    ],
    "noodle_town": [
        *[("lane_straight", x, 40, 90) for x in range(-40, 41, 10)],
        ("row_arch", -34, 50, 0),
        ("row_bulb", -22, 50, 0),
        ("row_fin", -14, 50, 0),
        ("row_saw", -6, 50, 0),
        ("row_twin", 2, 50, 0),
        ("row_tank", 10, 50, 0),
        ("row_slant", 20, 50, 0),
        ("row_step", 30, 50, 0),
        ("row_needle", 36, 50, 0),
        ("saucer_diner", -56, 28, 0),
        ("pod_tower", -50, -20, 0),
        ("market_stall", -30, 26, 0),
        ("market_stall", -20, 26, 0),
        ("market_stall", 6, 28, 0),
        ("market_stall", 14, 28, 0),
        ("hover_bus", -12, 30, 90),
        ("billboard", -2, 66, 0),
        ("lamp_arc", -38, 30, 0),
        ("lamp_arc", -3, 30, 0),
        ("lamp_arc", 24, 30, 0),
        ("lamp_arc", -30, 31, 0),
    ],
    "gulch_freeport": [
        ("row_garage", -44, 46, 0),
        ("row_garage", -16, 52, 0),
        ("row_garage", 30, 52, 0),
        ("row_garage", 62, 28, -90),
        ("hull_broken", -60, -14, 30, "props"),
        ("hull_broken", -32, -62, 100, "props"),
        ("wrack_pile", -20, -40, 0, "props"),
        ("mast", 44, -30, 0, "props"),
        ("mushroom_giant", -50, -30, 0, "props"),
        ("wall_broken", 10, -60, 0, "props"),
        ("debris_panel", -40, 4, 0, "props"),
        ("debris_strut", 48, 22, 0, "props"),
        ("crate_stack", -38, 30, 0),
        ("crate_stack", -36, 30, 0),
        ("crate_stack", -37, 32, 0),
        ("crate_stack", -36, 42, 0),
        ("crate_stack", -34, 42, 0),
        ("crate_stack", 22, 38, 0),
        ("crate_stack", 24, 38, 0),
        ("crate_stack", 23, 40, 0),
        *[("fence_panel", x, 72, 0) for x in (-8, -4, 0, 4, 8)],
        ("corner_sign", -66, 10, 0),
        ("hover_van", -20, 28, 90),
    ],
}

def load_pads(place):
    with open(os.path.join(ROOT, "content", "place", place + ".json")) as f:
        return [(p["id"], p["at"][0], p["at"][1], p["radius_m"]) for p in json.load(f)["pads"]]


def rot(x, y, deg):
    a = math.radians(deg)
    return x * math.cos(a) - y * math.sin(a), x * math.sin(a) + y * math.cos(a)


def footprint(bounds, x, y, turn):
    """Axis-aligned box of the glb x/north rectangle, turned about z and moved to (x, y)."""
    lo, hi = bounds
    corners = [rot(cx, cy, turn) for cx in (lo[0], hi[0]) for cy in (-hi[2], -lo[2])]
    xs = [c[0] + x for c in corners]
    ys = [c[1] + y for c in corners]
    return min(xs), min(ys), max(xs), max(ys), corners


def rect_disc_dist(rect, cx, cy):
    x0, y0, x1, y1 = rect[:4]
    dx = max(x0 - cx, 0.0, cx - x1)
    dy = max(y0 - cy, 0.0, cy - y1)
    return math.hypot(dx, dy)


def check_town(place, items):
    pads = load_pads(place)
    problems = []
    fps = []
    out = []
    for it in items:
        model, x, y, turn = it[:4]
        folder = it[4] if len(it) > 4 else "city"
        base = os.path.join(ROOT, "content", folder)
        path = os.path.join(base, model + ".glb")
        if not os.path.exists(path):
            problems.append(f"missing glb {path}")
            continue
        bounds = glb_bounds(path)
        x0, y0, x1, y1, corners = footprint(bounds, x, y, turn)
        fps.append((model, (x0, y0, x1, y1), folder))
        if max(math.hypot(cx + x, cy + y) for cx, cy in corners) > TOWN_R:
            problems.append(f"{model} at ({x},{y}) turn {turn} reaches beyond {TOWN_R:.0f} m")
        entry = {"model": model, "at": [round(x, 2), round(y, 2), 0], "turn_deg": round(turn, 2)}
        if folder == "props":
            entry["folder"] = "props"
        out.append((entry, bounds))
    for pid, px, py, r in pads:
        keep = r + KEEP_EXTRA
        for model, rect, _ in fps:
            if rect_disc_dist(rect, px, py) < keep:
                problems.append(f"{model} {tuple(round(v, 2) for v in rect)} touches keep-clear of pad {pid}")
        for i in range(5):
            sx, sy = px + 2 * i, py + r + 1.5
            for model, rect, _ in fps:
                if rect[0] <= sx <= rect[2] and rect[1] <= sy <= rect[3]:
                    problems.append(f"buyer spot {pid} i={i} ({sx},{sy}) inside {model}")
    for a in range(len(fps)):
        for b in range(a + 1, len(fps)):
            ma, ra, _ = fps[a]
            mb, rb, _ = fps[b]
            if ma.startswith(GROUND_PREFIX) and mb.startswith(GROUND_PREFIX):
                continue
            ox = min(ra[2], rb[2]) - max(ra[0], rb[0])
            oy = min(ra[3], rb[3]) - max(ra[1], rb[1])
            if ox > TOL and oy > TOL:
                problems.append(f"{ma} overlaps {mb}")
    return out, problems


def models_of(entries):
    models = {}
    for entry, bounds in entries:
        key = entry["model"]
        models[key] = {"min": bounds[0], "max": bounds[1]}
    return dict(sorted(models.items()))


def main():
    bad = 0
    out_dir = os.path.join(ROOT, "content", "city", "dress")
    os.makedirs(out_dir, exist_ok=True)
    for place, items in TOWNS.items():
        entries, problems = check_town(place, items)
        for p in problems:
            print(f"PROBLEM {place}: {p}")
        bad += len(problems)
        doc = {
            "_comment": "Generated by art/city/dressing.py; do not edit by hand.",
            "place": place,
            "placements": [e for e, _ in entries],
            "models": models_of(entries),
            "quests": [],
        }
        path = os.path.join(out_dir, place + ".json")
        with open(path, "w") as f:
            json.dump(doc, f, indent=1)
            f.write("\n")
        print(f"DRESS {place}: {len(entries)} placements -> {os.path.relpath(path, ROOT)}")
    if bad == 0:
        print("OK no PROBLEM lines")


if __name__ == "__main__":
    main()
