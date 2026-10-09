"""Suburb homes: single-storey retro-future bungalows with yards, the low counterpart to the rows.

Butterfly roofs, a dome house, a flying-wedge roof, glass walls to the street, carports with a
hover pad. Each home faces -Y with its front door; the yard is part of the lot. The whole house is
one room (#94): see-through glass to the street, a door that opens, furniture and an `npc` anchor
for whoever lives there. Roofs are too thin to hide a door leaf, so leaves slide sideways into the
front wall.
Names are placeholders. Look and measures: BRIEF.md.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/homes.py -- --out content/city [--renders DIR] [--only id,id]
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
STONE = colour("#a89a8c")
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")

WALL_H = 3.2
WALL = 0.2
CLEAR = colour("#cfeef5")
FLOOR = colour("#b98f5a")   # warm wood
LINING = colour("#efe6d6")  # inside walls


def glass_wall(p, x0, x1, z0, z1, y, frame, lit_every=2, clear=False):
    """A row of tall panes between thin mullions on a wall face at y (facing -Y). clear: see-through
    panes over a hole in the wall, into the room."""
    n = max(1, round((x1 - x0) / 1.6))
    for k in range(n):
        a = x0 + (x1 - x0) * k / n
        b = x0 + (x1 - x0) * (k + 1) / n
        on = k % lit_every == 0
        if clear:
            p.box((a + 0.06, y - 0.05, z0), (b - 0.06, y + 0.02, z1), CLEAR, "clear")
        else:
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


def walls(p, x0, x1, d, z0, z1, rgb, holes):
    """The house as a room: a front wall with holes, side and back walls, no overlaps at corners,
    and a warm lining inside."""
    p.wall((x0, 0, z0), (x1, WALL, z1), rgb, holes)
    p.box((x0, d - WALL, z0), (x1, d, z1), rgb)
    p.box((x0, WALL, z0), (x0 + WALL, d - WALL, z1), rgb)
    p.box((x1 - WALL, WALL, z0), (x1, d - WALL, z1), rgb)
    i0, i1, t = x0 + WALL, x1 - WALL, 0.02
    p.wall((i0, WALL, z0), (i1, WALL + t, z1), LINING, holes)
    p.box((i0, d - WALL - t, z0), (i1, d - WALL, z1), LINING)
    p.box((i0, WALL + t, z0), (i0 + t, d - WALL - t, z1), LINING)
    p.box((i1 - t, WALL + t, z0), (i1, d - WALL - t, z1), LINING)


def sofa(p, x, y, w, rgb, turn=0, z=0.0):
    """A low sofa with a curved back; built facing -Y, turned by `turn` degrees about its centre."""
    with p.placed(Matrix.Translation((x, y, z)) @ Matrix.Rotation(math.radians(turn), 4, "Z")):
        p.rounded_box((-w / 2, -0.45, 0), (w / 2, 0.45, 0.42), 0.2, rgb, segments=4)
        p.rounded_box((-w / 2, 0.2, 0.42), (w / 2, 0.45, 0.85), 0.12, rgb, segments=4)
        for sx in (-1, 1):
            p.cylinder((sx * (w / 2 - 0.15), -0.3, 0), 0.03, 0.12, METAL, segments=6)


def kidney_table(p, x, y, z=0.0):
    p.cylinder((x, y, z), 0.05, 0.38, METAL, segments=8)
    p.rounded_box((x - 0.6, y - 0.3, z + 0.38), (x + 0.6, y + 0.3, z + 0.43), 0.28, TRIM, segments=5)
    p.sphere((x + 0.25, y, z + 0.5), 0.07, GLOW_PINK, "glow", segments=8)


def sputnik_lamp(p, x, y, z=0.0):
    """A floor lamp: a ball with glowing tips on rods, on a thin stand."""
    p.cylinder((x, y, z), 0.25, 0.04, DARK, segments=12)
    p.cylinder((x, y, z + 0.04), 0.025, 1.4, METAL, segments=6)
    p.sphere((x, y, z + 1.55), 0.12, METAL, segments=10)
    for k in range(6):
        a, b = math.radians(60 * k), math.radians(35 if k % 2 else -25)
        dx, dy, dz = math.cos(a) * math.cos(b), math.sin(a) * math.cos(b), math.sin(b)
        p.sphere((x + 0.35 * dx, y + 0.35 * dy, z + 1.55 + 0.35 * dz), 0.06, LIT, "glow", segments=8)
        p.box((x + 0.1 * dx - 0.01, y + 0.1 * dy - 0.01, z + 1.55 + 0.1 * dz - 0.01),
              (x + 0.33 * dx + 0.01, y + 0.33 * dy + 0.01, z + 1.55 + 0.33 * dz + 0.01), METAL)


def screen(p, x, y, rgb, z=0.0):
    """A boxy TV on splayed legs, its screen facing -Y."""
    for sx in (-1, 1):
        p.box((x + sx * 0.3 - 0.02, y - 0.02, z), (x + sx * 0.3 + 0.02, y + 0.02, z + 0.4), METAL)
    p.rounded_box((x - 0.45, y - 0.3, z + 0.4), (x + 0.45, y + 0.3, z + 1.1), 0.15, rgb, segments=4)
    p.rounded_box((x - 0.33, y - 0.33, z + 0.52), (x + 0.33, y - 0.29, z + 0.98), 0.1, GLOW_CYAN, "glow", segments=3)


def bungalow_butterfly():
    """A long low house under a butterfly roof, a glass wall to the street, a stone chimney wall."""
    p = Part("bungalow_butterfly")
    body, frame, accent = colour("#d8c7a8"), colour("#f2ead8"), colour("#2f7f86")
    w, d = 12.0, 8.0
    x0, x1 = -w / 2, w / 2
    p.box((x0 - 0.2, -0.2, -0.0), (x1 + 0.2, d + 0.2, 0.3), STONE)       # plinth, the floor
    p.box((x0 + WALL, WALL, 0.3), (x1 - WALL, d - WALL, 0.32), FLOOR)
    # The door slides right into the front wall and on into the chimney wall.
    door = p.doorway("door_home", x0 + 1.5, 0, frame, accent, GLOW_CYAN, w=1.2, h=2.4, wall=WALL,
                     slide=(1.2, 0.0, 0.0), z0=0.3, use="home")
    walls(p, x0, x1, d, 0.3, WALL_H, body, [door, (x0 + 3.4, x1 - 0.5, 0.3, WALL_H - 0.2)])
    glass_wall(p, x0 + 3.4, x1 - 0.5, 0.3, WALL_H - 0.2, 0, frame, clear=True)
    # Stone chimney wall cutting through the roof: open behind the front wall, so the hall by the
    # door and the living room are one room; a hearth on the room side.
    top = WALL_H + 1.6
    p.box((x0 + 2.6, -0.5, 0), (x0 + 3.2, WALL, top), STONE)
    p.box((x0 + 2.6, 2.6, 0), (x0 + 3.2, d + 0.3, top), STONE)
    p.box((x0 + 2.6, WALL, 2.5), (x0 + 3.2, 2.6, WALL_H), STONE)
    p.box((x0 + 3.2, 4.0, 0.32), (x0 + 3.5, 5.4, 0.9), DARK)
    p.box((x0 + 3.5, 4.15, 0.45), (x0 + 3.54, 5.25, 0.85), GLOW_PINK, "glow")
    # Living room: a sofa facing the glass, a kidney table, a lamp, a screen, the resident.
    sofa(p, 1.5, 5.8, 3.0, accent, turn=0, z=0.32)
    kidney_table(p, 1.5, 4.4, z=0.32)
    sputnik_lamp(p, -1.2, 6.6, z=0.32)
    screen(p, 4.6, 6.8, frame, z=0.32)
    p.anchor("npc", (3.6, 4.6, 0.32), kind="npc", role="resident")
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
    p.cylinder((0, cy, 0.3), r - 0.25, 0.02, FLOOR, segments=32)
    tunnel = 1.6
    gap = math.degrees(math.asin((tunnel - 0.1) / r))
    p.ring_wall((0, cy), r - 0.25, r, 0.3, 1.9, accent, segments=32, gaps=[(-90 - gap, -90 + gap)])
    for k in range(8):
        a = math.radians(225 + k * 45)
        x, y = (r + 0.02) * math.cos(a), cy + (r + 0.02) * math.sin(a)
        on = k % 3 == 0
        if k == 1:
            continue   # 270 degrees: the tunnel sits there
        p.sphere((x, y, 1.1), 0.45, LIT if on else GLASS, "glow" if on else "glass", squash=0.8, segments=12)
    p.dome((0, cy, 1.9), r, body, squash=0.75, segments=32)
    p.torus((0, cy, 1.9), r + 0.05, 0.08, GLOW_PINK, "glow", segments=40, sides=6)
    # Entrance tunnel to the front: hollow, the door slides sideways into its front wall.
    t0, t1 = -0.6, cy - math.sqrt(r * r - tunnel * tunnel) + 0.4
    p.box((-tunnel, t0, 0), (tunnel, t1, 0.3), STONE)
    hole = p.doorway("door_home", 0.0, t0, frame, accent, GLOW_PINK, w=1.0, h=2.1, wall=WALL,
                     slide=(1.0, 0.0, 0.0), z0=0.3, use="home")
    p.wall((-tunnel, t0, 0.3), (tunnel, t0 + WALL, 2.7), body, [hole])
    for sx in (-1, 1):
        a, b = sorted((sx * tunnel, sx * (tunnel - WALL)))
        p.box((a, t0 + WALL, 0.3), (b, t1, 2.7), body)
    p.rounded_box((-tunnel, t0, 2.7), (tunnel, t1, 3.0), 0.3, body, segments=4)
    # Inside: a round sofa along the wall, a round table, a lamp hanging from the top of the dome.
    p.ring_wall((0, cy), 2.7, 3.5, 0.32, 0.75, colour("#3d5a80"), segments=32, gaps=[(-125, -55)])
    p.ring_wall((0, cy), 3.3, 3.55, 0.75, 1.25, colour("#3d5a80"), segments=32, gaps=[(-125, -55)])
    p.cylinder((0, cy + 0.3, 0.32), 0.08, 0.4, METAL, segments=8)
    p.cylinder((0, cy + 0.3, 0.72), 0.9, 0.06, TRIM, segments=24)
    p.sphere((0.3, cy + 0.4, 0.86), 0.08, GLOW_CYAN, "glow", segments=8)
    apex = 1.9 + r * 0.75
    p.cylinder((0, cy, 3.6), 0.02, apex - 3.6, METAL, segments=6)
    p.sphere((0, cy, 3.5), 0.25, METAL, segments=12)
    p.torus((0, cy, 3.5), 0.55, 0.05, LIT, "glow", segments=24, sides=6)
    for k in range(6):
        a = math.radians(60 * k)
        p.box((0.2 * math.cos(a) - 0.015, cy + 0.2 * math.sin(a) - 0.015, 3.48),
              (0.55 * math.cos(a) + 0.015, cy + 0.55 * math.sin(a) + 0.015, 3.52), METAL)
    p.anchor("npc", (-1.5, cy + 1.2, 0.32), kind="npc", role="resident")
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
    door = p.doorway("door_home", x0 + 1.5, 0, frame, accent, GLOW_CYAN, w=1.3, h=2.4, wall=WALL,
                     slide=(1.3, 0.0, 0.0), use="home")
    walls(p, x0, x1, d, 0, WALL_H, body, [door, (x0 + 3.6, x1 - 0.4, 0.4, 2.6)])
    p.box((x0 + WALL, WALL, 0), (x1 - WALL, d - WALL, 0.02), FLOOR)
    # Stone cladding either side of the door.
    p.box((x0, -0.1, 0), (x0 + 0.7, 0, 1.2), STONE)
    p.box((x0 + 2.3, -0.1, 0), (x0 + 3.4, 0, 1.2), STONE)
    glass_wall(p, x0 + 3.6, x1 - 0.4, 0.4, 2.6, 0, frame, lit_every=3, clear=True)
    # Inside: a sofa facing the street, a table, a screen and a lamp, a kitchen bar at the back.
    sofa(p, 1.8, 5.2, 3.2, accent)
    kidney_table(p, 1.8, 3.8)
    screen(p, -2.3, 3.0, frame)
    sputnik_lamp(p, 4.1, 6.0)
    p.box((x0 + WALL, d - WALL - 0.7, 0.02), (x0 + 4.5, d - WALL, 0.95), TRIM)
    p.box((x0 + WALL, d - WALL - 0.75, 0.95), (x0 + 4.6, d - WALL, 1.0), METAL)
    p.cylinder((x0 + 1.5, d - WALL - 0.35, 1.0), 0.15, 0.3, GLOW_PINK, "glow", segments=10)
    p.anchor("npc", (x0 + 2.4, d - 1.6, 0.02), kind="npc", role="resident")
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
