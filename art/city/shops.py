"""Shop buildings in the retro-future style: round, tapered, glowing, a bit silly.

Names and fiction are placeholders until the initiator picks. Look and measures: BRIEF.md.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/shops.py -- --out content/city [--renders DIR] [--only id,id]
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import GROUND_STOREY, STOREY, Part, colour  # noqa: E402

TRIM = colour("#f4f1e8")     # warm white: slabs, rings, frames (the city's shared trim)
METAL = colour("#9aa7b8")
GLASS = colour("#2a4a6b")
LIT = colour("#ffe2a0")
CLEAR = colour("#cfeef5")   # see-through glass into a room
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")


def saucer_diner():
    """A flying saucer parked on a stick: glass kiosk below, diner in the disc, a spire on top,
    and a boomerang fin sign beside it. The kiosk is a real room: a round order counter with the
    cook in the middle, stools around it. The disc upstairs is instanced (#94)."""
    p = Part("saucer_diner")
    body, accent = colour("#ff8c5a"), colour("#2fb5a8")
    # Kiosk: a round glass room on a low white plinth; the plinth is its floor.
    floor, roof, r = 0.12, 3.2, 3.2
    p.cylinder((0, 0, 0), 3.6, floor, TRIM, segments=32)
    p.cylinder((0, 0, floor), r - 0.1, 0.02, colour("#d9d3e6"), segments=32)
    gap = math.degrees(math.asin(0.95 / r))
    p.ring_wall((0, 0), r - 0.1, r, floor, roof, CLEAR, "clear", segments=32, gaps=[(-90 - gap, -90 + gap)])
    for k in range(8):
        a = math.radians(22.5 + 45 * k)
        if abs(math.degrees(a) - 270) > 30:
            p.cylinder((r * math.cos(a), r * math.sin(a), floor), 0.07, roof - floor, TRIM, segments=6)
    p.cylinder((0, 0, roof), 3.5, 0.35, TRIM, segments=32)
    p.torus((0, 0, roof - 0.08), 2.2, 0.06, LIT, "glow", segments=32, sides=4)
    # A sliding glass door: no wall above to hide a leaf in, so it slides sideways along the glass.
    p.doorway("door_shop", 0.0, -r - 0.05, TRIM, accent, GLOW_PINK, w=1.5, h=2.4, wall=0.2,
              slide=(1.6, 0.0, 0.0), use="shop")
    # Order counter: a ring around the cook, open at the back so the cook gets in.
    p.ring_wall((0, 0), 1.05, 1.5, floor, floor + 1.0, accent, segments=24, gaps=[(60, 120)])
    p.ring_wall((0, 0), 0.98, 1.6, floor + 1.0, floor + 1.06, TRIM, segments=24, gaps=[(64, 116)])
    for k, a in enumerate((-150, -120, -60, -30, 0, 180)):
        x, y = 2.15 * math.cos(math.radians(a)), 2.15 * math.sin(math.radians(a))
        p.cylinder((x, y, floor), 0.05, 0.65, METAL, segments=8)
        p.cylinder((x, y, floor + 0.65), 0.22, 0.1, GLOW_PINK if k % 2 else accent, segments=12)
    # A shake machine on the counter, glowing tanks.
    for dx in (-0.25, 0.0, 0.25):
        p.cylinder((1.25 * math.cos(math.radians(30)) + dx * 0.3, 1.25 * math.sin(math.radians(30)) + dx,
                    floor + 1.06), 0.1, 0.45, GLOW_CYAN if dx else LIT, "glow", segments=8)
    p.anchor("npc", (0, 0, floor), kind="npc", role="cook")
    # Stem and saucer.
    p.cylinder((0, 0, 3.55), 1.1, 4.0, accent, segments=16)
    p.cylinder((0, 0, 7.0), 1.6, 1.0, body, radius_top=5.6, segments=32)       # underside
    p.cylinder((0, 0, 8.0), 5.6, 0.4, TRIM, segments=32)
    p.cylinder((0, 0, 8.4), 5.3, 1.3, LIT, "glow", segments=32)              # window ribbon, all lit
    p.cylinder((0, 0, 9.7), 5.6, 0.9, body, radius_top=2.6, segments=32)     # top cone
    p.dome((0, 0, 10.6), 2.6, GLASS, "glass", squash=0.6, segments=24)
    p.torus((0, 0, 8.2), 5.7, 0.12, GLOW_CYAN, "glow", segments=48)
    # Spire with a ball.
    p.cylinder((0, 0, 12.0), 0.14, 5.0, METAL, radius_top=0.04, segments=8)
    p.sphere((0, 0, 17.2), 0.35, GLOW_PINK, "glow", segments=12)
    # Boomerang fin sign on the right, leaning out; the glowing face looks at the street.
    fin = [(4.4, 0.0), (5.6, 0.0), (7.4, 8.5), (6.4, 9.2)]
    p.prism(fin, -0.2, 0.2, accent, plane="XZ")
    face = [(4.9, 1.2), (5.5, 1.2), (6.9, 7.8), (6.4, 8.1)]
    p.prism(face, -0.26, -0.2, GLOW_PINK, "glow", plane="XZ")
    p.sphere((6.9, 0.0, 9.4), 0.5, GLOW_CYAN, "glow", segments=12)
    return p


def pod_tower():
    """Flats on a tapering stalk: a rounded shop block at the foot, ribbon windows per floor,
    a bulb on top with its own hover-car pad, a glass lift tube up the side."""
    p = Part("pod_tower")
    body, accent = colour("#8fd4e8"), colour("#7a5cc4")
    # Shop block: a room with see-through windows on its straight sides; a fit-out from the plan
    # furnishes it (room anchor, 2 bays). The door sits in a solid stretch of wall, its leaf slides
    # up into the wall and the trim slab.
    t, half = 0.25, 5.5
    door_x, side = 1.5, (-1.6, 1.6, 0.9, 3.4)
    front = [(-3.4, -0.2, 0.9, 3.4), (2.9, 3.4, 0.9, 3.4)]
    hole = p.doorway("door_shop", door_x, -half, TRIM, accent, GLOW_CYAN, w=1.5, h=2.4, wall=t, use="shop")
    p.rounded_room((-half, -half, 0), (half, half, GROUND_STOREY), 2.0, t, body,
                   {"front": front + [hole], "left": [side], "right": [side]})
    for xa, xb, _, _ in front:
        p.box((xa, -half - 0.04, 0.9), (xb, -half + 0.02, 3.4), CLEAR, "clear")
    for sx in (-1, 1):
        x = sx * half
        p.box((min(x, x - sx * 0.06), -1.6, 0.9), (max(x + sx * 0.04, x - sx * 0.06), 1.6, 3.4), CLEAR, "clear")
    p.rounded_box((-half + t, -half + t, 0), (half - t, half - t, 0.02), 2.0 - t, colour("#7d7887"))
    p.rounded_box((-half, -half, GROUND_STOREY - 0.3), (half, half, GROUND_STOREY), 2.0, colour("#e6f3f7"))
    for x in (-2.5, 2.5):
        p.box((x - 0.15, -3.5, GROUND_STOREY - 0.35), (x + 0.15, 3.5, GROUND_STOREY - 0.3), LIT, "glow")
    p.anchor("room", (0, half - t - 0.02, 0.02), kind="room", bays=2, size=[7.36, 9.36, 4.18])
    p.rounded_box((-5.8, -5.8, GROUND_STOREY), (5.8, 5.8, GROUND_STOREY + 0.4), 2.2, TRIM)
    p.sign(-2.0, 3.6, 4.5, 0.8, -5.5, accent, GLOW_PINK)
    # Stalk: tapers from r 4 to r 3, one floor slab and one window ribbon per floor.
    z0, floors = GROUND_STOREY + 0.4, 8
    top = z0 + floors * STOREY

    def radius(z):
        return 4.0 - (z - z0) / (top - z0)

    p.cylinder((0, 0, z0), 4.0, top - z0, accent, radius_top=3.0, segments=24)
    for i in range(floors):
        z = z0 + i * STOREY
        r = radius(z + 1.0)
        lit = (i * 5) % 3 != 0
        p.cylinder((0, 0, z + 1.0), r + 0.08, 1.4, LIT if lit else GLASS, "glow" if lit else "glass", segments=24)
        p.cylinder((0, 0, z + STOREY - 0.25), radius(z + STOREY) + 0.5, 0.25, TRIM, segments=24)
    # Bulb on top with a glass band and an antenna.
    p.sphere((0, 0, top + 3.0), 4.6, body, squash=0.7, segments=24)
    p.cylinder((0, 0, top + 2.4), 4.62, 1.0, GLASS, "glass", segments=32)
    p.cylinder((0, 0, top + 6.0), 0.12, 4.0, METAL, segments=8)
    p.sphere((0, 0, top + 10.1), 0.3, GLOW_CYAN, "glow", segments=12)
    # Hover-car pad cantilevered from floor 6, with a glowing rim.
    pz = z0 + 6 * STOREY
    p.box((2.5, -0.5, pz - 0.6), (5.8, 0.5, pz), TRIM)
    p.cylinder((7.6, 0, pz - 0.3), 2.6, 0.3, TRIM, segments=24)
    p.torus((7.6, 0, pz), 2.5, 0.08, GLOW_CYAN, "glow", segments=32)
    p.anchor("pad", (7.6, 0, pz), kind="pad", size="car")
    # Glass lift tube with the cabin halfway up.
    p.cylinder((-4.9, -1.5, GROUND_STOREY + 0.4), 0.9, top - GROUND_STOREY - 0.4, GLASS, "glass", segments=16)
    p.cylinder((-4.9, -1.5, z0 + 3 * STOREY), 0.7, 2.2, LIT, "glow", segments=12)
    return p


def bubble_shop():
    """A narrow two-floor shop with rounded corners, a bubble on the roof and a tail-fin sign."""
    p = Part("bubble_shop")
    body, accent = colour("#ffd84d"), colour("#e2483d")
    w, d = 7.0, 8.0
    lo, hi = (-w / 2, -d / 2 + 1.5, 0), (w / 2, d / 2 - 0.2, GROUND_STOREY + STOREY)
    # Ground floor: a gumball shop, too shallow for a fit-out, so it carries its own furniture.
    t, r, gs = 0.25, 1.6, GROUND_STOREY
    side = (-1.3, 1.3, 0.8, 3.4)
    front = [(0.4, 1.8, 0.8, 3.4)]
    hole = p.doorway("door_shop", -0.9, lo[1], TRIM, accent, GLOW_PINK, w=1.5, h=2.5, wall=t, use="shop")
    p.rounded_room(lo, (hi[0], hi[1], gs), r, t, body, {"front": front + [hole], "left": [side], "right": [side]})
    p.box((0.4, lo[1] - 0.04, 0.8), (1.8, lo[1] + 0.02, 3.4), CLEAR, "clear")
    cy = (lo[1] + hi[1]) / 2
    for sx in (-1, 1):
        x = sx * w / 2
        p.box((min(x + sx * 0.04, x - sx * 0.02), cy - 1.3, 0.8), (max(x + sx * 0.04, x - sx * 0.02), cy + 1.3, 3.4),
              CLEAR, "clear")
    p.rounded_box((lo[0] + t, lo[1] + t, 0), (hi[0] - t, hi[1] - t, 0.02), r - t, colour("#f3c6d6"))
    p.rounded_box((lo[0], lo[1], gs - 0.3), (hi[0], hi[1], gs), r, TRIM)
    p.torus((0, cy, gs - 0.36), 1.6, 0.05, LIT, "glow", segments=32, sides=4)
    p.rounded_box((lo[0], lo[1], gs), hi, r, body)
    # Counter across the back, the shopkeeper behind it, a jar shelf on the back wall.
    back = hi[1] - t
    p.rounded_box((-2.0, back - 1.6, 0.02), (2.0, back - 1.0, 1.05), 0.25, accent, segments=4)
    p.rounded_box((-2.08, back - 1.68, 1.05), (2.08, back - 0.95, 1.11), 0.3, TRIM, segments=4)
    p.box((-2.2, back - 0.3, 0.02), (2.2, back, 2.2), colour("#f4f1e8"))
    for k, z in enumerate((0.9, 1.5)):
        p.box((-2.2, back - 0.45, z - 0.04), (2.2, back - 0.3, z), METAL)
        for j in range(7):
            x = -1.95 + j * 0.65
            p.cylinder((x, back - 0.38, z), 0.13, 0.3, (GLOW_PINK, GLOW_CYAN, LIT)[(j + k) % 3], "glow", segments=8)
    p.anchor("npc", (0, back - 0.5, 0.02), kind="npc", role="trader")
    # Giant gumball machines either side of the room: a clear globe full of balls on a red foot.
    balls = [colour(h) for h in ("#ff5fa2", "#5ef2e0", "#ffd84d", "#7fb069", "#e07b39", "#7a5cc4")]
    for k, (x, y) in enumerate(((-2.3, -0.6), (2.4, -0.4))):
        p.cylinder((x, y, 0.02), 0.45, 0.9, accent, radius_top=0.3, segments=16)
        p.box((x - 0.12, y - 0.33, 0.45), (x + 0.12, y - 0.28, 0.65), METAL)
        p.sphere((x, y, 1.55), 0.65, CLEAR, "clear", segments=16)
        for j in range(9):
            a = 2.4 * j + k
            p.sphere((x + 0.26 * math.cos(a), y + 0.26 * math.sin(a), 1.3 + 0.06 * j), 0.16,
                     balls[(j + k) % len(balls)], segments=8)
        p.cylinder((x, y, 2.2), 0.2, 0.12, accent, segments=12)
    # Swoosh canopy over the door, wider at the street end.
    canopy = [(-3.4, lo[1]), (0.4, lo[1]), (0.8, lo[1] - 1.4), (-3.8, lo[1] - 1.4)]
    p.prism(canopy, 3.3, 3.5, accent)
    p.box((-3.8, lo[1] - 1.45, 3.22), (0.8, lo[1] - 1.35, 3.3), GLOW_CYAN, "glow")
    # Roof: a white rim and a big glass bubble.
    p.rounded_box((lo[0], lo[1], hi[2]), (hi[0], hi[1], hi[2] + 0.4), 1.6, TRIM)
    p.dome((0.0, 1.1, hi[2] + 0.4), 2.6, GLASS, "glass", segments=24)
    # Tail fin on the right corner, sticking up past the roof, with a glowing star.
    fin = [(0.0, 0.0), (1.4, 0.0), (0.9, 6.0), (-0.6, 4.5)]
    p.prism([(x + 2.0, z + hi[2] - 1.0) for x, z in fin], lo[1] + 0.4, lo[1] + 0.7, accent, plane="XZ")
    p.sphere((3.0, lo[1] + 0.55, hi[2] + 5.3), 0.45, GLOW_PINK, "glow", segments=12)
    return p


MODELS = {
    "saucer_diner": (saucer_diner, (-6.0, -6.0, 7.6, 6.0)),
    "pod_tower": (pod_tower, (-6.0, -6.0, 10.4, 6.0)),
    "bubble_shop": (bubble_shop, (-4.0, -4.0, 4.0, 4.0)),
}

kit.run(MODELS, "content/city")
