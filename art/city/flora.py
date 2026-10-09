"""Alien flora for streets, plazas and back yards: round, soft, a little glowing.

Each plant stands on its origin. Colours stay in the city's palette: teal and plum leaves,
pink and cyan glow pods. Names are placeholders. Look: BRIEF.md.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/flora.py -- --out content/city [--renders DIR] [--only id,id]
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import Part, colour  # noqa: E402

BARK = colour("#6a5248")
STEM = colour("#5f8f4a")
LEAF_TEAL = colour("#4fa39a")
LEAF_PLUM = colour("#8a5fa8")
LEAF_LIME = colour("#9cc46a")
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")


def tree_bulb():
    """A tree of three fat bulbs on a short trunk, glow pods hanging under the biggest one."""
    p = Part("tree_bulb", "prop")
    p.cylinder((0, 0, 0), 0.35, 0.4, BARK, radius_top=0.25, segments=10)
    p.cylinder((0, 0, 0.4), 0.25, 3.2, BARK, radius_top=0.14, segments=10)
    for x, y, z, r, c in ((0.0, 0.0, 4.2, 1.5, LEAF_TEAL), (0.9, 0.3, 5.4, 1.0, LEAF_TEAL), (-0.6, -0.4, 5.9, 0.8, LEAF_LIME)):
        p.sphere((x, y, z), r, c, squash=0.85, segments=14)
    for a in range(0, 360, 72):
        x, y = 1.1 * math.cos(math.radians(a)), 1.1 * math.sin(math.radians(a))
        p.cylinder((x, y, 2.9), 0.02, 0.6, STEM, segments=5)
        p.sphere((x, y, 2.85), 0.13, GLOW_PINK, "glow", segments=8)
    return p


def tree_spiral():
    """An alien conifer: a thin stem through stacked, shrinking discs that turn a little each."""
    p = Part("tree_spiral", "prop")
    p.cylinder((0, 0, 0), 0.18, 6.4, BARK, radius_top=0.06, segments=8)
    for k in range(6):
        z = 1.6 + k * 0.85
        r = 1.6 - k * 0.22
        off = 0.15 * math.cos(k * 1.3), 0.15 * math.sin(k * 1.3)
        p.cylinder((off[0], off[1], z), r, 0.3, LEAF_PLUM if k % 2 else LEAF_TEAL, radius_top=r * 0.7, segments=14)
    p.sphere((0, 0, 6.6), 0.25, GLOW_CYAN, "glow", segments=10)
    return p


def bush_puff():
    """A cluster of soft puffs with two glow berries."""
    p = Part("bush_puff", "prop")
    for x, y, r, c in ((0.0, 0.0, 0.7, LEAF_TEAL), (0.55, 0.2, 0.5, LEAF_LIME), (-0.5, 0.15, 0.55, LEAF_TEAL),
                       (0.1, -0.45, 0.45, LEAF_PLUM)):
        p.sphere((x, y, r * 0.75), r, c, squash=0.75, segments=12)
    p.sphere((0.3, -0.5, 0.75), 0.09, GLOW_PINK, "glow", segments=8)
    p.sphere((-0.6, -0.3, 0.6), 0.08, GLOW_CYAN, "glow", segments=8)
    return p


def grass_tuft():
    """A tuft of thick blades, each a thin cone leaning out."""
    p = Part("grass_tuft", "prop")
    p.cylinder((0, 0, 0), 0.25, 0.08, STEM, segments=10)
    for k in range(7):
        a = math.radians(k * 360 / 7)
        x, y = 0.12 * math.cos(a), 0.12 * math.sin(a)
        h = 0.5 + 0.15 * ((k * 3) % 4)
        p.cylinder((x, y, 0.05), 0.05, h, LEAF_LIME if k % 2 else LEAF_TEAL, radius_top=0.0, segments=5)
    p.sphere((0, 0, 0.75), 0.06, GLOW_CYAN, "glow", segments=8)
    return p


MODELS = {
    "tree_bulb": (tree_bulb, (-1.7, -1.7, 1.9, 1.7)),
    "tree_spiral": (tree_spiral, (-1.8, -1.8, 1.8, 1.8)),
    "bush_puff": (bush_puff, (-1.1, -0.95, 1.1, 0.8)),
    "grass_tuft": (grass_tuft, (-0.3, -0.3, 0.3, 0.3)),
}

kit.run(MODELS, "content/city")
