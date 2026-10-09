"""Row houses and corner houses: boxy blocks that stand wall to wall, Schedule I grown upwards,
retro-future on top.

Every house is the same recipe: a ground floor that is a real room (shell walls, a door that opens,
see-through shop windows, an empty room the game fills with a fit-out from interiors.py), solid
upper floors, a street facade in relief (piers per bay, floor bands,
framed windows sitting back between them), a shop floor, a false front that rises above the roof
and gives the house its silhouette, a plain back facade, junk on the roof, sometimes one silly
topper. Corner houses carry a second street facade on one side and a turret or sign on the corner.
Names and fiction are placeholders until the initiator picks. Look and measures: BRIEF.md.

Origin: front centre on the ground; the street front is y = 0 and faces -Y, the house runs back to
y = depth. Neighbours touch at x = +-width/2.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/rows.py -- --out content/city [--renders DIR] [--only id,id]
"""

import math
import os
import random
import sys

from mathutils import Matrix

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import BAY, GROUND_STOREY, STOREY, WALL, Part, colour  # noqa: E402

TRIM = colour("#f4f1e8")
METAL = colour("#9aa7b8")
DARK = colour("#3b3f4a")
GLASS = colour("#2a4a6b")
LIT = colour("#ffe2a0")
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")
CLEAR = colour("#cfeef5")    # tint of see-through shop glass
FLOOR = colour("#7d7887")
CEILING = GROUND_STOREY - 0.3  # underside of the ground floor's ceiling slab

DEPTH = 10.0
OVERHANG = 1.6   # awnings and blade signs reach this far over the walkway
PIER = 0.5       # pier width; piers stand 0.3 m proud of the wall, windows sit back between them


def lit(i, j):
    """Which windows are lit: a fixed scatter, so reruns look the same."""
    return (i * 7 + j * 3) % 5 < 2


def top_of(upper):
    return GROUND_STOREY + upper * STOREY


# ---------------------------------------------------------------- facade parts (local frame)
# A facade is built in its own frame: wall plane y = 0, facing -Y, spanning x in [-w/2, w/2].
# `Part.placed` turns it onto any side of the body.

def pane(p, cx, z, w, h, frame, on, bars=(0, 1), clear=False):
    """A window set into the wall: frame strips, a deep sill, glass, mullions (vertical, horizontal).
    clear: see-through glass over a hole in the wall (shop windows into the room)."""
    g0, g1 = cx - w / 2, cx + w / 2
    p.box((g0 - 0.12, -0.12, z + h), (g1 + 0.12, 0, z + h + 0.12), frame)
    p.box((g0 - 0.2, -0.22, z - 0.14), (g1 + 0.2, 0, z), frame)
    p.box((g0 - 0.12, -0.12, z), (g0, 0, z + h), frame)
    p.box((g1, -0.12, z), (g1 + 0.12, 0, z + h), frame)
    if clear:
        p.box((g0, -0.04, z), (g1, 0, z + h), CLEAR, "clear")
    else:
        p.box((g0, -0.04, z), (g1, 0, z + h), LIT if on else GLASS, "glow" if on else "glass")
    nv, nh = bars
    for k in range(1, nv + 1):
        x = g0 + w * k / (nv + 1)
        p.box((x - 0.04, -0.08, z), (x + 0.04, -0.04, z + h), frame)
    for k in range(1, nh + 1):
        zz = z + h * (0.62 if nh == 1 else k / (nh + 1))
        p.box((g0, -0.07, zz - 0.04), (g1, -0.04, zz + 0.04), frame)   # behind the vertical bars


def bay_windows(p, style, cx, z, frame, panel, on):
    if style == "pair":
        for dx in (-0.8, 0.8):
            pane(p, cx + dx, z + 0.9, 1.1, 1.8, frame, on)
    elif style == "wide":
        pane(p, cx, z + 1.0, 2.7, 1.5, frame, on, bars=(2, 0))
    elif style == "tall":
        pane(p, cx, z + 0.5, 1.5, 2.6, frame, on, bars=(1, 1))
    elif style == "porthole":
        p.cylinder((cx, 0, z + 1.8), 0.9, 0.18, frame, segments=24, axis="Y")
        p.cylinder((cx, -0.05, z + 1.8), 0.7, 0.18, LIT if on else GLASS, "glow" if on else "glass",
                   segments=24, axis="Y")
    # A spandrel panel under the window, a hair proud of the wall.
    if style != "porthole":
        p.box((cx - 1.4, -0.05, z + 0.2), (cx + 1.4, 0, z + 0.65), panel)


def facade(p, w, upper, c, windows, doors, shop=True, ends=(0.0, 0.0)):
    """Street facade, including the ground floor's front wall (y from 0 to WALL). doors: [(name, x,
    use, door_rgb, width, height)]; shop doors open into the room, the others are flat (instanced
    behind). shop=True: see-through shop glass, awning and fascia sign on the ground floor; otherwise
    ground-floor windows like the floors above. ends: how much of the front wall to leave out at
    the left and right end, where another wall already stands (a corner house's side facade)."""
    bays = round(w / BAY)
    x0, x1 = -w / 2, w / 2
    top = top_of(upper)
    for k in range(bays + 1):
        x = x0 + k * BAY
        a, b = max(x0, x - PIER / 2), min(x1, x + PIER / 2)
        if k == 0:
            b = x0 + PIER
        if k == bays:
            a = x1 - PIER
        z0 = 0 if k in (0, bays) else GROUND_STOREY
        p.box((a, -0.3, z0), (b, 0, top), c["frame"])
    for i in range(1, upper):
        z = GROUND_STOREY + i * STOREY
        p.box((x0 + 0.01, -0.25, z - 0.12), (x1 - 0.01, 0, z + 0.12), c["frame"])   # inside the end piers

    holes = []
    for name, x, use, rgb, dw, dh in doors:
        if use == "shop":
            holes.append(p.doorway(name, x, 0, c["frame"], rgb, c["glow"], w=dw, h=dh, use=use))
        else:
            p.door(name, x, 0, c["frame"], rgb, w=dw, h=dh, use=use)
    if shop:
        # Shop glass fills what the doors leave free.
        free = [(x0 + 0.7, x1 - 0.7)]
        for _, x, _, _, dw, _ in doors:
            cut = (x - dw / 2 - 0.6, x + dw / 2 + 0.6)
            free = [s for a, b in free for s in ((a, min(b, cut[0])), (max(a, cut[1]), b)) if s[1] - s[0] > 0.01]
        for a, b in free:
            if b - a > 1.0:
                pane(p, (a + b) / 2, 0.7, b - a, 2.0, c["frame"], True, bars=(max(0, int((b - a) / 2)), 0), clear=True)
                holes.append((a, b, 0.7, 2.7))
        awning = [(0, 3.35), (-1.4, 2.95), (-1.4, 2.8), (0, 3.15)]
        p.prism(awning, x0 + 0.5, x1 - 0.5, c["accent"], plane="YZ")
        p.box((x0 + 0.01, -0.45, GROUND_STOREY - 0.6), (x1 - 0.01, 0, GROUND_STOREY + 0.2), c["frame"])
        p.sign(0, GROUND_STOREY - 0.5, w - 1.6, 0.6, -0.45, DARK, c["glow"])
    else:
        door_xs = [x for _, x, *_ in doors]
        for j in range(bays):
            cx = x0 + BAY * (j + 0.5)
            if all(abs(cx - dx) > 1.6 for dx in door_xs):
                bay_windows(p, windows, cx, 0.0, c["frame"], c["panel"], lit(9, j))
        p.box((x0 + 0.01, -0.25, GROUND_STOREY - 0.12), (x1 - 0.01, 0, GROUND_STOREY + 0.12), c["frame"])
    # The walls stop under the ceiling slab and the lining stays between the side walls, so no two
    # parts share an outer face (z-fighting).
    p.wall((x0 + ends[0], 0, 0), (x1 - ends[1], WALL, CEILING), c["body"], holes)
    p.wall((x0 + WALL, WALL, 0.02), (x1 - WALL, WALL + 0.02, CEILING), c["inside"], holes)

    for i in range(upper):
        z = GROUND_STOREY + i * STOREY
        for j in range(bays):
            bay_windows(p, windows, x0 + BAY * (j + 0.5), z, c["frame"], c["panel"], lit(i, j))


def back_facade(p, w, upper, c):
    """The back: small plain windows, a back door, a drainpipe. Seen from the air and the back yards."""
    bays = round(w / BAY)
    x0, x1 = -w / 2, w / 2
    for i in range(upper + 1):
        z = 0 if i == 0 else GROUND_STOREY + (i - 1) * STOREY
        for j in range(bays):
            if i == 0 and j == 0:
                continue
            pane(p, x0 + BAY * (j + 0.5), z + 1.1, 1.1, 1.3, c["frame"], lit(i + 3, j), bars=(0, 0))
    p.door("door_back", x0 + BAY / 2, 0, c["frame"], DARK, w=1.2, h=2.3, use="back")
    p.cylinder((x1 - 0.4, -0.15, 0), 0.09, top_of(upper), METAL, segments=8)


def false_front(p, shape, x0, x1, top, colour_):
    """Outline of the front above the roof, in the front view (x, z). Returns where a sign fits (z) or None."""
    w = x1 - x0
    if shape == "step":
        h = [1.0, 2.0, 3.0]
        s = [w / 2, w * 0.32, w * 0.14]
        # Right side up the steps, across the top, down the left.
        pts = [(x0, top), (x1, top), (x1, top + h[0]), (s[1], top + h[0]), (s[1], top + h[1]), (s[2], top + h[1]),
               (s[2], top + h[2]), (-s[2], top + h[2]), (-s[2], top + h[1]), (-s[1], top + h[1]), (-s[1], top + h[0]),
               (x0, top + h[0])]
        sign = top + 0.3
    elif shape == "arch":
        r = w / 2 - 0.3
        pts = [(x0, top), (x1, top), (x1, top + 0.9)]
        pts += [(r * math.cos(math.pi * k / 16), top + 0.9 + r * 0.55 * math.sin(math.pi * k / 16)) for k in range(17)]
        pts += [(x0, top + 0.9)]
        sign = top + 0.5
    elif shape == "fin":
        pts = [(x0, top), (x1, top), (x1, top + 4.5), (x1 - 0.9, top + 3.6), (x0, top + 0.9)]
        sign = None
    elif shape == "butterfly":
        pts = [(x0, top), (x1, top), (x1, top + 2.6), (0, top + 0.5), (x0, top + 2.6)]
        sign = None
    elif shape == "saw":
        # Three teeth leaning left, like a factory roof seen from the front.
        n = 3
        pts = [(x0, top), (x1, top)]
        for k in range(n, 0, -1):
            a = x0 + w * k / n
            pts.append((a, top + 2.2))
            pts.append((a - w / n, top + 0.6))
        sign = None
    else:
        return None
    p.prism(pts, -0.3, 0.2, colour_, plane="XZ")
    return sign


def roof_junk(p, name, x0, x1, y0, y1, roof, keep_clear):
    """Air units, vents and a dish, scattered behind the false front, away from the topper."""
    rnd = random.Random(name)
    spots = []
    for x in (x0 + 1.4, x1 - 1.4, x0 + 2.6, x1 - 2.6):
        # Far enough apart that two units never overlap (their tops would z-fight).
        if abs(x) > keep_clear and x0 + 1 < x < x1 - 1 and all(abs(x - o) >= 1.6 for o in spots):
            spots.append(x)
    for x in spots[:3]:
        y = rnd.uniform(y0 + 3.0, y1 - 2.0)
        kind = rnd.choice(("ac", "vent", "ac", "dish"))
        if kind == "ac":
            p.box((x - 0.7, y - 0.5, roof), (x + 0.7, y + 0.5, roof + 0.9), METAL)
            p.cylinder((x, y, roof + 0.9), 0.38, 0.06, DARK, segments=12)
        elif kind == "vent":
            p.cylinder((x, y, roof), 0.25, 1.4, METAL, segments=10)
            p.cylinder((x, y, roof + 1.4), 0.4, 0.25, DARK, radius_top=0.1, segments=10)
        else:
            p.cylinder((x, y, roof), 0.06, 1.6, METAL, segments=8)
            p.cylinder((x, y, roof + 1.6), 0.15, 0.4, TRIM, radius_top=0.8, segments=16, axis="Y")


def topper(p, kind, w, cy, roof, c):
    """The house's one silly idea on the roof. Returns how much roof it keeps clear around x = 0."""
    if kind == "dome":
        r = min(w / 2 - 0.8, 3.0)
        p.cylinder((0, cy, roof), r + 0.2, 0.4, c["frame"], segments=32)
        p.dome((0, cy, roof + 0.4), r, GLASS, "glass", segments=24)
        p.torus((0, cy, roof + 0.4), r + 0.2, 0.1, c["glow"], "glow", segments=40)
        p.cylinder((0, cy, roof + 0.4 + r), 0.1, 2.5, METAL, segments=8)
        p.sphere((0, cy, roof + 3.2 + r), 0.3, c["glow"], "glow", segments=12)
        return r + 0.3
    if kind == "tank":
        # A round tank on stilts, for something nobody explains.
        for sx in (-1.2, 1.2):
            for sy in (-1.2, 1.2):
                p.cylinder((sx, cy + sy, roof), 0.12, 2.6, METAL, segments=8)
        p.sphere((0, cy, roof + 4.2), 2.0, c["accent"], segments=24)
        p.cylinder((0, cy, roof + 3.9), 2.04, 0.5, c["glow"], "glow", segments=32)
        p.cylinder((0, cy, roof + 6.0), 0.1, 1.5, METAL, segments=8)
        return 2.2
    if kind == "pad":
        p.cylinder((0, cy, roof), 0.8, 1.5, METAL, segments=16)
        p.cylinder((0, cy, roof + 1.5), 3.0, 0.3, TRIM, segments=32)
        p.torus((0, cy, roof + 1.8), 2.9, 0.08, c["glow"], "glow", segments=40)
        p.anchor("pad", (0, cy, roof + 1.8), kind="pad", size="car")
        return 3.2
    if kind == "bulb":
        # A mast with a big glowing ball and a ring around it: a sign without words.
        p.cylinder((0, cy, roof), 0.5, 0.4, DARK, segments=12)
        p.cylinder((0, cy, roof + 0.4), 0.15, 4.0, METAL, segments=10)
        p.sphere((0, cy, roof + 5.4), 1.3, c["glow"], "glow", segments=20)
        p.torus((0, cy, roof + 5.4), 1.9, 0.12, c["accent"], segments=36, sides=8)
        return 2.1
    if kind == "mast":
        p.cylinder((0, cy, roof), 0.3, 9.0, METAL, radius_top=0.08, segments=10)
        for k, z in enumerate((3.0, 5.5, 7.5)):
            p.torus((0, cy, roof + z), 0.9 - k * 0.25, 0.06, c["glow"], "glow", segments=24, sides=6)
        p.sphere((0, cy, roof + 9.2), 0.3, GLOW_PINK, "glow", segments=12)
        return 1.2
    return 0.0


def blade_sign(p, x, upper, c):
    """A blade sign sticking out over the walkway; its faces look up and down the street."""
    bz = GROUND_STOREY + 0.6
    bh = min(6.0, upper * STOREY - 1.0)
    p.box((x - 0.15, -OVERHANG, bz), (x + 0.15, 0, bz + bh), c["accent"])
    for a, b in ((x - 0.2, x - 0.15), (x + 0.15, x + 0.2)):
        p.box((a, -OVERHANG + 0.2, bz + 0.3), (b, -0.5, bz + bh - 0.3), c["glow"], "glow")


def palette(body, frame, accent, glow):
    return {"body": body, "frame": frame, "accent": accent, "glow": glow, "panel": tuple(v * 0.75 for v in body),
            "inside": tuple(v + (1 - v) * 0.55 for v in body)}


def shell(p, x0, x1, d, height, c, open_sides):
    """The body: a ground-floor room (floor, lined walls, ceiling with light strips) and solid upper
    floors. open_sides: walls a facade builds itself ("front", "left", "right"). Puts the `room`
    anchor on the floor at the back wall's inner face, centred; a fit-out stands there, facing -Y."""
    # No two parts overlap: the back wall runs the full width, the side walls stand between front
    # and back, everything stops under the ceiling slab, floor and linings stay inside the walls.
    gs, t = GROUND_STOREY, 0.02
    walls = {"back": ((x0, d - WALL, 0), (x1, d, CEILING)), "left": ((x0, WALL, 0), (x0 + WALL, d - WALL, CEILING)),
             "right": ((x1 - WALL, WALL, 0), (x1, d - WALL, CEILING))}
    linings = {"back": ((x0 + WALL, d - WALL - t, t), (x1 - WALL, d - WALL, CEILING)),
               "left": ((x0 + WALL, WALL + t, t), (x0 + WALL + t, d - WALL - t, CEILING)),
               "right": ((x1 - WALL - t, WALL + t, t), (x1 - WALL, d - WALL - t, CEILING))}
    for side, (lo, hi) in walls.items():
        if side not in open_sides:
            p.box(lo, hi, c["body"])
            p.box(*linings[side], c["inside"])
    p.box((x0 + WALL, WALL, 0), (x1 - WALL, d - WALL, t), FLOOR)
    p.box((x0, 0, CEILING), (x1, d, gs), c["inside"])
    p.box((x0, 0, gs), (x1, d, height), c["body"])
    bays = round((x1 - x0) / BAY)
    for j in range(bays):
        cx = x0 + BAY * (j + 0.5)
        p.box((cx - 0.15, 1.2, CEILING - 0.05), (cx + 0.15, d - 1.2, CEILING), LIT, "glow")
    p.anchor("room", ((x0 + x1) / 2, d - WALL - 0.02, 0.02), kind="room", bays=bays,
             size=[round(x1 - x0 - 2 * WALL - 0.04, 2), round(d - 2 * WALL - 0.04, 2), round(CEILING - 0.02, 2)])


def turned(angle_deg, at):
    return Matrix.Translation(at) @ Matrix.Rotation(math.radians(angle_deg), 4, "Z")


# ---------------------------------------------------------------- houses

def row_house(name, bays, upper, c, windows="pair", front="flat", top=None, blade=False, garage=False):
    p = Part(name)
    w = bays * BAY
    x0, x1 = -w / 2, w / 2
    height = top_of(upper)
    shell(p, x0, x1, DEPTH, height, c, {"front"})
    if garage:
        doors = [("door_shop", 0.0, "shop", c["accent"], min(w - 3.0, 7.0), 3.6)]
    else:
        doors = [("door_shop", x0 + 1.5, "shop", c["accent"], 1.5, 2.5)]
        if bays >= 2:
            doors.append(("door_flat", x1 - 1.5, "flat", DARK, 1.5, 2.5))
    facade(p, w, upper, c, windows, doors)
    with p.placed(turned(180, (0, DEPTH, 0))):
        back_facade(p, w, upper, c)

    p.box((x0, -0.4, height), (x1, DEPTH, height + 0.4), c["frame"])
    roof = height + 0.4
    sign_z = false_front(p, front, x0, x1, roof, c["frame"])
    if sign_z is not None:
        p.sign(0, sign_z, min(w - 2.0, 5.0), 0.9, -0.3, DARK, c["glow"])
    if blade:
        blade_sign(p, x1 - 1.0, upper, c)
    clear = topper(p, top, w, DEPTH / 2 + 0.5, roof, c)
    roof_junk(p, name, x0, x1, 0, DEPTH, roof, clear)
    return p


def corner_house(name, bays, side_bays, upper, c, windows="pair", side="left", corner="turret", front="flat"):
    """Facades on the front and on one side (left: -X, right: +X), something tall on the corner."""
    p = Part(name)
    w, d = bays * BAY, side_bays * BAY
    x0, x1 = -w / 2, w / 2
    s = -1 if side == "left" else 1
    sx = x1 if s > 0 else x0
    height = top_of(upper)
    shell(p, x0, x1, d, height, c, {"front", side})
    facade(p, w, upper, c, windows, [("door_shop", -s * (w / 2 - 1.5), "shop", c["accent"], 1.5, 2.5)])
    # Side street facade, turned onto the side; its door sits at the far end from the corner.
    with p.placed(turned(90 * s, (sx, d / 2, 0))):
        facade(p, d, upper, c, windows, [("door_flat", s * (d / 2 - 1.5), "flat", DARK, 1.5, 2.5)],
               ends=(WALL, WALL))
    with p.placed(turned(180, (0, d, 0))):
        back_facade(p, w, upper, c)

    p.box((x0, -0.4, height), (x1, d, height + 0.4), c["frame"])
    a, b = (sx, sx + 0.4) if s > 0 else (sx - 0.4, sx)
    p.box((a, -0.4, height), (b, d, height + 0.4), c["frame"])
    roof = height + 0.4
    false_front(p, front, x0, x1, roof, c["frame"])

    if corner == "turret":
        # A round turret on the corner, taller than the house, with lit rings and a spire.
        r = 1.7
        p.cylinder((sx, 0, 0), r, height + 3.5, c["accent"], segments=24)
        p.cylinder((sx, 0, GROUND_STOREY - 1.0), r + 0.12, 0.25, c["frame"], segments=24)
        for k in range(upper):
            z = GROUND_STOREY + k * STOREY
            p.cylinder((sx, 0, z + STOREY - 0.2), r + 0.12, 0.25, c["frame"], segments=24)
            on = lit(k, 5)
            p.cylinder((sx, 0, z + 1.1), r + 0.05, 1.2, LIT if on else GLASS, "glow" if on else "glass", segments=24)
        p.cylinder((sx, 0, height + 3.5), r + 0.3, 0.3, c["frame"], segments=24)
        p.dome((sx, 0, height + 3.8), r, c["accent"], squash=1.3, segments=20)
        p.cylinder((sx, 0, height + 3.8 + r * 1.3), 0.08, 3.0, METAL, segments=8)
        p.sphere((sx, 0, height + 7.0 + r * 1.3), 0.3, c["glow"], "glow", segments=12)
    else:
        # A tall sign pylon standing diagonally on the corner, readable from both streets.
        h = height + 5.0
        with p.placed(turned(45 * s, (sx, 0, 0))):
            p.box((-0.3, -1.2, GROUND_STOREY), (0.3, 0.3, h), c["accent"])
            for a2, b2 in ((-0.36, -0.3), (0.3, 0.36)):
                p.box((a2, -1.0, GROUND_STOREY + 0.5), (b2, 0.1, h - 0.5), c["glow"], "glow")
            p.sphere((0, -0.45, h + 0.6), 0.6, c["glow"], "glow", segments=16)
    roof_junk(p, name, x0 + (2.5 if s < 0 else 0), x1 - (2.5 if s > 0 else 0), 0, d, roof, 0.0)
    return p


def row_lot(bays):
    return (-bays * BAY / 2, -OVERHANG, bays * BAY / 2, DEPTH + 0.3)


def corner_lot(bays, side_bays, side):
    w, d = bays * BAY / 2, side_bays * BAY
    return (-w - 2.0, -2.0, w, d + 0.3) if side == "left" else (-w, -2.0, w + 2.0, d + 0.3)


# Muted bodies, a light frame, one strong accent, one glow: the glows and accents carry the colour.
ROWS = {
    "row_step": dict(bays=2, upper=3, c=palette(colour("#c9b79c"), colour("#efe6d6"), colour("#2f7f86"), GLOW_CYAN),
                     windows="pair", front="step"),
    "row_butterfly": dict(bays=2, upper=2, c=palette(colour("#93ab9b"), colour("#e8e4d8"), colour("#b8573f"), GLOW_PINK),
                          windows="wide", front="butterfly", top="dome"),
    "row_arch": dict(bays=3, upper=4, c=palette(colour("#b98f8a"), colour("#f0e2d6"), colour("#3d5a80"), GLOW_CYAN),
                     windows="tall", front="arch"),
    "row_fin": dict(bays=1, upper=5, c=palette(colour("#8593a6"), colour("#dfe3e8"), colour("#d9a441"), GLOW_PINK),
                    windows="porthole", front="fin", blade=True),
    "row_tower": dict(bays=2, upper=7, c=palette(colour("#bfa46f"), colour("#f2ead8"), colour("#6b4e8a"), GLOW_CYAN),
                      windows="pair", top="pad"),
    "row_tank": dict(bays=2, upper=1, c=palette(colour("#a3c4bc"), colour("#f4f1e8"), colour("#c4473a"), GLOW_PINK),
                     windows="wide", front="step", top="tank", blade=True),
    "row_bulb": dict(bays=2, upper=3, c=palette(colour("#9fb4c7"), colour("#eef0f2"), colour("#e07b39"), GLOW_CYAN),
                     windows="porthole", front="arch", top="bulb"),
    "row_slant": dict(bays=3, upper=2, c=palette(colour("#c7a9b5"), colour("#f3ece9"), colour("#2f7f86"), GLOW_PINK),
                      windows="wide", front="fin", blade=True),
    "row_needle": dict(bays=1, upper=7, c=palette(colour("#b7b08f"), colour("#efeadb"), colour("#b8573f"), GLOW_CYAN),
                       windows="tall", front="step", top="mast"),
    "row_garage": dict(bays=3, upper=1, c=palette(colour("#a9a39b"), colour("#ecebe7"), colour("#d9a441"), GLOW_CYAN),
                       windows="wide", top="pad", garage=True),
    "row_saw": dict(bays=2, upper=4, c=palette(colour("#b4c6a6"), colour("#eef2e8"), colour("#6b4e8a"), GLOW_PINK),
                    windows="pair", front="saw"),
    "row_twin": dict(bays=2, upper=5, c=palette(colour("#d1b38c"), colour("#f5ecdc"), colour("#3d5a80"), GLOW_CYAN),
                     windows="wide", front="butterfly", top="dome"),
}

CORNERS = {
    "corner_turret": dict(bays=3, side_bays=3, upper=4, side="left", corner="turret", windows="pair",
                          c=palette(colour("#c2a68e"), colour("#f2e9dc"), colour("#2f7f86"), GLOW_CYAN)),
    "corner_sign": dict(bays=2, side_bays=3, upper=3, side="right", corner="sign", windows="wide", front="step",
                        c=palette(colour("#8fa3a8"), colour("#e9eef0"), colour("#c4473a"), GLOW_PINK)),
}

MODELS = {mid: ((lambda mid=mid, spec=spec: row_house(mid, **spec)), row_lot(spec["bays"])) for mid, spec in ROWS.items()}
MODELS.update({mid: ((lambda mid=mid, spec=spec: corner_house(mid, **spec)),
                     corner_lot(spec["bays"], spec["side_bays"], spec["side"])) for mid, spec in CORNERS.items()})

kit.run(MODELS, "content/city")
