"""Landmarks: big one-off buildings that anchor a quarter and show over the horizon.

On a 5 km planet the horizon is about 130 m away at eye height; a 30 m tower shows over it to
about 680 m. Names and fiction are placeholders until the initiator picks. Look: BRIEF.md.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/landmarks.py -- --out content/city [--renders DIR] [--only id,id]
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
LIT = colour("#ffe2a0")
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")


def turned(angle_deg, at=(0, 0, 0)):
    return Matrix.Translation(at) @ Matrix.Rotation(math.radians(angle_deg), 4, "Z")


def company_hq():
    """A company's head office: a podium with a gate big enough for anything that walks or rolls,
    a round logo over it, and a stepped tower on top with ribbon windows and a mast."""
    p = Part("company_hq")
    body, frame, accent = colour("#8a93a3"), colour("#e9ebef"), colour("#c4473a")
    w, d, podium = 20.0, 16.0, 10.0
    p.box((-w / 2, 0, 0), (w / 2, d, podium), body)
    # Gate portal: a deep frame around the gate, the gate itself with glowing bands.
    p.box((-5.2, -1.0, 0), (-4.2, 0, 11.0), accent)
    p.box((4.2, -1.0, 0), (5.2, 0, 11.0), accent)
    p.box((-5.2, -1.0, 10.0), (5.2, 0, 11.0), accent)
    p.door("door_gate", 0.0, 0, frame, DARK, w=8.0, h=9.5, use="company")
    for z in (2.0, 4.5, 7.0):
        p.box((-3.8, -0.22, z), (3.8, -0.17, z + 0.15), GLOW_CYAN, "glow")
    # Round logo over the gate: a frame disc, a glowing ring, a dark middle.
    p.cylinder((0, 0, 13.4), 2.6, 1.3, frame, segments=32, axis="Y")
    p.cylinder((0, -1.15, 13.4), 2.2, 0.3, GLOW_PINK, "glow", segments=32, axis="Y")
    p.cylinder((0, -1.3, 13.4), 1.3, 0.3, DARK, segments=32, axis="Y")
    # Podium windows on both sides of the gate, two tall storeys, and a staff door.
    for x in (-8.5, -6.5, 6.5, 8.5):
        for z in (1.0, 5.5):
            on = (x > 0) == (z > 2)
            p.box((x - 0.75, -0.15, z), (x + 0.75, 0, z + 3.4), frame)
            p.box((x - 0.6, -0.2, z + 0.15), (x + 0.6, -0.15, z + 3.25), LIT if on else GLASS,
                  "glow" if on else "glass")
    p.door("door_staff", 8.6, 0, frame, accent, use="staff")
    p.box((-w / 2, -0.5, podium), (w / 2, d, podium + 0.5), frame)
    # Googie fins at the podium corners.
    fin = [(0.0, 0.0), (1.4, 0.0), (0.8, 20.0), (0.2, 19.0)]
    for x in (-w / 2 - 0.2, w / 2 - 0.2):
        p.prism([(-0.5 + u, v) for u, v in fin], x, x + 0.4, accent, plane="YZ")
    # Stepped tower: three tiers, each with a ribbon window and a white slab.
    tiers = [(8.0, 2.0, 8.0), (6.0, 3.0, 8.0), (4.0, 4.0, 7.0)]
    z = podium + 0.5
    for half, y0, h in tiers:
        p.box((-half, y0, z), (half, d - y0, z + h), body)
        for k in range(int(h // 3.5)):
            zz = z + 1.0 + k * 3.5
            on = k % 2 == 0
            p.box((-half - 0.05, y0 - 0.05, zz), (half + 0.05, d - y0 + 0.05, zz + 1.6),
                  LIT if on else GLASS, "glow" if on else "glass")
        z += h
        p.box((-half - 0.3, y0 - 0.3, z), (half + 0.3, d - y0 + 0.3, z + 0.4), frame)
        z += 0.4
    p.sign(0, z - 4.2, 6.0, 1.4, 4.0, DARK, GLOW_PINK)
    p.cylinder((0, d / 2, z), 0.25, 7.0, METAL, radius_top=0.08, segments=10)
    p.torus((0, d / 2, z + 4.0), 0.8, 0.07, GLOW_CYAN, "glow", segments=24, sides=6)
    p.sphere((0, d / 2, z + 7.3), 0.45, GLOW_PINK, "glow", segments=12)
    return p


def beacon_spire():
    """The plaza's needle: three swept fins hold a shaft, an observation saucer at 30 m,
    a needle with a light on top. Seen from everywhere in the quarter and over the horizon."""
    p = Part("beacon_spire")
    accent, frame = colour("#2f7f86"), colour("#efe6d6")
    p.cylinder((0, 0, 0), 5.0, 0.6, frame, segments=40)
    p.torus((0, 0, 0.6), 4.9, 0.08, GLOW_CYAN, "glow", segments=48, sides=6)
    fin = [(0.9, 0.6), (4.6, 0.6), (1.4, 16.0), (0.9, 16.0)]
    for k in range(3):
        with p.placed(turned(90 + 120 * k)):
            p.prism(fin, -0.25, 0.25, accent, plane="XZ")
            p.box((4.0, -0.3, 0.6), (4.7, 0.3, 1.2), frame)
    p.cylinder((0, 0, 0.6), 1.3, 29.0, frame, radius_top=0.9, segments=20)
    # Lift: a glass strip up the front and a door at the foot.
    p.box((-0.4, -1.32, 0.6), (0.4, -1.0, 28.0), GLASS, "glass")
    p.box((-0.3, -1.36, 8.0), (0.3, -1.3, 10.0), LIT, "glow")
    p.door("door_lift", 0.0, -1.3, frame, accent, w=1.4, h=2.4, use="lift")
    # Observation saucer.
    z = 29.0
    p.cylinder((0, 0, z), 1.0, 1.5, accent, radius_top=5.0, segments=40)
    p.cylinder((0, 0, z + 1.5), 5.2, 0.35, frame, segments=40)
    p.cylinder((0, 0, z + 1.85), 4.8, 1.6, LIT, "glow", segments=40)
    p.cylinder((0, 0, z + 3.45), 5.0, 1.2, accent, radius_top=1.4, segments=40)
    p.torus((0, 0, z + 1.65), 5.25, 0.1, GLOW_PINK, "glow", segments=48, sides=6)
    p.cylinder((0, 0, z + 4.65), 0.25, 11.0, METAL, radius_top=0.05, segments=10)
    for zz, r in ((z + 7.0, 0.7), (z + 10.0, 0.45)):
        p.torus((0, 0, zz), r, 0.06, GLOW_CYAN, "glow", segments=20, sides=6)
    p.sphere((0, 0, z + 15.9), 0.5, GLOW_PINK, "glow", segments=14)
    return p


MODELS = {
    "company_hq": (company_hq, (-10.5, -2.0, 10.5, 16.0)),
    "beacon_spire": (beacon_spire, (-5.5, -5.5, 5.5, 5.5)),
}

kit.run(MODELS, "content/city")
