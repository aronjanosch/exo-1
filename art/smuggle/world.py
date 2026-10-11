"""Smuggling-quarter world pieces: the inspection arch, a small control tower and the patrol craft.

Each model stands on its origin (ground centre, +Y front, metres). Look: the city kit's flat vertex
colours and muted bodies with saturated accents, cyan and pink glows, goofy retro-future.

Run headless (writes content/smuggle/<id>.glb, renders optional):
    blender -b --python-exit-code 1 -P art/smuggle/world.py -- --out content/smuggle --renders DIR [--only id,id]
"""

import math
import os
import sys

from mathutils import Matrix

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import Part, colour  # noqa: E402

TRIM = colour("#f4f1e8")
METAL = colour("#9aa7b8")
DARK = colour("#3b3f4a")
GLASS = colour("#2a4a6b")
TEAL = colour("#2f7f86")
MUSTARD = colour("#d9a441")
RUST = colour("#b8573f")
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")
GLOW_RED = colour("#ff3a3a")
RED = colour("#ff3a3a")
BLUE = colour("#3a8bff")


def stroke(p, pts, a, b, y0, y1, rgb, mat="paint"):
    """A band in the front view (XZ) along the polyline `pts` of (x, z) points, from offset a to offset b
    along the left normal (for a path that runs clockwise around the arch, negative is inward).
    Corners are mitred so the band keeps its width. Extruded in y from y0 to y1."""
    n = len(pts)
    rows = []
    for i, (x, z) in enumerate(pts):
        segs = []
        if i > 0:
            segs.append((x - pts[i - 1][0], z - pts[i - 1][1]))
        if i < n - 1:
            segs.append((pts[i + 1][0] - x, pts[i + 1][1] - z))
        norms = []
        for dx, dz in segs:
            ln = math.hypot(dx, dz)
            norms.append((-dz / ln, dx / ln))
        nx = sum(q[0] for q in norms)
        nz = sum(q[1] for q in norms)
        ln = math.hypot(nx, nz)
        nx, nz = nx / ln, nz / ln
        scale = 1.0 / max(0.5, nx * norms[0][0] + nz * norms[0][1])
        rows.append([(x + nx * off * scale, z + nz * off * scale) for off in (a, b)])
    outline = [r[0] for r in rows] + [r[1] for r in rows][::-1]
    p.prism(outline, y0, y1, rgb, mat, plane="XZ")


def scan_gate():
    """An inspection arch over the pad: an eyeball scanner hangs under the top, one red iris looking down
    at whoever walks through. Legs stand outside the 12 m pad; red glow strips run along the inside."""
    p = Part("scan_gate", "prop")
    # Centre line: legs at x = +-14 up to z = 3.5, then a half ellipse (14 x 5) to a top at z = 8.5.
    arc = [(14 * math.cos(math.pi * (1 - k / 16)), 3.5 + 5 * math.sin(math.pi * (1 - k / 16))) for k in range(17)]
    spine = [(-14.0, 0.0), (-14.0, 3.5)] + arc[1:-1] + [(14.0, 3.5), (14.0, 0.0)]
    stroke(p, spine, -0.5, 0.5, -1.0, 1.0, TRIM)
    # Glow strips on the inside of the arch (the hole side), kept just inside the surface.
    stroke(p, spine, -0.68, -0.48, -0.95, 0.95, GLOW_RED, "glow")
    # The eye: a trim ball on a bracket, a red glowing iris and a dark pupil facing the pad.
    p.cylinder((0, 0, 7.0), 0.15, 1.4, METAL, segments=8)
    p.sphere((0, 0, 5.9), 1.5, TRIM, segments=16)
    p.sphere((0, -1.35, 5.9), 0.8, GLOW_RED, "glow", squash=0.5, segments=12)
    p.sphere((0, -1.8, 5.9), 0.32, DARK, squash=0.5, segments=8)
    return p


def tower_small():
    """A little control tower with a lit cab and a dish that cups the sky like an ear.
    The silly idea: the dish is an ear, and it listens to the cab's fishbowl-glass windows.
    Over the 1000 triangle guide (1224): the four glow rings are the look and cost about 700 of them."""
    p = Part("tower_small", "prop")
    p.rounded_box((-1.6, -1.6, 0.0), (1.6, 1.6, 1.2), 0.4, METAL, segments=4)
    p.box((-0.8, -1.66, 0.45), (0.8, -1.6, 0.9), GLOW_PINK, "glow")         # door sign
    p.cylinder((0, 0, 1.2), 0.9, 9.2, TRIM, radius_top=0.55, segments=12)  # shaft, tapering
    for z in (3.2, 6.2, 9.2):
        r = 0.9 - 0.35 * (z - 1.2) / 9.2
        p.torus((0, 0, z), r + 0.02, 0.05, GLOW_PINK, "glow", segments=16, sides=5)
    p.cylinder((0, 0, 10.4), 1.55, 0.2, DARK, segments=16)                  # cab floor
    p.cylinder((0, 0, 10.6), 1.5, 1.8, GLASS, "glass", segments=12)         # cab
    p.torus((0, 0, 11.5), 1.53, 0.06, GLOW_CYAN, "glow", segments=20, sides=5)
    for x, y in ((0, -1.52), (0, 1.52)):
        p.box((x - 0.35, y - 0.04, 10.8), (x + 0.35, y + 0.04, 11.6), GLOW_CYAN, "glow")
    p.dome((0, 0, 12.4), 1.6, TRIM, squash=0.5, segments=12)                # cab roof, top at 12.8
    p.cylinder((0, 0, 12.7), 0.07, 0.7, METAL, segments=8)                  # dish mast
    # The dish, tilted towards the sky, built at its own origin then turned into place.
    with p.placed(Matrix.Translation((0, 0, 13.4)) @ Matrix.Rotation(math.radians(38), 4, "X")):
        p.cylinder((0, 0, 0), 0.12, 0.3, METAL, segments=8)
        p.cylinder((0, 0, 0.25), 0.15, 0.25, TRIM, radius_top=0.95, segments=16)
        p.cylinder((0, 0, 0.5), 0.95, 0.06, TRIM, segments=16)
        p.sphere((0, 0, 0.6), 0.1, GLOW_CYAN, "glow", segments=8)
    return p


def patrol_craft():
    """A wedge hover craft with tail fins and a light bar of two lamps that argue: red left, blue right,
    never in sync. The silly idea: the lights are two police lamps that disagree about everything.
    Origin at the hull's bottom centre; it hovers, so there is no wheel to see."""
    p = Part("patrol_craft", "vehicle")
    # Side profile (y, z), extruded across the 4.5 m width: nose low at -Y, flat deck at 1.4 m.
    p.prism([(-3.5, 0.0), (3.5, 0.0), (3.5, 1.4), (-1.0, 1.4), (-3.5, 0.5)], -2.25, 2.25, METAL, plane="YZ")
    # Stripe along the sides so the wedge reads as a hull, not a brick.
    p.prism([(-3.46, 0.3), (3.45, 0.3), (3.45, 0.5), (-3.46, 0.5)], -2.27, 2.27, RUST, plane="YZ")
    # Tail fins: two tall plates up at the back, two flat wings out to the sides.
    for x in (-1.7, 1.7):
        p.prism([(1.8, 1.4), (3.5, 1.4), (3.5, 2.6), (2.4, 2.1), (1.8, 1.6)], x - 0.1, x + 0.1, TRIM, plane="YZ")
    for s in (-1, 1):
        wing = [(2.2 * s, 1.0), (3.0 * s, 2.3), (3.0 * s, 3.4), (2.2 * s, 3.4)]
        p.prism(wing, 0.4, 0.6, TEAL, plane="XY")
    # Light bar on the deck, split in two lamps with separate materials and red/blue vertex colours.
    p.box((-1.2, -0.6, 1.35), (-0.05, 0.6, 1.6), RED, "light_red")
    p.box((0.05, -0.6, 1.35), (1.2, 0.6, 1.6), BLUE, "light_blue")
    return p


MODELS = {
    "scan_gate": (scan_gate, (-15.0, -2.3, 15.0, 2.3)),
    "tower_small": (tower_small, (-2.0, -2.0, 2.0, 2.0)),
    "patrol_craft": (patrol_craft, (-3.2, -3.6, 3.2, 3.6)),
}

kit.run(MODELS, "content/smuggle")
