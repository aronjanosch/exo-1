"""Smuggling lab props: a brew vat, a lab table, and three smuggling crates.

Each model has one silly idea, written in its docstring. Names and fiction are placeholders
until the initiator picks. Measures: art/city/BRIEF.md (guide values, not rules).

Run headless (writes content/smuggle/<id>.glb, renders optional):
    blender -b --python-exit-code 1 -P art/smuggle/lab.py -- --out content/smuggle --renders DIR [--only id,id]
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
VAT = colour("#b9c4cc")
RUST = colour("#b8573f")
MUSTARD = colour("#d9a441")
TEAL = colour("#2f7f86")
PLUM = colour("#6b4e8a")
LILAC = colour("#9a8fb0")
TAN = colour("#c9b48a")
SACK = colour("#d8c9a3")
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")
BREW = colour("#8c9a8a")
RED = colour("#ff4d4d")
BLUE = colour("#4d8dff")


def lab_kettle():
    """A brewing vat on three legs, a lid propped open on a rubber boot, a pipe to the wall.
    TODO(initiator): silly idea = the lid is held open by a rubber boot; the brew is grey-green
    so the game can recolour it per batch."""
    p = Part("lab_kettle", "prop")
    for ang in (90, 210, 330):
        a = math.radians(ang)
        p.cylinder((0.5 * math.cos(a), 0.5 * math.sin(a), 0), 0.06, 0.45, DARK, radius_top=0.05, segments=8)
    p.cylinder((0, 0, 0.4), 0.6, 0.6, VAT, radius_top=0.7, segments=24)
    p.torus((0, 0, 1.0), 0.7, 0.05, TRIM, segments=32, sides=6)
    p.cylinder((0, 0, 1.0), 0.64, 0.02, BREW, "brew", segments=24)
    # Pipe from the side of the vat, down to a pump on the ground.
    p.cylinder((0.6, 0, 0.75), 0.05, 0.5, RUST, segments=8, axis="X")
    p.cylinder((1.1, 0, 0.1), 0.05, 0.65, RUST, segments=8)
    p.box((0.9, -0.15, 0), (1.3, 0.15, 0.2), METAL)
    # Lid, tilted up on its back edge, with the rubber boot under its front edge.
    with p.placed(Matrix.Translation((0, 0, 1.32)) @ Matrix.Rotation(math.radians(-12), 4, "X")):
        p.dome((0, 0, 0), 0.72, MUSTARD, squash=0.25, segments=20)
    p.rounded_box((-0.1, -0.78, 1.02), (0.1, -0.42, 1.08), 0.05, PLUM, segments=4)   # boot sole
    p.box((-0.1, -0.78, 1.08), (0.1, -0.52, 1.46), PLUM)                              # boot leg
    return p


def lab_table():
    """A lab bench with jars and a burner. The burner's flame is a pink sphere on a dark ring.
    TODO(initiator): silly idea = the burner flame is a pink sphere, and the two status lights
    are red and blue (no reason given)."""
    p = Part("lab_table", "prop")
    for x in (-0.72, 0.72):
        for y in (-0.32, 0.32):
            p.box((x - 0.04, y - 0.04, 0), (x + 0.04, y + 0.04, 0.84), DARK)
    p.rounded_box((-0.8, -0.4, 0.84), (0.8, 0.4, 0.9), 0.05, METAL, segments=4)
    for (x, y, c) in ((-0.5, 0.1, TEAL), (-0.3, -0.15, PLUM), (0.1, 0.2, MUSTARD)):
        p.cylinder((x, y, 0.9), 0.09, 0.2, c, segments=12)
        p.cylinder((x, y, 1.1), 0.1, 0.04, TRIM, segments=12)
    p.cylinder((0.5, 0, 0.9), 0.12, 0.05, DARK, segments=12)
    p.torus((0.5, 0, 0.97), 0.1, 0.02, DARK, segments=12, sides=6)
    p.sphere((0.5, 0, 1.06), 0.08, GLOW_PINK, "glow", segments=10)
    p.sphere((0.6, -0.41, 0.87), 0.03, RED, "light_red", segments=6)
    p.sphere((0.7, -0.41, 0.87), 0.03, BLUE, "light_blue", segments=6)
    return p


def crate_product():
    """A medium crate (1.0 m, the game's medium size) in lilac with a glowing belt and a face on the
    label. TODO(initiator): silly idea = the crate wears a glowing belt to keep its trousers up;
    the label is a face."""
    p = Part("crate_product", "prop")
    p.box((-0.5, -0.5, 0), (0.5, 0.5, 1.0), LILAC)
    corners = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]
    p.ribbon(corners, -0.02, 0.04, 0.45, 0.55, TRIM, "band", closed=True)
    p.box((-0.3, -0.53, 0.7), (0.3, -0.5, 0.92), TRIM)     # label panel
    p.sphere((-0.12, -0.53, 0.84), 0.03, DARK, segments=6)  # label eyes
    p.sphere((0.12, -0.53, 0.84), 0.03, DARK, segments=6)
    return p


def crate_base():
    """A barrel the size of the crate. Its tap plays one note when a crate is opened.
    TODO(initiator): silly idea = the barrel's tap sings (the note is not decided)."""
    p = Part("crate_base", "prop")
    p.cylinder((0, 0, 0), 0.44, 0.5, MUSTARD, radius_top=0.5, segments=16)
    p.cylinder((0, 0, 0.5), 0.5, 0.5, MUSTARD, radius_top=0.44, segments=16)
    p.torus((0, 0, 0.2), 0.5, 0.03, DARK, segments=16, sides=6)
    p.torus((0, 0, 0.8), 0.5, 0.03, DARK, segments=16, sides=6)
    p.cylinder((0, -0.5, 0.5), 0.05, 0.12, METAL, segments=8, axis="Y")
    p.sphere((0, -0.6, 0.5), 0.05, GLOW_CYAN, "glow", segments=8)
    return p


def crate_ingredient():
    """A lumpy sack, tied at the top, with a green sprout coming out of the knot. A different
    silhouette from the crates on purpose. TODO(initiator): silly idea = the sprout (is it the
    ingredient?)."""
    p = Part("crate_ingredient", "prop")
    p.sphere((0, 0, 0.525), 0.5, SACK, squash=1.05, segments=16)
    for (x, y, z, r) in ((0.3, 0.2, 0.3, 0.18), (-0.3, 0.15, 0.6, 0.2), (0.1, -0.35, 0.45, 0.2),
                         (-0.2, -0.3, 0.2, 0.16), (0.35, -0.1, 0.8, 0.16), (-0.35, 0.0, 0.85, 0.14)):
        p.sphere((x, y, z), r, TAN if (x + y) > 0 else MUSTARD, segments=8)
    p.cylinder((0, 0, 0.95), 0.12, 0.15, SACK, radius_top=0.09, segments=10)
    p.sphere((0, 0, 1.12), 0.1, TAN, segments=8)
    p.cylinder((0, 0, 1.2), 0.02, 0.2, TEAL, segments=6)
    p.sphere((0.05, 0, 1.42), 0.08, colour("#7fc46a"), squash=1.6, segments=8)
    return p


MODELS = {
    "lab_kettle": (lab_kettle, (-0.8, -0.8, 1.35, 0.8)),
    "lab_table": (lab_table, (-0.8, -0.45, 0.8, 0.45)),
    "crate_product": (crate_product, (-0.55, -0.55, 0.55, 0.55)),
    "crate_base": (crate_base, (-0.55, -0.65, 0.55, 0.55)),
    "crate_ingredient": (crate_ingredient, (-0.6, -0.6, 0.6, 0.6)),
}

kit.run(MODELS, "content/smuggle")
