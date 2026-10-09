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


def hover_van():
    """A boxy delivery van: a bubble cab up front, a tall cargo box behind, a light bar on top."""
    p = Part("hover_van", kind="vehicle")
    body, box_rgb = colour("#d9a441"), colour("#efe6d6")
    for x in (-0.8, 0.8):
        for y in (-1.8, 1.8):
            p.cylinder((x, y, 0.0), 0.35, 0.12, GLOW, "glow", segments=12)
    p.rounded_box((-1.15, -2.9, 0.12), (1.15, 2.9, 0.8), 0.6, body, segments=6)
    p.rounded_box((-1.1, -0.6, 0.8), (1.1, 2.8, 2.6), 0.3, box_rgb, segments=4)
    p.box((-1.12, 0.0, 1.4), (1.12, 2.4, 1.9), body)
    p.sphere((0.0, -1.5, 0.8), 0.95, GLASS, "glass", squash=0.75, segments=16)
    p.box((-0.6, -0.4, 2.6), (0.6, -0.1, 2.75), colour("#ff5fa2"), "glow")
    p.sphere((0.0, -2.9, 0.5), 0.18, GLOW, "glow", segments=8)
    return p


def hover_bus():
    """A long bus with a window band, a roof dome for the driver and fins at the back."""
    p = Part("hover_bus", kind="vehicle")
    body = colour("#3d8a9a")
    for y in (-3.5, 0.0, 3.5):
        for x in (-0.9, 0.9):
            p.cylinder((x, y, 0.0), 0.4, 0.12, GLOW, "glow", segments=12)
    p.rounded_box((-1.3, -4.6, 0.12), (1.3, 4.6, 2.6), 1.1, body, segments=8)
    p.rounded_box((-1.33, -4.63, 1.2), (1.33, 4.63, 2.1), 1.12, GLASS, "glass", segments=8)
    p.rounded_box((-1.36, -4.66, 0.6), (1.36, 4.66, 0.85), 1.15, TRIM, segments=8)
    p.dome((0.0, -3.0, 2.6), 0.9, GLASS, "glass", squash=0.6, segments=16)
    for x in (-1.0, 1.0):
        p.prism([(3.0, 2.4), (4.5, 2.4), (4.7, 3.6), (4.2, 3.6)], x - 0.07, x + 0.07, body, plane="YZ")
    p.sphere((0.0, -4.6, 1.0), 0.2, GLOW, "glow", segments=8)
    return p


MODELS = {
    "hover_car_red": (hover_car("hover_car_red", colour("#e2483d")), (-1.3, -2.6, 1.3, 2.6)),
    "hover_car_teal": (hover_car("hover_car_teal", colour("#2fb5a8")), (-1.3, -2.6, 1.3, 2.6)),
    "hover_van": (hover_van, (-1.3, -3.2, 1.3, 3.0)),
    "hover_bus": (hover_bus, (-1.5, -4.9, 1.5, 4.9)),
}

kit.run(MODELS, "content/city")
