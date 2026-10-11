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
TODO(initiator): gulch_freeport, smugglers' yard without a tower: five garage warehouses on a ring, crate clusters, wrecks with debris, wall lines, a fence, two masts and two vans, one goofy mushroom by the main pad.
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
# Small clutter may touch each other (not buildings or keep-clear discs).
SMALL = {"crate_stack", "wrack_pile", "debris_panel", "debris_strut"}
VEHICLES = {"hover_car_red", "hover_car_teal"}

def polar(r, deg):
    a = math.radians(deg)
    return round(r * math.cos(a), 2), round(r * math.sin(a), 2)


def facing_centre(x, y):
    """Turn that makes the front (-Y at turn 0) point at the origin."""
    return round(math.degrees(math.atan2(-x, y)), 2)


def crate_cluster(cx, cy, n):
    """Up to 8 crate stacks on a 2 m grid, centred on (cx, cy)."""
    grid = [(0, 0), (2, 0), (4, 0), (0, 2), (2, 2), (4, 2), (0, 4), (2, 4)]
    return [("crate_stack", round(cx + gx - 2, 2), round(cy + gy - 2, 2), 0) for gx, gy in grid[:n]]


def gulch_layout():
    """Smugglers' yard: five garages on a ring, crate clusters, wrecks with their debris, walls, fences."""
    out = []
    for a in (110, 160, 215, 265, 320):
        x, y = polar(45, a)
        out.append(("row_garage", x, y, facing_centre(x, y)))
        out += crate_cluster(*polar(38, a + 14 if a < 300 else a - 28), 5)
    out += crate_cluster(*polar(28, 200), 5)
    out += crate_cluster(34, 24, 5)
    hulls = [(-60, -14, 30), (-32, -62, 100), (-56, 38, 137), (50, -50, 211), (6, -68, 53)]
    for hx, hy, ht in hulls:
        out.append(("hull_broken", hx, hy, ht, "props"))
    for hx, hy in [(-60, -14), (-32, -62), (-56, 38)]:
        out += [
            ("wrack_pile", hx + 6, hy + 2, 0, "props"),
            ("debris_panel", hx - 6, hy + 3, 0, "props"),
            ("debris_strut", hx + 2, hy - 7, 0, "props"),
            ("wrack_pile", hx - 5, hy - 6, 0, "props"),
        ]
    out += [("wall_broken", 66, -4 + 3 * k, 90, "props") for k in range(6)]
    out += [("wall_broken", -70, -25 + 3 * k, 90, "props") for k in range(6)]
    out += [("fence_panel", x, 72, 0) for x in range(-18, 19, 4)]
    out += [("mast", 56, -30, 0, "props"), ("mast", -64, 22, 0, "props")]
    out += [("hover_van", -20, 28, 90), ("hover_van", 24, -54, -20)]
    out += [("corner_sign", -72, 0, 0), ("mushroom_giant", 0, 30, 0, "props")]
    return out


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
        ("parking_pads", 40, 58, 0),
        ("hover_car_red", -41, 40, 0),
        ("hover_car_teal", -37, 40, 0),
        ("hover_car_teal", 40, 58, 90),
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
    "gulch_freeport": gulch_layout(),
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
            if ma in SMALL and mb in SMALL:
                continue
            if {ma, mb} & VEHICLES and "parking_pads" in (ma, mb) and {ma, mb} - VEHICLES - {"parking_pads"} == set():
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
