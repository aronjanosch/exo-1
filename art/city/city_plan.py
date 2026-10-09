"""The first city on Hearth, as data: where every model stands, where the roads run, where the
ground is paved, where quest givers wait. No Blender here, so the game can read the same plan
later. street_preview.py builds it in Blender.

Districts (placeholder names):
- Downtown: dense, on the 10 m lane grid, around the origin. Shops, the plaza, the company.
- The arterial: a long curved road east, with pylons, trees and the charge stop.
- Ringside: a suburb on a loop road, bungalows around a park with a pond, a short shop street.
- The west road: a stub towards the future spaceport, ending at a gate.

A placement is (id, x, y, z, turn): turn is the yaw in degrees; models face -Y as built, so
turn 0 faces -Y, 180 faces +Y, 90 faces +X, -90 faces -X. A building with a ground-floor room
carries a sixth element, {"fit": kind, "role": role}: which fit-out (interiors.py) furnishes the
room and who stands behind the counter (role None: the fit-out's default). `fit()` sets it per
placement; every other building gets its model's default from FIT at the end of this file.
"""

import math
import random

LANE, WALK = 3.0, 5.0          # half widths: hover lane, lane plus walkway
TOWER_TOP = 4.5 + 7 * 3.5 + 0.4
GARAGE_TOP = 4.5 + 1 * 3.5 + 0.4

# Lot extents along a model's own X axis, for lining models up along a road.
ROW_BAYS = {"row_step": 2, "row_butterfly": 2, "row_arch": 3, "row_fin": 1, "row_tower": 2, "row_tank": 2,
            "row_bulb": 2, "row_slant": 3, "row_needle": 1, "row_garage": 3, "row_saw": 2, "row_twin": 2}
LOT_X = {
    **{mid: (-2.0 * b, 2.0 * b) for mid, b in ROW_BAYS.items()},
    "bungalow_butterfly": (-6.8, 11.4), "bungalow_dome": (-5.6, 10.2), "bungalow_wedge": (-5.5, 10.4),
    "charge_stop": (-9.1, 5.0),
}


# ---------------------------------------------------------------- geometry helpers

def smooth(points, steps=6, closed=False):
    """Catmull-Rom through the control points: a road that bends instead of kinks."""
    pts = list(points)
    n = len(pts)
    out = []
    last = n if closed else n - 1
    for i in range(last):
        p0 = pts[i - 1] if (i > 0 or closed) else pts[0]
        p1, p2 = pts[i], pts[(i + 1) % n]
        p3 = pts[(i + 2) % n] if (i + 2 < n or closed) else pts[-1]
        for k in range(steps):
            t = k / steps
            t2, t3 = t * t, t * t * t
            out.append(tuple(0.5 * (2 * p1[c] + (-p0[c] + p2[c]) * t + (2 * p0[c] - 5 * p1[c] + 4 * p2[c] - p3[c]) * t2
                                    + (-p0[c] + 3 * p1[c] - 3 * p2[c] + p3[c]) * t3) for c in (0, 1)))
    if not closed:
        out.append(pts[-1])
    return out


def ellipse(cx, cy, rx, ry, n=40):
    return [(cx + rx * math.cos(2 * math.pi * k / n), cy + ry * math.sin(2 * math.pi * k / n)) for k in range(n)]


def at(points, s, closed=False):
    """Point and unit direction at arc length s along a polyline (wrapping around a closed one)."""
    segs = list(zip(points, points[1:] + (points[:1] if closed else [])))
    if closed:
        s %= length_of(points, closed)
    for a, b in segs:
        length = math.dist(a, b)
        if s <= length or (a, b) == segs[-1]:
            t = min(1.0, s / length) if length else 0.0
            d = ((b[0] - a[0]) / length, (b[1] - a[1]) / length)
            return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t), d
        s -= length
    raise ValueError("empty polyline")


def length_of(points, closed=False):
    segs = zip(points, points[1:] + (points[:1] if closed else []))
    return sum(math.dist(a, b) for a, b in segs)


def facing(nx, ny):
    """Yaw that turns a model's front (-Y) away from the normal (nx, ny), i.e. towards the road."""
    return math.degrees(math.atan2(-nx, ny))


def frontage(points, s0, ids, side, setback, gap=0.0, closed=False):
    """Line models up along a road from arc length s0, on the left (side=1) or right (-1), fronts
    `setback` metres from the centre line, facing the road. Returns placements."""
    out = []
    s = s0
    for mid in ids:
        x0, x1 = LOT_X[mid]
        (px, py), (dx, dy) = at(points, s + (x1 - x0) / 2, closed)
        nx, ny = -dy * side, dx * side                      # away from the road
        turn = facing(nx, ny)
        # The model's +X in the world, to put the lot's centre on the road point.
        ax, ay = math.cos(math.radians(turn)), math.sin(math.radians(turn))
        shift = -(x0 + x1) / 2
        out.append((mid, px + nx * setback + ax * shift, py + ny * setback + ay * shift, 0, turn))
        s += (x1 - x0) + gap
    return out


def cut(points, s0, s1, closed=False):
    """The part of a polyline between arc lengths s0 and s1 (s1 may pass the end of a closed one)."""
    total = length_of(points, closed)
    out = [at(points, s0, closed)[0]]
    step = 1.0
    s = s0 + step
    while s < s1:
        out.append(at(points, s, closed)[0])
        s += step
    out.append(at(points, min(s1, s1 if closed else total), closed)[0])
    return out


def beside(points, s, mid, side, offset, z=0.0, closed=False):
    """One model beside a road at arc length s, `offset` metres out, facing the road."""
    (px, py), (dx, dy) = at(points, s, closed)
    nx, ny = -dy * side, dx * side
    return (mid, px + nx * offset, py + ny * offset, z, facing(nx, ny))


def scatter(points, ids, every, side, offset, seed, closed=False, start=0.0, end=None, jitter=1.0):
    """Props or plants along a road at a fixed spacing, with a little seeded jitter."""
    rnd = random.Random(seed)
    out = []
    total = length_of(points, closed) if end is None else end
    s = start
    while s < total:
        (px, py), (dx, dy) = at(points, s, closed)
        nx, ny = -dy * side, dx * side
        o = offset + rnd.uniform(-jitter, jitter)
        out.append((rnd.choice(ids), px + nx * o, py + ny * o, 0, rnd.uniform(0, 360)))
        s += every * rnd.uniform(0.8, 1.2)
    return out


def grove(cx, cy, rx, ry, n, seed, ids=("tree_bulb", "tree_spiral", "tree_spiral", "bush_puff", "grass_tuft")):
    """A loose clump of plants in an ellipse: the green between districts."""
    rnd = random.Random(seed)
    out = []
    for _ in range(n):
        a, r = rnd.uniform(0, 2 * math.pi), math.sqrt(rnd.random())
        out.append((rnd.choice(ids), cx + rx * r * math.cos(a), cy + ry * r * math.sin(a), 0, rnd.uniform(0, 360)))
    return out


def pylons(points, every, side, offset, start=0.0, end=None):
    """Pylons along a road, arms across it; returns (placements, cable run)."""
    out, run = [], []
    total = length_of(points) if end is None else end
    s = start
    while s <= total:
        (px, py), (dx, dy) = at(points, s)
        nx, ny = -dy * side, dx * side
        turn = math.degrees(math.atan2(dy, dx)) + 90
        x, y = px + nx * offset, py + ny * offset
        out.append(("lane_pylon", x, y, 0, turn))
        run.append((x, y, turn))
        s += every
    return out, run


def fit(placements, kinds, roles=None):
    """Give buildings their fit-out: kinds (and roles) in the same order as the placements."""
    roles = roles or [None] * len(kinds)
    return [(*p[:5], {"fit": k, "role": r}) for p, k, r in zip(placements, kinds, roles, strict=True)]


# ---------------------------------------------------------------- downtown (lane grid)

DOWNTOWN = [
    # Main street, now open at both ends into the arterial and the west road.
    ("lane_straight", -40, 0, 0, 90), ("lane_straight", -30, 0, 0, 90), ("lane_straight", -20, 0, 0, 90),
    ("lane_straight", -10, 0, 0, 90), ("lane_crossing", 0, 0, 0, 0), ("lane_straight", 10, 0, 0, 90),
    ("lane_straight", 20, 0, 0, 90), ("lane_straight", 30, 0, 0, 90), ("lane_straight", 40, 0, 0, 90),
    # Cross street, the curve and the northern stub.
    ("lane_straight", 0, 10, 0, 0), ("lane_straight", 0, 20, 0, 0), ("lane_curve", 0, 30, 0, 0),
    ("lane_straight", 10, 30, 0, 90), ("lane_end", 20, 30, 0, -90),
    ("lane_straight", 0, -10, 0, 0), ("lane_straight", 0, -20, 0, 0), ("lane_straight", 0, -30, 0, 0),
    ("lane_crossing", 0, -40, 0, 0), ("lane_end", 0, -50, 0, 180),
    # South street, parallel to the main street.
    ("lane_end", -50, -40, 0, 90), *[("lane_straight", x, -40, 0, 90) for x in (-40, -30, -20, -10, 10, 20, 30, 40)],
    ("lane_end", 50, -40, 0, -90),
    # NW block.
    ("corner_sign", -9, 5, 0, 0), ("row_step", -17, 5, 0, 0), ("row_fin", -23, 5, 0, 0),
    ("row_bulb", -29, 5, 0, 0), ("row_tank", -37, 5, 0, 0),
    ("row_saw", -5, 21, 0, 90), ("row_tower", -5, 29, 0, 90),
    # NE block: the plaza.
    ("plaza_tile", 10, 10, 0, 0), ("plaza_tile", 20, 10, 0, 0), ("plaza_tile", 10, 20, 0, 0),
    ("plaza_tile", 20, 20, 0, 0), ("beacon_spire", 15, 15, 0, 0), ("company_hq", 27, 15.3, 0, -90),
    ("market_stall", 7.5, 11, 0, 90), ("market_stall", 7.5, 16, 0, 90), ("market_stall", 7.5, 21, 0, 90),
    ("bench_float", 15, 8.5, 0, 180), ("bench_float", 15, 21.5, 0, 0),
    ("planter_blob", 11, 19, 0, 0), ("planter_blob", 19, 11, 0, 0), ("booth_tele", 22.5, 7.5, 0, 0),
    ("robot_sweeper", 12, 10, 0, 30), ("vending_tube", 24.5, 22.5, 0, -90),
    ("tree_bulb", 22, 22, 0, 0), ("tree_spiral", 11.5, 23, 0, 0), ("grass_tuft", 19.5, 19.5, 0, 0),
    ("grass_tuft", 10.5, 11.5, 0, 40), ("billboard", 12, 37, 0, 0),
    # SW block.
    ("corner_turret", -11, -5, 0, 180), ("row_twin", -21, -5, 0, 180), ("row_needle", -27, -5, 0, 180),
    ("row_arch", -35, -5, 0, 180), ("row_garage", -5, -23, 0, 90),
    # SE block.
    ("row_slant", 11, -5, 0, 180), ("saucer_diner", 25, -12, 0, 180),
    ("parking_pads", 37, -10, 0, 0), ("row_butterfly", 5, -21, 0, -90),
    # Back yards.
    *[("fence_panel", x, 25.6, 0, 0) for x in (-39, -35, -31, -27, -23, -19)],
    ("crate_stack", -30, 18, 0, 10), ("tree_spiral", -36, 21, 0, 0), ("bush_puff", -24, 22, 0, 0),
    ("bin_bot", -20, 17.5, 0, 180), ("grass_tuft", -33, 23, 0, 0), ("grass_tuft", -27, 19, 0, 0),
    ("tree_bulb", -33, -20, 0, 0), ("crate_stack", -23, -18, 0, 80), ("bush_puff", -38, -19, 0, 0),
    ("roof_dish", -19, -21, 0, 150), ("grass_tuft", -28, -22, 0, 0),
    # Walkways.
    ("lamp_arc", -31, 4.6, 0, 0), ("lamp_arc", -19, 4.6, 0, 0), ("lamp_arc", 13, -4.6, 0, 180),
    ("lamp_arc", -23, -4.6, 0, 180), ("lamp_arc", -4.6, 15, 0, -90), ("lamp_arc", 4.6, -15, 0, 90),
    ("bin_bot", -18.6, 4.4, 0, 0), ("vending_tube", -13.6, 4.4, 0, 0), ("mail_tube", -27.5, 4.4, 0, 0),
    ("bench_float", -33, -3.9, 0, 180), ("bin_bot", -15, -4.4, 0, 180), ("mail_tube", 9, -4.4, 0, 180),
    ("sign_post", 4.2, -4.2, 0, 0), ("sign_post", -4.2, 4.2, 0, 180), ("booth_tele", -4.2, -10, 0, 90),
    ("robot_sweeper", -12, 3.8, 0, 200), ("planter_blob", 30, -4.3, 0, 0), ("bush_puff", 33, -4.2, 0, 0),
    # Traffic.
    ("hover_bus", 16, 1.5, 1.4, -90), ("hover_car_red", -9, -1.5, 1.2, 90), ("hover_car_teal", 8, 1.5, 1.6, -90),
    ("hover_van", -1.5, -23, 1.0, 0), ("hover_car_teal", 1.5, 14, 1.3, 180),
    ("hover_car_red", 34.5, -10, 0.4, 0), ("hover_van", 39.5, -10, 0.4, 180),
    ("hover_car_teal", -10.5, 29, TOWER_TOP + 1.8 + 0.4, 30),
    ("hover_car_red", -10.5, -23, GARAGE_TOP + 1.8 + 0.4, -60),
]
_SOUTH = [(-45.0, -40.0), (45.0, -40.0)]
DOWNTOWN += [
    # North side of the south street (backs onto the SW and SE back yards).
    *fit(frontage(_SOUTH, 0.0, ["row_saw", "row_bulb", "row_fin", "row_step"], 1, WALK),
         ["office", "bar", "workshop", "shop"]),
    *frontage(_SOUTH, 50.0, ["row_slant", "row_twin", "row_arch", "row_tank"], 1, WALK),
    # South side: rows in the west, the round shops in the east as a little square of their own.
    *frontage(_SOUTH, 0.0, ["row_tower", "row_needle", "row_arch", "row_saw", "row_fin"], -1, WALK),
    ("bubble_shop", 12.0, -51.0, 0, 180), ("pod_tower", 30.0, -53.0, 0, 180),
    ("plaza_tile", 20, -50, 0, 0), ("bench_float", 20, -47.5, 0, 180), ("tree_bulb", 21, -56, 0, 0),
    ("lamp_arc", -30, -44.6, 0, 180), ("lamp_arc", 25, -35.4, 0, 0), ("bin_bot", -12, -35.6, 0, 0),
    ("vending_tube", 6.5, -44.5, 0, 180), ("hover_car_teal", -22, -38.5, 1.3, 90),
    *[("fence_panel", x, -60.6, 0, 0) for x in (-43, -39, -35, -31, -27, -23, -19, -15, -11, -7)],
]
DOWNTOWN_PYLONS = [((-35, -25, -15), 3.3), ((15, 25, 35), 3.3), ((15, 25, 35), -3.3)]

# ---------------------------------------------------------------- roads off the grid

ARTERIAL = smooth([(45, 0), (70, 0), (95, -6), (118, -22), (140, -45), (165, -68), (190, -80)])
RING_C, RING_R = (235.0, -80.0), (45.0, 30.0)
RING = ellipse(*RING_C, *RING_R, n=48)
SHOP_STREET = [(235.0, -50.0), (235.0, -20.0)]
CUL_DE_SAC = smooth([(235.0, -110.0), (236.0, -128.0), (246.0, -146.0), (264.0, -152.0)])
CUL_END = (268.0, -153.0, 8.0)   # x, y, radius of the turnaround
WEST_ROAD = smooth([(-45, 0), (-75, 0), (-100, 6), (-125, 18), (-150, 24)])

def _ring_at(angle_deg):
    return length_of(RING, closed=True) * (angle_deg % 360) / 360


# edges: where the glowing edge strips run, as (side, from, to) in arc length; side 1 is the left
# edge. Strips stop where another road joins, so they do not cross its lane.
JOIN = 6.0   # metres of edge left open each side of a junction
_ring_len_all = length_of(RING, closed=True)
ROADS = [
    {"points": ARTERIAL, "closed": False,
     "edges": [(1, 0.0, length_of(ARTERIAL) - JOIN), (-1, 0.0, length_of(ARTERIAL) - JOIN)]},
    {"points": RING, "closed": True,
     "edges": [(1, 0.0, _ring_len_all)]
              + [(-1, _ring_at(a) + JOIN * 1.4, _ring_at(b) - JOIN * 1.4) for a, b in ((90, 180), (180, 270))]
              + [(-1, _ring_at(270) + JOIN * 1.4, _ring_at(90) + _ring_len_all - JOIN * 1.4)]},
    {"points": SHOP_STREET, "closed": False, "edges": [(1, JOIN, 30.0), (-1, JOIN, 30.0)]},
    {"points": CUL_DE_SAC, "closed": False,
     "edges": [(1, JOIN, length_of(CUL_DE_SAC)), (-1, JOIN, length_of(CUL_DE_SAC))]},
    {"points": WEST_ROAD, "closed": False,
     "edges": [(1, 0.0, length_of(WEST_ROAD)), (-1, 0.0, length_of(WEST_ROAD))]},
]

# ---------------------------------------------------------------- ground

PAVING = [
    [(-46, -32), (46, -32), (46, 38), (-46, 38)],                 # downtown
    [(222, -52), (248, -52), (248, -10), (222, -10)],             # Ringside shop street
    [(-56, -62), (56, -62), (56, -32), (-56, -32)],               # downtown, south street
]
POND = ellipse(235.0, -86.0, 14.0, 8.0, n=32)

# ---------------------------------------------------------------- the arterial

_art_pylons, _art_run = pylons(ARTERIAL, 24.0, -1, 4.3, start=8.0)
ARTERIAL_ITEMS = [
    *_art_pylons,
    *grove(80, 30, 22, 12, 22, "grove-arterial-north"), *grove(110, -60, 18, 14, 20, "grove-arterial-south"),
    *grove(175, -25, 14, 10, 14, "grove-arterial-east"), *grove(60, -75, 20, 12, 16, "grove-south"),
    *scatter(ARTERIAL, ["tree_bulb", "tree_spiral", "bush_puff"], 14.0, 1, 8.0, seed="arterial-left", start=6.0),
    *scatter(ARTERIAL, ["grass_tuft", "bush_puff", "tree_spiral"], 18.0, -1, 8.5, seed="arterial-right", start=15.0),
    # The charge stop on the arterial's north side, its forecourt reaching to the walkway.
    *frontage(ARTERIAL, 112.0, ["charge_stop"], 1, WALK + 6.2),
    beside(ARTERIAL, 122.0, "hover_van", 1, 8.0, z=0.4),
    ("billboard", 100.0, 8.0, 0, 0), ("sign_post", 52.0, 4.2, 0, 0), ("hover_car_teal", 120.0, -24.0, 1.4, 55),
]

# ---------------------------------------------------------------- Ringside

_ring_len = length_of(RING, closed=True)


def _ring_s(angle_deg):
    """Arc length on the ring at an angle from its centre (the ring starts at angle 0, counterclockwise)."""
    return _ring_len * (angle_deg % 360) / 360


_homes = ["bungalow_butterfly", "bungalow_dome", "bungalow_wedge"]
RINGSIDE = [
    # Bungalows around the outside of the ring, gaps where the shop street (90 degrees), the
    # arterial (180 degrees) and the cul-de-sac (270 degrees) join.
    *frontage(RING, _ring_s(112), [_homes[2], _homes[1]], -1, 9.0, gap=4.0, closed=True),
    *frontage(RING, _ring_s(196), [_homes[0], _homes[2]], -1, 9.0, gap=4.0, closed=True),
    *frontage(RING, _ring_s(288), [_homes[1], _homes[0], _homes[2], _homes[1], _homes[0]], -1, 9.0, gap=4.0,
              closed=True),
    # The shop street: a turnaround at its end, shops on both sides.
    ("lane_end", 235, -15, 0, 0),
    *fit(frontage(SHOP_STREET, 6.0, ["row_garage", "row_tank", "row_fin"], 1, WALK),
         ["workshop", "shop", "office"], ["mechanic", "trader", "quest"]),
    *fit(frontage(SHOP_STREET, 6.0, ["row_slant", "row_step"], -1, WALK), ["bar", "shop"]),
    # The park inside the ring.
    ("bench_float", 235, -75.5, 0, 0), ("bench_float", 222, -86, 0, 90), ("bench_float", 248, -86, 0, -90),
    ("tree_bulb", 215, -78, 0, 0), ("tree_bulb", 254, -92, 0, 0), ("tree_spiral", 220, -96, 0, 0),
    ("tree_spiral", 252, -72, 0, 0), ("tree_bulb", 238, -98, 0, 0), ("bush_puff", 226, -72, 0, 30),
    ("bush_puff", 246, -98, 0, 0), ("planter_blob", 230, -74, 0, 0), ("planter_blob", 240, -74, 0, 0),
    ("market_stall", 212, -88, 0, 90), ("market_stall", 212, -82, 0, 90), ("robot_sweeper", 228, -97, 0, 0),
    *[("grass_tuft", 235 + 17 * math.cos(a), -86 + 10 * math.sin(a), 0, 0) for a in (0.3, 1.1, 2.4, 3.3, 4.2, 5.4)],
    *scatter(RING, ["bush_puff", "grass_tuft", "grass_tuft"], 11.0, -1, 6.6, seed="ring-out",
             closed=True, jitter=0.4),
    *scatter(RING, ["grass_tuft", "bush_puff"], 20.0, 1, 6.5, seed="ring-in", closed=True, jitter=0.5),
    ("mail_tube", 229.5, -45, 0, 90), ("bin_bot", 240.6, -40, 0, -90), ("lamp_arc", 230.4, -30, 0, 90),
    ("lamp_arc", 239.6, -38, 0, -90),
    ("hover_car_red", 233.5, -35, 1.3, 0), ("hover_bus", 195.0, -95.0, 1.4, 150),
    # The cul-de-sac south of the ring.
    *frontage(CUL_DE_SAC, 12.0, [_homes[1], _homes[0]], 1, 9.0, gap=4.0),
    *frontage(CUL_DE_SAC, 12.0, [_homes[2], _homes[1]], -1, 9.0, gap=4.0),
    *scatter(CUL_DE_SAC, ["bush_puff", "grass_tuft", "tree_spiral"], 9.0, 1, 6.6, seed="cul-left", jitter=0.4,
             start=10.0),
    *scatter(CUL_DE_SAC, ["grass_tuft", "bush_puff"], 10.0, -1, 6.6, seed="cul-right", jitter=0.4, start=10.0),
    # Groves around Ringside.
    *grove(300, -70, 14, 24, 26, "grove-east"), *grove(200, -140, 22, 14, 22, "grove-south-west"),
    *grove(285, -130, 14, 10, 14, "grove-south-east"), *grove(270, -20, 18, 10, 16, "grove-north-east"),
]

# ---------------------------------------------------------------- the west road and the spaceport gate

def _gate():
    """A fence across the end of the west road, a billboard behind it facing back down the road."""
    (ex, ey), (dx, dy) = at(WEST_ROAD, length_of(WEST_ROAD))
    nx, ny = -dy, dx
    across = math.degrees(math.atan2(ny, nx))
    out = [("fence_panel", ex + nx * k * 4 + dx, ey + ny * k * 4 + dy, 0, across) for k in (-1.5, -0.5, 0.5, 1.5)]
    out.append(("billboard", ex + dx * 7, ey + dy * 7, 0, facing(dx, dy)))
    out.append(("sign_post", ex - dx * 3 + nx * 6, ey - dy * 3 + ny * 6, 0, 0))
    out.append(("booth_tele", ex - dx * 4 - nx * 6.5, ey - dy * 4 - ny * 6.5, 0, facing(-nx, -ny)))
    return out


WEST = [
    *scatter(WEST_ROAD, ["tree_spiral", "bush_puff", "grass_tuft"], 15.0, 1, 7.5, seed="west-left", start=8.0),
    *scatter(WEST_ROAD, ["tree_bulb", "grass_tuft"], 17.0, -1, 7.5, seed="west-right", start=12.0),
    # Outskirts: a few homes along the road, groves behind them.
    *frontage(WEST_ROAD, 22.0, ["bungalow_wedge"], 1, 9.0), *frontage(WEST_ROAD, 40.0, ["bungalow_dome"], -1, 9.0),
    *frontage(WEST_ROAD, 64.0, ["bungalow_butterfly"], 1, 9.0),
    *grove(-95, 35, 20, 12, 22, "grove-west-north"), *grove(-110, -18, 24, 10, 20, "grove-west-south"),
    *grove(-60, 55, 16, 10, 14, "grove-north"),
    # The gate: the road ends at a fence and a billboard; the spaceport comes later.
    *_gate(),
]

# ---------------------------------------------------------------- everything

PLACEMENTS = DOWNTOWN + ARTERIAL_ITEMS + RINGSIDE + WEST

# Cable runs: lists of (x, y, pylon turn); cables hang between neighbours.
CABLE_RUNS = [[(x, y, 90) for x in xs] for xs, y in DOWNTOWN_PYLONS] + [_art_run]
PLACEMENTS += [("lane_pylon", x, y, 0, turn) for run in CABLE_RUNS[:len(DOWNTOWN_PYLONS)] for x, y, turn in run]

# Quest givers (placeholders): where the game puts a character with a quest.
_gx, _gy = at(WEST_ROAD, length_of(WEST_ROAD) - 4.0)[0]
_cs = beside(ARTERIAL, 104.0, "npc_marker", 1, 6.5)
QUESTS = [
    ("plaza_regular", 11.5, 13.0), ("back_yard_dealer", -30.0, 21.0), ("garage_mechanic", -3.8, -20.0),
    ("diner_cook", 21.0, -4.0), ("charge_stop_clerk", _cs[1], _cs[2]), ("pond_watcher", 235.0, -76.5),
    ("shop_street_courier", 233.0, -26.0), ("gatekeeper", _gx, _gy + 3.5),
]
PLACEMENTS += [("npc_marker", x, y, 0, 0) for _, x, y in QUESTS]

# Fit-out per building model unless a placement says otherwise (interiors.py: shop, bar, workshop,
# office). Placeholders until the initiator names the shops.
FIT = {
    "row_step": "bar", "row_butterfly": "shop", "row_arch": "office", "row_fin": "shop", "row_tower": "office",
    "row_tank": "bar", "row_bulb": "shop", "row_slant": "shop", "row_needle": "office", "row_garage": "workshop",
    "row_saw": "workshop", "row_twin": "bar", "corner_turret": "bar", "corner_sign": "shop",
}
PLACEMENTS = [p if len(p) > 5 or p[0] not in FIT else (*p, {"fit": FIT[p[0]], "role": None}) for p in PLACEMENTS]

GRASS = (0.25, 0.36, 0.22)   # linear rgb of the ground between districts
