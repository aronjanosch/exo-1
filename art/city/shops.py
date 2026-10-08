"""Shop buildings: a shop floor at street level, flats above, junk on the roof.

Names and fiction are placeholders until the initiator picks. Look and measures: BRIEF.md.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/shops.py -- --out content/city [--renders DIR] [--only id,id]
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import BAY, GROUND_STOREY, STOREY, Part, colour  # noqa: E402

TRIM = colour("#2b2838")
CONCRETE = colour("#8f8a99")
GLASS = colour("#1f3d4f")
LIT = colour("#ffd38a")
METAL = colour("#b9bcc4")


def floors(n_upper):
    """Floor heights from the ground: [0, 4.5, 8.0, ...]."""
    return [0.0] + [GROUND_STOREY + i * STOREY for i in range(n_upper + 1)]


def noodle_bar():
    """Two bays, three storeys plus a roof dome. A big round porthole on each flat, a fat awning."""
    p = Part("noodle_bar")
    body, accent, glow = colour("#e8836b"), colour("#f2c14e"), colour("#5ef2e0")
    w, d = 2 * BAY, 10.0
    zs = floors(2)
    top = zs[-1]
    # Body sits 1.7 m back from the lot's front line, so the awning stays on the lot.
    front = -d / 2 + 1.7
    p.box((-w / 2, front, 0), (w / 2, d / 2, top), body)
    # Plinth and floor bands: chunky stripes that read from far away.
    p.box((-w / 2 - 0.0, front - 0.15, 0), (w / 2, front, 0.5), TRIM)
    for z in zs[1:]:
        p.box((-w / 2, front - 0.25, z - 0.25), (w / 2, front, z + 0.15), TRIM)
    # Shop floor: wide window left, door right, sign over both, awning.
    p.window(-1.2, 0.7, 4.5, 2.6, front, METAL, LIT, lit=True)
    p.door("door_shop", 2.4, front, METAL, accent, use="shop")
    p.box((-w / 2, front - 1.6, 3.2), (w / 2, front, 3.45), accent)            # awning
    p.box((-w / 2, front - 1.65, 3.12), (w / 2, front - 1.55, 3.2), glow, "glow")  # glow strip under its lip
    p.sign(0, 3.6, 6.0, 0.8, front, TRIM, glow)
    # Flats: one porthole and one narrow window per floor, alternating sides.
    for i, z in enumerate(zs[1:-1]):
        side = -1 if i % 2 == 0 else 1
        p.cylinder((side * 1.6, front, z + 1.75), 1.0, 0.25, METAL, axis="Y", segments=16)
        p.cylinder((side * 1.6, front - 0.05, z + 1.75), 0.8, 0.25, GLASS, "glass", axis="Y", segments=16)
        p.window(-side * 2.0, z + 0.8, 1.0, 2.0, front, METAL, LIT if i == 1 else GLASS, lit=(i == 1))
    # Pipe down the side, bent over the cornice.
    p.cylinder((w / 2 - 0.4, front - 0.25, 0.5), 0.12, top - 0.3, METAL, segments=8)
    # Roof: parapet, a squat dome, a dish on a stick, an antenna with a glowing tip.
    p.box((-w / 2, front, top), (w / 2, d / 2, top + 0.6), TRIM)
    p.dome((0.0, 1.0, top + 0.6), 2.4, accent, squash=0.7, segments=16)
    p.cylinder((-2.8, 3.5, top + 0.6), 0.1, 1.6, METAL, segments=6)
    p.cylinder((-2.8, 3.5, top + 2.2), 0.9, 0.3, METAL, radius_top=0.3, segments=12)
    p.cylinder((3.0, 3.8, top + 0.6), 0.06, 4.0, METAL, segments=6)
    p.cylinder((3.0, 3.8, top + 4.6), 0.18, 0.3, glow, "glow", segments=8)
    return p


def tower_shop():
    """Narrow and tall: one bay, a kiosk at the bottom, five floors that step out as they rise."""
    p = Part("tower_shop")
    body, accent, glow = colour("#7aa6c2"), colour("#c9a0dc"), colour("#ff5fa2")
    w, d = BAY, 8.0
    zs = floors(5)
    front = -d / 2 + 0.8
    p.box((-w / 2, front, 0), (w / 2, d / 2, GROUND_STOREY), body)
    p.window(0.0, 0.9, 1.8, 2.2, front, TRIM, LIT, lit=True)                 # kiosk hatch
    p.box((-1.2, front - 0.6, 0.9), (1.2, front, 1.05), METAL)               # counter
    p.anchor("door_shop", (0.0, front - 1.0, 0.0), kind="door", use="shop")
    p.sign(0.0, 3.4, 3.4, 0.8, front, TRIM, glow)
    # Upper floors step forward 0.2 m each, up to the lot line.
    for i, z in enumerate(zs[1:-1]):
        f = max(front - 0.2 * (i + 1), -d / 2 + 0.15)
        h = STOREY
        p.box((-w / 2, f, z), (w / 2, d / 2, z + h), body if i % 2 == 0 else accent)
        p.box((-w / 2, f - 0.1, z), (w / 2, f, z + 0.3), TRIM)
        p.window(0.0, z + 0.8, 2.2, 1.8, f, TRIM, LIT if i in (1, 3) else GLASS, lit=i in (1, 3))
    top = zs[-1]
    p.box((-w / 2, -d / 2, top), (w / 2, d / 2, top + 0.4), TRIM)
    # A water tank on legs and a beacon.
    for x in (-1.0, 1.0):
        for y in (0.5, 2.5):
            p.cylinder((x, y, top + 0.4), 0.08, 1.2, METAL, segments=6)
    p.cylinder((0.0, 1.5, top + 1.6), 1.4, 1.6, METAL, segments=12)
    p.dome((0.0, 1.5, top + 3.2), 1.4, accent, squash=0.5, segments=12)
    p.cylinder((0.0, 1.5, top + 3.85), 0.15, 0.3, glow, "glow", segments=8)
    return p


MODELS = {
    "noodle_bar": (noodle_bar, (-4.0, -5.0, 4.0, 5.0)),
    "tower_shop": (tower_shop, (-2.0, -4.0, 2.0, 4.0)),
}

kit.run(MODELS, "content/city")
