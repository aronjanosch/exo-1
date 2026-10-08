"""Road tiles on the 10 m grid: carriageway in the middle, raised sidewalks at the edges.

Each tile is TILE x TILE metres, centred on its origin, top of the carriageway at z = 0.
Measures: BRIEF.md.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/roads.py -- --out content/city [--renders DIR] [--only id,id]
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import TILE, Part, colour  # noqa: E402

ROAD = 6.0         # carriageway width
WALK = 2.0         # sidewalk width each side
KERB = 0.15        # sidewalk height
ASPHALT = colour("#3a3846")
SIDEWALK = colour("#a9a2b3")
KERB_RGB = colour("#d8d2df")
LINE = colour("#f2d64e")
GLOW = colour("#5ef2e0")
H = TILE / 2


def slab(p, lo, hi):
    p.box((lo[0], lo[1], -0.3), (hi[0], hi[1], 0.0), ASPHALT)


def walk(p, lo, hi):
    """Sidewalk block with a light kerb strip along its road edge (kerb drawn as the slab's own top)."""
    p.box((lo[0], lo[1], -0.3), (hi[0], hi[1], KERB), SIDEWALK)


def dashes(p, along, at, length, rgb=LINE):
    """Centre line dashes, 1.5 m on, 1.5 m off, flush plates on the asphalt."""
    n = int(length // 3)
    for i in range(n):
        a = -length / 2 + 0.75 + i * 3
        if along == "Y":
            p.box((at - 0.08, a, 0.0), (at + 0.08, a + 1.5, 0.01), rgb)
        else:
            p.box((a, at - 0.08, 0.0), (a + 1.5, at + 0.08, 0.01), rgb)


def lamp(p, x, y, arm):
    """Street light: a post on the sidewalk, an arm over the road, a glowing bar."""
    p.cylinder((x, y, KERB), 0.09, 5.5, KERB_RGB, segments=6)
    x2 = x + arm
    p.box((min(x, x2) - 0.06, y - 0.06, KERB + 5.4), (max(x, x2) + 0.06, y + 0.06, KERB + 5.55), KERB_RGB)
    p.box((x2 - 0.4 if arm > 0 else x2, y - 0.12, KERB + 5.25), (x2 if arm > 0 else x2 + 0.4, y + 0.12, KERB + 5.4), GLOW, "glow")


def straight():
    """Runs along Y."""
    p = Part("road_straight", kind="road")
    slab(p, (-ROAD / 2, -H), (ROAD / 2, H))
    walk(p, (-H, -H), (-ROAD / 2, H))
    walk(p, (ROAD / 2, -H), (H, H))
    dashes(p, "Y", 0.0, TILE)
    lamp(p, -H + 0.6, 0.0, 2.4)
    return p


def crossing():
    """Four-way junction: corner sidewalk squares, a zebra on each arm."""
    p = Part("road_crossing", kind="road")
    slab(p, (-H, -H), (H, H))
    for sx in (-1, 1):
        for sy in (-1, 1):
            xs = sorted((sx * ROAD / 2, sx * H))
            ys = sorted((sy * ROAD / 2, sy * H))
            walk(p, (xs[0], ys[0]), (xs[1], ys[1]))
    # Zebra stripes on each arm, just inside the tile edge.
    for i in range(5):
        a = -ROAD / 2 + 0.4 + i * 1.2
        for edge in (-1, 1):
            e = edge * (H - 1.2)
            p.box((a, e - 0.6, 0.0), (a + 0.6, e + 0.6, 0.01), KERB_RGB)
            p.box((e - 0.6, a, 0.0), (e + 0.6, a + 0.6, 0.01), KERB_RGB)
    return p


def tee():
    """T junction: straight along Y, a branch to +X."""
    p = Part("road_tee", kind="road")
    slab(p, (-ROAD / 2, -H), (H, H))
    walk(p, (-H, -H), (-ROAD / 2, H))
    for sy in (-1, 1):
        ys = sorted((sy * ROAD / 2, sy * H))
        walk(p, (ROAD / 2, ys[0]), (H, ys[1]))
    dashes(p, "Y", 0.0, TILE)
    return p


MODELS = {
    "road_straight": (straight, (-H, -H, H, H)),
    "road_crossing": (crossing, (-H, -H, H, H)),
    "road_tee": (tee, (-H, -H, H, H)),
}

kit.run(MODELS, "content/city")
