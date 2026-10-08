"""Hover-lane tiles on the 10 m grid. Traffic floats, so the ground is flush: a plaza of paving
with a darker lane in the middle, glowing edge strips and chevrons, and beacon lamps.

Each tile is TILE x TILE metres, centred on its origin, top of the paving at z = 0.
Measures: BRIEF.md.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/lanes.py -- --out content/city [--renders DIR] [--only id,id]
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import TILE, Part, colour  # noqa: E402

LANE = 6.0         # hover lane width
PAVING = colour("#d9d3e6")
LANE_RGB = colour("#4b5a8a")
CHEVRON = colour("#8fa0d8")
POST = colour("#f4f1e8")
GLOW = colour("#5ef2e0")
H = TILE / 2
L = LANE / 2


def plaza(p):
    p.box((-H, -H, -0.3), (H, H, 0.0), PAVING)


def lane(p, lo, hi):
    """Lane surface: a flush plate a hair above the paving."""
    p.box((lo[0], lo[1], 0.0), (hi[0], hi[1], 0.02), LANE_RGB)


def strip(p, lo, hi):
    p.box((lo[0], lo[1], 0.0), (hi[0], hi[1], 0.03), GLOW, "glow")


def chevron(p, x, y, along="Y"):
    """An arrow pointing +Y (or +X): which way the lane flows."""
    pts = [(-1.0, -0.6), (0.0, 0.2), (1.0, -0.6), (1.0, -0.2), (0.0, 0.6), (-1.0, -0.2)]
    if along == "X":
        pts = [(v, -u) for u, v in pts]
    p.prism([(x + u, y + v) for u, v in pts], 0.0, 0.025, CHEVRON)


def beacon(p, x, y):
    """Lamp: a white post with a ring halfway and a glowing ball on top."""
    p.cylinder((x, y, 0.0), 0.3, 0.3, POST, segments=12)
    p.cylinder((x, y, 0.3), 0.08, 4.6, POST, radius_top=0.05, segments=8)
    p.torus((x, y, 3.2), 0.35, 0.05, POST, segments=16, sides=6)
    p.sphere((x, y, 5.1), 0.3, GLOW, "glow", segments=12)


def straight():
    """Lane along Y, paving on both sides for walkers."""
    p = Part("lane_straight", kind="road")
    plaza(p)
    lane(p, (-L, -H), (L, H))
    for s in (-1, 1):
        strip(p, (s * L - 0.1, -H), (s * L + 0.1, H))
    chevron(p, -1.5, -2.0)
    chevron(p, 1.5, 2.0)
    beacon(p, -H + 0.8, 0.0)
    return p


def tee():
    """Lane along Y with a branch to +X."""
    p = Part("lane_tee", kind="road")
    plaza(p)
    lane(p, (-L, -H), (L, H))
    lane(p, (L, -L), (H, L))
    strip(p, (-L - 0.1, -H), (-L + 0.1, H))
    for s in (-1, 1):
        strip(p, (L - 0.1, s * L if s > 0 else -H), (L + 0.1, H if s > 0 else -L))
        strip(p, (L, s * L - 0.1), (H, s * L + 0.1))
    beacon(p, -H + 0.8, 0.0)
    return p


def crossing():
    """Four lanes meet; a glowing ring in the middle marks the hover roundabout."""
    p = Part("lane_crossing", kind="road")
    plaza(p)
    lane(p, (-L, -H), (L, H))
    lane(p, (-H, -L), (H, L))
    for sx in (-1, 1):
        for sy in (-1, 1):
            # Edge strips around each corner of paving.
            x0, x1 = sorted((sx * L, sx * H))
            y0, y1 = sorted((sy * L, sy * H))
            strip(p, (sx * L - 0.1, y0), (sx * L + 0.1, y1))
            strip(p, (x0, sy * L - 0.1), (x1, sy * L + 0.1))
    p.torus((0, 0, 0.03), 1.6, 0.06, GLOW, "glow", segments=32, sides=6)
    p.cylinder((0, 0, 0.0), 0.9, 0.06, POST, segments=16)
    beacon(p, H - 0.9, H - 0.9)
    return p


MODELS = {
    "lane_straight": (straight, (-H, -H, H, H)),
    "lane_tee": (tee, (-H, -H, H, H)),
    "lane_crossing": (crossing, (-H, -H, H, H)),
}

kit.run(MODELS, "content/city")
