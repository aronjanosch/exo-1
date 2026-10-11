"""Smuggling-quarter stand-ins: three buyer placeholders and the order marker.

The buyers are plain placeholders, not character designs: a capsule body, a head ball and one hat or
antenna each, one colour per variant. Each stands on its origin (ground centre, +Y front), about 1.8 m.
The order marker is a floating octahedron, about 0.6 m, that marks a quest; it has its own centre.

Run headless (writes content/smuggle/<id>.glb, renders optional):
    blender -b --python-exit-code 1 -P art/smuggle/folks.py -- --out content/smuggle --renders DIR [--only id,id]
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
from kit import Part, colour  # noqa: E402

TRIM = colour("#f4f1e8")
DARK = colour("#3b3f4a")
METAL = colour("#9aa7b8")
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")
MUSTARD = colour("#d9a441")
PLUM = colour("#8a6fa8")
TEAL = colour("#5f9ea0")
ROSE = colour("#c98fa0")
RUST = colour("#b8573f")
SLATE = colour("#6b7a8f")


def capsule(p, rgb, radius=0.3):
    """A standing capsule from the ground up to z = 1.3 m: rounded foot, straight body, rounded top."""
    p.sphere((0, 0, radius), radius, rgb, segments=12)
    p.cylinder((0, 0, radius), radius, 1.0 - radius, rgb, segments=12)
    p.sphere((0, 0, 1.0), radius, rgb, segments=12)


def head(p, z=1.45, r=0.2):
    p.sphere((0, 0, z), r, TRIM, segments=12)
    for x in (-0.08, 0.08):
        p.sphere((x, -r * 0.85, z + 0.02), 0.04, DARK, segments=6)


def buyer_a():
    """Rose body, a pointed mustard hat. Placeholder: the hat is a cone."""
    p = Part("buyer_a", "prop")
    capsule(p, ROSE)
    head(p)
    p.cylinder((0, 0, 1.55), 0.22, 0.3, MUSTARD, radius_top=0.02, segments=10)
    return p


def buyer_b():
    """Teal body, a thin antenna with a glowing ball. Placeholder: the antenna is the hat."""
    p = Part("buyer_b", "prop")
    capsule(p, TEAL)
    head(p)
    p.cylinder((0, 0, 1.55), 0.02, 0.2, METAL, segments=6)
    p.sphere((0, 0, 1.8), 0.06, GLOW_CYAN, "glow", segments=8)
    return p


def buyer_c():
    """Plum body, a flat brimmed hat in rust. Placeholder: a brim and a crown."""
    p = Part("buyer_c", "prop")
    capsule(p, PLUM)
    head(p)
    p.cylinder((0, 0, 1.6), 0.36, 0.04, RUST, segments=14)
    p.cylinder((0, 0, 1.6), 0.2, 0.18, RUST, segments=12)
    return p


def order_marker():
    """A floating octahedron, gold above and a glowing pink point below: the quest is here.

    Origin at the lower tip, so the kit's ground rule holds; the game floats it over the buyer's head.
    TODO(initiator): shape and colours of the order icon.
    """
    p = Part("order_marker", "prop")
    r, h = 0.4, 0.5
    top = p.bm.verts.new((0, 0, 2 * h))
    bot = p.bm.verts.new((0, 0, 0))
    eq = [p.bm.verts.new((x, y, h)) for x, y in ((r, 0), (0, r), (-r, 0), (0, -r))]
    up, down = [], []
    for i in range(4):
        a, b = eq[i], eq[(i + 1) % 4]
        up.append(p.bm.faces.new((top, a, b)))
        down.append(p.bm.faces.new((bot, b, a)))
    p._paint(up, MUSTARD, "paint")
    p._paint(down, GLOW_PINK, "glow")
    return p


MODELS = {
    "buyer_a": (buyer_a, (-0.45, -0.45, 0.45, 0.45)),
    "buyer_b": (buyer_b, (-0.45, -0.45, 0.45, 0.45)),
    "buyer_c": (buyer_c, (-0.45, -0.45, 0.45, 0.45)),
    "order_marker": (order_marker, (-0.45, -0.45, 0.45, 0.45)),
}

kit.run(MODELS, "content/smuggle")
