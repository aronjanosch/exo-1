"""Fit-outs: the furniture of a ground-floor room, one model per kind and room width.

A building's ground floor is an empty room with a `room` anchor (rows.py, `shell`); the city plan
picks a fit-out kind per placement (city_plan.py), so one house can be a bar in one street and a
workshop in the next. The game puts `fit_<kind>_<bays>` on the room anchor and a character on the
fit-out's `npc` anchor (`role` is the default; the plan may override it).

Kinds: shop, bar, workshop, office. Every kind keeps the front of the room clear (doors sit
anywhere along the front wall), puts a counter or desk across the back and the npc behind it.

Origin: on the floor at the back wall's inner face, centred; the room runs to -Y (the street).
Room: width bays * 4 m - 0.64, depth 9.36, ceiling 4.18 (rows.py, `shell`).

Run headless (writes content/city/fit_<kind>_<bays>.glb, renders optional):
    blender -b -P art/city/interiors.py -- --out content/city [--renders DIR] [--only id,id]
"""

import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import BAY, Part, colour  # noqa: E402

TRIM = colour("#f4f1e8")
METAL = colour("#9aa7b8")
DARK = colour("#3b3f4a")
GLASS = colour("#2a4a6b")
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")
LIT = colour("#ffe2a0")
GOODS = [colour(h) for h in ("#e07b39", "#2f7f86", "#d9a441", "#b8573f", "#6b4e8a", "#7fb069", "#ff5fa2")]

DEPTH = 9.36
HEIGHT = 4.18
COUNTER_Y = -2.6     # front face of the counter; the npc stands between it and the back wall


def room_width(bays):
    return bays * BAY - 0.64


# ---------------------------------------------------------------- pieces

def shelf(p, x0, x1, y0, y1, h, levels, rnd, frame=METAL):
    """An open shelf with boards and goods on them (boxes, jars, orbs)."""
    for x in (x0, x1 - 0.06):
        p.box((x, y0, 0), (x + 0.06, y1, h), frame)
    for k in range(levels):
        z = 0.15 + k * (h - 0.25) / max(1, levels - 1) if levels > 1 else 0.15
        p.box((x0, y0, z), (x1, y1, z + 0.04), TRIM)
        if z > h - 0.3:
            continue
        x = x0 + 0.15
        while x < x1 - 0.3:
            g = rnd.choice(GOODS)
            size = rnd.uniform(0.18, 0.3)
            cy = (y0 + y1) / 2
            kind = rnd.random()
            if kind < 0.45:
                p.box((x, cy - size / 2, z + 0.04), (x + size, cy + size / 2, z + 0.04 + size * 1.2), g)
            elif kind < 0.8:
                p.cylinder((x + size / 2, cy, z + 0.04), size / 2, size * 1.4, g, segments=8)
            else:
                p.sphere((x + size / 2, cy, z + 0.04 + size / 2), size / 2, g, "glow", segments=8)
            x += size + rnd.uniform(0.05, 0.2)


def counter(p, w, c, depth=0.7, height=1.05):
    """The counter across the back, rounded ends, a trim top and a glow strip along its foot."""
    y0 = COUNTER_Y
    p.rounded_box((-w / 2, y0, 0), (w / 2, y0 + depth, height), 0.3, c["accent"], segments=4)
    p.rounded_box((-w / 2 - 0.08, y0 - 0.08, height), (w / 2 + 0.08, y0 + depth + 0.05, height + 0.06), 0.35, TRIM,
                  segments=4)
    p.box((-w / 2 + 0.3, y0 - 0.03, 0.12), (w / 2 - 0.3, y0, 0.2), c["glow"], "glow")
    return height + 0.06


def stool(p, x, y, c):
    p.cylinder((x, y, 0), 0.22, 0.05, METAL, segments=12)
    p.cylinder((x, y, 0.05), 0.05, 0.65, METAL, segments=8)
    p.cylinder((x, y, 0.7), 0.22, 0.1, c["accent"], segments=12)


def plant(p, x, y):
    p.cylinder((x, y, 0), 0.25, 0.45, TRIM, radius_top=0.3, segments=12)
    p.sphere((x, y, 0.85), 0.42, colour("#7fb069"), squash=1.2, segments=12)
    p.sphere((x + 0.1, y - 0.1, 1.35), 0.12, GLOW_PINK, "glow", segments=8)


def npc(p, role):
    """Where the game puts the character: behind the counter, facing the room."""
    p.anchor("npc", (0, (COUNTER_Y + 0.7) / 2 - 0.25, 0), kind="npc", role=role)


# ---------------------------------------------------------------- kinds

def shop(p, w, rnd, c):
    """A corner shop: goods on every wall, a till on the counter, a glowing orb on show in bigger rooms."""
    shelf(p, -w / 2 + 0.2, w / 2 - 0.2, -0.45, 0.0, 2.6, 4, rnd)
    top = counter(p, min(w - 1.2, 4.0), c)
    p.box((0.4, COUNTER_Y + 0.15, top), (0.9, COUNTER_Y + 0.55, top + 0.3), DARK)
    p.box((0.45, COUNTER_Y + 0.12, top + 0.12), (0.85, COUNTER_Y + 0.15, top + 0.27), c["glow"], "glow")
    for sx in (-1, 1):
        x0, x1 = (-w / 2, -w / 2 + 0.5) if sx < 0 else (w / 2 - 0.5, w / 2)
        shelf(p, x0, x1, -7.6, -3.4, 2.2, 3, rnd)
    if w > 5:
        p.cylinder((0, -6.0, 0), 0.5, 0.9, TRIM, radius_top=0.35, segments=16)
        p.sphere((0, -6.0, 1.25), 0.35, c["glow"], "glow", segments=16)
        p.torus((0, -6.0, 1.25), 0.5, 0.03, METAL, segments=20, sides=4)
        p.box((-0.04, -6.04, 0.9), (0.04, -5.96, 1.0), METAL)
    plant(p, -w / 2 + 0.6, -8.6)
    npc(p, "trader")


def bar(p, w, rnd, c):
    """A bar: bottles on a lit back shelf, stools at the counter, booths along the walls, a neon ring."""
    shelf(p, -w / 2 + 0.3, w / 2 - 0.3, -0.35, 0.0, 2.2, 3, rnd, frame=DARK)
    p.box((-w / 2 + 0.3, -0.02, 2.4), (w / 2 - 0.3, 0.0, 2.6), c["glow"], "glow")
    cw = min(w - 0.8, 6.0)
    counter(p, cw, c)
    n = max(2, int(cw / 0.9))
    for k in range(n):
        stool(p, -cw / 2 + cw * (k + 0.5) / n, COUNTER_Y - 0.55, c)
    # Neon ring over the counter, hung from the ceiling.
    p.torus((0, COUNTER_Y + 0.35, 3.3), min(cw / 2 - 0.3, 1.6), 0.06, c["glow"], "glow", segments=36, sides=6)
    for sx in (-1, 1):
        x = sx * min(cw / 2 - 0.3, 1.6)
        p.cylinder((x, COUNTER_Y + 0.35, 3.3), 0.015, HEIGHT - 3.3, METAL, segments=4)
    if w > 5:
        for sx in (-1, 1):
            x_wall = sx * (w / 2)
            xi = x_wall - sx * 0.6
            for yb in (-5.2, -7.6):
                p.box((min(x_wall, xi), yb - 0.25, 0), (max(x_wall, xi), yb + 0.25, 0.45), c["accent"])
                p.box((min(x_wall, x_wall - sx * 0.15), yb - 0.25, 0.45), (max(x_wall, x_wall - sx * 0.15), yb + 0.25, 1.1),
                      c["accent"])
            # Table between the two benches.
            tx = x_wall - sx * 0.55
            p.cylinder((tx, -6.4, 0), 0.06, 0.72, METAL, segments=8)
            p.cylinder((tx, -6.4, 0.72), 0.5, 0.05, TRIM, segments=16)
            p.sphere((tx, -6.4, 0.85), 0.08, GLOW_PINK, "glow", segments=8)
    else:
        plant(p, -w / 2 + 0.5, -8.6)
    npc(p, "barkeep")


def workshop(p, w, rnd, c):
    """A workshop: a pegboard with tools, a bench, crates and drums, a half-built hover bike on a lift."""
    p.box((-w / 2 + 0.3, -0.05, 1.0), (w / 2 - 0.3, 0.0, 2.6), colour("#c9b79c"))
    x = -w / 2 + 0.6
    while x < w / 2 - 0.6:
        h = rnd.uniform(0.3, 0.7)
        p.box((x, -0.1, 1.9 - h / 2), (x + 0.06, -0.05, 1.9 + h / 2), rnd.choice((METAL, DARK, c["accent"])))
        x += rnd.uniform(0.25, 0.5)
    p.box((-w / 2 + 0.3, -0.8, 0), (w / 2 - 0.3, -0.05, 0.9), DARK)
    p.box((-w / 2 + 0.25, -0.85, 0.9), (w / 2 - 0.25, 0.0, 0.96), METAL)
    top = counter(p, min(w - 1.2, 3.2), {**c, "accent": DARK})
    p.box((-0.5, COUNTER_Y + 0.1, top), (0.1, COUNTER_Y + 0.5, top + 0.25), c["accent"])
    for sx in (-1, 1):
        xw = sx * (w / 2 - 0.55)
        for k, y in enumerate((-4.0, -5.1)):
            s = 0.9 - 0.15 * k
            p.box((xw - s / 2, y - s / 2, 0), (xw + s / 2, y + s / 2, s), colour("#b98f5a"))
            p.box((xw - s / 2 - 0.02, y - 0.06, 0), (xw + s / 2 + 0.02, y + 0.06, s + 0.02), DARK)
        p.cylinder((xw, -7.6, 0), 0.35, 1.0, c["accent"], segments=12)
        p.cylinder((xw, -7.6, 0.3), 0.36, 0.08, DARK, segments=12)
        p.cylinder((xw, -7.6, 0.7), 0.36, 0.08, DARK, segments=12)
    if w > 5:
        # The lift and the bike: a platform, a rounded body, a ring where the drive goes.
        p.box((-1.3, -7.2, 0), (1.3, -5.0, 0.15), METAL)
        p.box((-0.15, -6.2, 0.15), (0.15, -6.0, 0.8), DARK)
        p.rounded_box((-1.1, -6.6, 0.8), (1.1, -5.6, 1.35), 0.45, c["accent"], segments=5)
        p.sphere((0.6, -6.1, 1.35), 0.35, GLASS, "glass", squash=0.7, segments=12)
        p.torus((-1.25, -6.1, 1.05), 0.32, 0.08, METAL, segments=16, sides=6)
        p.cylinder((-1.25, -6.1, 1.05), 0.2, 0.05, c["glow"], "glow", segments=12, axis="X")
    npc(p, "mechanic")


def office(p, w, rnd, c):
    """An office: a desk with a screen, filing cabinets, waiting chairs, a water orb, a framed picture."""
    p.box((-1.0, -0.06, 1.4), (1.0, 0.0, 2.6), TRIM)
    p.box((-0.85, -0.08, 1.55), (0.85, -0.06, 2.45), c["glow"], "glow")
    top = counter(p, min(w - 1.2, 3.0), c, depth=0.8, height=0.78)
    p.box((-0.5, COUNTER_Y + 0.45, top), (0.5, COUNTER_Y + 0.55, top + 0.6), DARK)
    p.box((-0.45, COUNTER_Y + 0.44, top + 0.05), (0.45, COUNTER_Y + 0.45, top + 0.55), GLOW_CYAN, "glow")
    p.cylinder((0, COUNTER_Y + 0.5, top), 0.04, 0.1, METAL, segments=6)
    p.cylinder((0, -0.9, 0), 0.25, 0.45, DARK, segments=12)
    p.box((-0.3, -0.75, 0.45), (0.3, -0.65, 1.1), c["accent"])
    for sx in (-1, 1):
        xw = sx * (w / 2 - 0.35)
        for k in range(2 if w < 5 else 3):
            y = -3.6 - k * 0.75
            p.box((xw - 0.3, y - 0.35, 0), (xw + 0.3, y + 0.35, 1.3), METAL)
            for z in (0.35, 0.8):
                p.box((xw - sx * 0.31 - 0.01, y - 0.15, z), (xw - sx * 0.31 + 0.01, y + 0.15, z + 0.06), DARK)
    # Waiting chairs along one side wall, a water orb by the other.
    xs = -w / 2 + 0.45
    for k in range(3 if w > 5 else 2):
        y = -6.5 - k * 0.7
        p.box((xs - 0.25, y - 0.25, 0.42), (xs + 0.25, y + 0.25, 0.5), c["accent"])
        p.box((xs - 0.3, y - 0.25, 0.5), (xs - 0.22, y + 0.25, 1.0), c["accent"])
        p.box((xs - 0.03, y - 0.03, 0), (xs + 0.03, y + 0.03, 0.42), METAL)
    xo = w / 2 - 0.45
    p.cylinder((xo, -7.0, 0), 0.22, 1.0, TRIM, segments=12)
    p.sphere((xo, -7.0, 1.3), 0.3, colour("#9fdcef"), "glass", segments=12)
    plant(p, w / 2 - 0.5, -8.6)
    npc(p, "quest")


KINDS = {
    "shop": (shop, {"accent": colour("#2f7f86"), "glow": GLOW_CYAN}),
    "bar": (bar, {"accent": colour("#b8573f"), "glow": GLOW_PINK}),
    "workshop": (workshop, {"accent": colour("#d9a441"), "glow": GLOW_CYAN}),
    "office": (office, {"accent": colour("#3d5a80"), "glow": GLOW_CYAN}),
}


def fit_out(kind, bays):
    make, c = KINDS[kind]
    name = f"fit_{kind}_{bays}"
    p = Part(name, "interior")
    p.ceiling = HEIGHT
    make(p, room_width(bays), random.Random(name), c)
    return p


MODELS = {f"fit_{kind}_{bays}": ((lambda kind=kind, bays=bays: fit_out(kind, bays)),
                                 (-room_width(bays) / 2, -DEPTH, room_width(bays) / 2, 0.0))
          for kind in KINDS for bays in (1, 2, 3)}

kit.run(MODELS, "content/city")
