"""Hover cars. The model sits on the ground at its origin; the game lifts it to hover height.
Front faces -Y like the buildings' street side.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/vehicles.py -- --out content/city [--renders DIR] [--only id,id]
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import Part, colour  # noqa: E402

TRIM = colour("#f4f1e8")
GLASS = colour("#2a4a6b")
GLOW = colour("#5ef2e0")


def hover_car(name, body_rgb):
    """A rounded tub with a bubble canopy, two tail fins and glowing hover pads underneath."""
    def make():
        p = Part(name, kind="vehicle")
        p.cylinder((-0.7, 1.4, 0.0), 0.35, 0.12, GLOW, "glow", segments=12)
        p.cylinder((0.7, 1.4, 0.0), 0.35, 0.12, GLOW, "glow", segments=12)
        p.cylinder((-0.7, -1.4, 0.0), 0.35, 0.12, GLOW, "glow", segments=12)
        p.cylinder((0.7, -1.4, 0.0), 0.35, 0.12, GLOW, "glow", segments=12)
        p.rounded_box((-1.05, -2.3, 0.12), (1.05, 2.3, 0.75), 0.9, body_rgb, segments=8)
        p.rounded_box((-1.12, -2.37, 0.45), (1.12, 2.37, 0.6), 0.95, TRIM, segments=8)   # bumper band
        p.sphere((0.0, -0.2, 0.75), 0.95, GLASS, "glass", squash=0.65, segments=16)
        for x in (-0.8, 0.8):
            p.prism([(0.9, 0.75), (2.3, 0.75), (2.5, 1.7), (2.1, 1.7)], x - 0.06, x + 0.06, body_rgb, plane="YZ")
        p.sphere((0.0, -2.3, 0.45), 0.18, GLOW, "glow", segments=8)                    # headlight
        return p
    return make


MODELS = {
    "hover_car_red": (hover_car("hover_car_red", colour("#e2483d")), (-1.3, -2.6, 1.3, 2.6)),
    "hover_car_teal": (hover_car("hover_car_teal", colour("#2fb5a8")), (-1.3, -2.6, 1.3, 2.6)),
}

kit.run(MODELS, "content/city")
