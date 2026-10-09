"""Suburb homes: single-storey retro-future bungalows with yards, the low counterpart to the rows.

Butterfly roofs, a dome house, a flying-wedge roof, glass walls to the street, carports with a
hover pad. Each home faces -Y with its front door; the yard is part of the lot.
Names are placeholders. Look and measures: BRIEF.md.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/homes.py -- --out content/city [--renders DIR] [--only id,id]
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import Part, colour  # noqa: E402

TRIM = colour("#f4f1e8")
METAL = colour("#9aa7b8")
DARK = colour("#3b3f4a")
GLASS = colour("#2a4a6b")
LIT = colour("#ffe2a0")
STONE = colour("#a89a8c")
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")

WALL_H = 3.2


def glass_wall(p, x0, x1, z0, z1, y, frame, lit_every=2):
    """A row of tall panes between thin mullions on a wall face at y (facing -Y)."""
    n = max(1, round((x1 - x0) / 1.6))
    for k in range(n):
        a = x0 + (x1 - x0) * k / n
        b = x0 + (x1 - x0) * (k + 1) / n
        on = k % lit_every == 0
        p.box((a + 0.06, y - 0.05, z0), (b - 0.06, y, z1), LIT if on else GLASS, "glow" if on else "glass")
        p.box((a - 0.06, y - 0.12, z0), (a + 0.06, y, z1), frame)
    p.box((x1 - 0.06, y - 0.12, z0), (x1 + 0.06, y, z1), frame)


def carport(p, x0, x1, y0, y1, accent, frame):
    """A thin roof on two boomerang legs, a pad underneath for the hover car."""
    for x in (x0 + 0.3, x1 - 0.5):
        leg = [(0.0, 0.0), (0.25, 0.0), (0.7, 1.6), (0.25, WALL_H - 0.3), (0.0, WALL_H - 0.3), (0.45, 1.6)]
        p.prism([(y0 + 0.6 + u, v) for u, v in leg], x, x + 0.2, accent, plane="YZ")
    p.box((x0, y0, WALL_H - 0.3), (x1, y1, WALL_H - 0.1), frame)
    p.cylinder(((x0 + x1) / 2, (y0 + y1) / 2, 0), 1.9, 0.06, TRIM, segments=24)
    p.torus(((x0 + x1) / 2, (y0 + y1) / 2, 0.06), 1.8, 0.04, GLOW_CYAN, "glow", segments=24, sides=4)
    p.anchor("pad", ((x0 + x1) / 2, (y0 + y1) / 2, 0.06), kind="pad", size="car")


def bungalow_butterfly():
    """A long low house under a butterfly roof, a glass wall to the street, a stone chimney wall."""
    p = Part("bungalow_butterfly")
    body, frame, accent = colour("#d8c7a8"), colour("#f2ead8"), colour("#2f7f86")
    w, d = 12.0, 8.0
    x0, x1 = -w / 2, w / 2
    p.box((x0, 0, 0), (x1, d, WALL_H), body)
    p.box((x0 - 0.2, -0.2, -0.0), (x1 + 0.2, d + 0.2, 0.3), STONE)       # plinth
    glass_wall(p, x0 + 3.0, x1 - 0.5, 0.3, WALL_H - 0.2, 0, frame)
    p.door("door_home", x0 + 1.6, 0, frame, accent, w=1.3, h=2.4, use="home")
    # Stone chimney wall cutting through the roof.
    p.box((x0 + 2.6, -0.5, 0), (x0 + 3.2, d + 0.3, WALL_H + 1.6), STONE)
    roof = [(x0 - 0.6, WALL_H), (x1 + 0.6, WALL_H), (x1 + 0.6, WALL_H + 1.3), (0.5, WALL_H + 0.35),
            (x0 - 0.6, WALL_H + 1.1)]
    p.prism(roof, -1.0, d + 0.6, frame, plane="XZ")
    p.box((x0 - 0.6, -1.05, WALL_H + 0.1), (x1 + 0.6, -1.0, WALL_H + 0.25), accent)
    carport(p, x1 + 0.8, x1 + 5.2, -0.5, 5.0, accent, frame)
    return p


def bungalow_dome():
    """A dome house: a white dome on a ring of round windows, an entrance tunnel, an antenna."""
    p = Part("bungalow_dome")
    body, frame, accent = colour("#e9e4dc"), colour("#f4f1e8"), colour("#e07b39")
    r = 5.0
    cy = r
    p.cylinder((0, cy, 0), r + 0.3, 0.3, STONE, segments=32)
    p.cylinder((0, cy, 0.3), r, 1.6, accent, segments=32)
    for k in range(8):
        a = math.radians(225 + k * 45)
        x, y = (r + 0.02) * math.cos(a), cy + (r + 0.02) * math.sin(a)
        on = k % 3 == 0
        p.sphere((x, y, 1.1), 0.45, LIT if on else GLASS, "glow" if on else "glass", squash=0.8, segments=12)
    p.dome((0, cy, 1.9), r, body, squash=0.75, segments=32)
    p.torus((0, cy, 1.9), r + 0.05, 0.08, GLOW_PINK, "glow", segments=40, sides=6)
    # Entrance tunnel to the front.
    p.rounded_box((-1.2, -0.6, 0.3), (1.2, 1.5, 2.7), 0.6, body, segments=4)
    p.door("door_home", 0.0, -0.6, frame, accent, w=1.2, h=2.2, use="home")
    p.cylinder((0, cy, 1.9 + r * 0.75), 0.6, 0.3, frame, segments=16)
    p.cylinder((0, cy, 2.2 + r * 0.75), 0.06, 2.0, METAL, segments=8)
    p.sphere((0, cy, 4.3 + r * 0.75), 0.2, GLOW_CYAN, "glow", segments=10)
    carport(p, r + 0.6, r + 5.0, 1.0, 6.5, accent, frame)
    return p


def bungalow_wedge():
    """A flying-wedge roof that rises to the street, clerestory windows under it, a stone base."""
    p = Part("bungalow_wedge")
    body, frame, accent = colour("#a9bfc4"), colour("#eef2f3"), colour("#c4473a")
    w, d = 10.0, 9.0
    x0, x1 = -w / 2, w / 2
    p.box((x0, 0, 0), (x1, d, WALL_H), body)
    p.box((x0, -0.1, 0), (x0 + 3.0, d, 1.2), STONE)
    glass_wall(p, x0 + 3.4, x1 - 0.4, 0.4, 2.6, 0, frame, lit_every=3)
    p.door("door_home", x0 + 1.5, 0, frame, accent, w=1.3, h=2.4, use="home")
    # Clerestory band and the wedge roof: high at the street, low at the back.
    glass_wall(p, x0 + 0.4, x1 - 0.4, WALL_H, WALL_H + 0.9, 0, frame, lit_every=4)
    wedge = [(-1.4, WALL_H + 1.0), (d + 0.6, WALL_H - 0.1), (d + 0.6, WALL_H + 0.15), (-1.4, WALL_H + 1.35)]
    p.prism(wedge, x0 - 0.4, x1 + 0.4, frame, plane="YZ")
    back = [(0.0, WALL_H), (d, WALL_H), (0.0, WALL_H + 1.0)]
    p.prism(back, x0, x1, body, plane="YZ")
    p.box((x0 - 0.4, -1.45, WALL_H + 1.0), (x1 + 0.4, -1.4, WALL_H + 1.3), accent)
    carport(p, x1 + 0.8, x1 + 5.2, -0.5, 5.0, accent, frame)
    return p


MODELS = {
    "bungalow_butterfly": (bungalow_butterfly, (-6.8, -1.1, 11.4, 8.7)),
    "bungalow_dome": (bungalow_dome, (-5.6, -0.8, 10.2, 10.6)),
    "bungalow_wedge": (bungalow_wedge, (-5.5, -1.5, 10.4, 9.7)),
}

kit.run(MODELS, "content/city")
