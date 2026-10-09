"""Street and roof props: the small stuff that makes a street look lived in.

Each prop stands on its origin; where it has a front, the front faces -Y like the buildings.
Names are placeholders until the initiator picks. Look and measures: BRIEF.md.

Run headless (writes content/city/<id>.glb, renders optional):
    blender -b -P art/city/props.py -- --out content/city [--renders DIR] [--only id,id]
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
GLOW_CYAN = colour("#5ef2e0")
GLOW_PINK = colour("#ff5fa2")
TEAL = colour("#2f7f86")
RUST = colour("#b8573f")
MUSTARD = colour("#d9a441")
PLUM = colour("#6b4e8a")
LIT_BULB = colour("#ffe2a0")


def bin_bot():
    """A bin with a domed lid and one glowing eye: it looks at you while you throw things in."""
    p = Part("bin_bot", "prop")
    p.cylinder((0, 0, 0), 0.38, 0.1, DARK, segments=16)
    p.cylinder((0, 0, 0.1), 0.35, 0.85, TEAL, radius_top=0.38, segments=16)
    p.cylinder((0, 0, 0.95), 0.41, 0.06, TRIM, segments=16)
    p.dome((0, 0, 1.01), 0.38, TRIM, squash=0.6, segments=16)
    p.box((-0.14, -0.4, 0.78), (0.14, -0.3, 0.88), DARK)       # throw-in slot
    p.sphere((0, -0.3, 1.12), 0.07, GLOW_CYAN, "glow", segments=10)
    p.cylinder((0.12, 0, 1.2), 0.02, 0.35, METAL, segments=6)
    p.sphere((0.12, 0, 1.57), 0.04, GLOW_PINK, "glow", segments=8)
    return p


def bench_float():
    """A bench on one stalk, so it looks like it floats; a glow strip under the seat."""
    p = Part("bench_float", "prop")
    p.cylinder((0, 0, 0), 0.3, 0.05, METAL, segments=16)
    p.cylinder((0, 0, 0.05), 0.08, 0.35, METAL, segments=10)
    p.rounded_box((-0.9, -0.25, 0.4), (0.9, 0.25, 0.48), 0.2, RUST, segments=4)
    p.box((-0.75, -0.18, 0.37), (0.75, 0.18, 0.4), GLOW_PINK, "glow")
    back = [(0.2, 0.48), (0.28, 0.48), (0.42, 1.0), (0.32, 1.02)]
    p.prism(back, -0.9, 0.9, RUST, plane="YZ")
    return p


def vending_tube():
    """A vending machine with a rounded top and a glowing window full of nothing in particular."""
    p = Part("vending_tube", "prop")
    p.rounded_box((-0.5, -0.4, 0), (0.5, 0.4, 1.8), 0.25, MUSTARD, segments=4)
    p.dome((0, 0, 1.8), 0.45, MUSTARD, squash=0.5, segments=16)
    p.box((-0.38, -0.43, 0.75), (0.2, -0.38, 1.6), GLOW_CYAN, "glow")
    p.box((0.26, -0.43, 1.0), (0.4, -0.38, 1.4), DARK)          # keypad
    p.box((-0.3, -0.45, 0.2), (0.3, -0.38, 0.45), DARK)         # tray
    p.sphere((0, 0, 2.12), 0.08, GLOW_PINK, "glow", segments=10)
    return p


def crate_stack():
    """Three cargo crates, the top one askew."""
    p = Part("crate_stack", "prop")
    for (x, y, z, s, c) in ((-0.45, 0, 0, 0.8, TEAL), (0.45, 0.05, 0, 0.8, PLUM), (0.0, 0.0, 0.8, 0.7, RUST)):
        h = s / 2
        p.box((x - h, y - h, z), (x + h, y + h, z + s), c)
        p.box((x - h - 0.02, y - h - 0.02, z + s * 0.45), (x + h + 0.02, y + h + 0.02, z + s * 0.55), TRIM)
    return p


def planter_blob():
    """A planter with an alien plant: fat stalks and glowing pods."""
    p = Part("planter_blob", "prop")
    p.rounded_box((-0.6, -0.6, 0), (0.6, 0.6, 0.55), 0.2, TRIM, segments=4)
    p.cylinder((0, 0, 0.55), 0.5, 0.05, colour("#4a3a2e"), segments=16)
    for (x, y, h, r, c) in ((0.0, 0.0, 1.4, 0.32, GLOW_PINK), (0.25, 0.15, 0.9, 0.22, GLOW_CYAN),
                            (-0.25, -0.1, 1.1, 0.25, GLOW_PINK)):
        p.cylinder((x, y, 0.6), 0.07, h, colour("#5f8f4a"), radius_top=0.04, segments=8)
        p.sphere((x, y, 0.6 + h + r * 0.6), r, colour("#7fc46a"), squash=0.8, segments=12)
        p.sphere((x, y - r * 0.8, 0.6 + h + r * 0.6), r * 0.35, c, "glow", segments=8)
    return p


def lane_pylon():
    """The hover lanes' power pole: a tapered mast, a cross arm with glowing insulators, a ring on top.
    Cables between pylons come with the placement."""
    p = Part("lane_pylon", "prop")
    p.cylinder((0, 0, 0), 0.35, 0.3, DARK, segments=12)
    p.cylinder((0, 0, 0.3), 0.18, 7.2, METAL, radius_top=0.1, segments=12)
    p.box((-1.4, -0.08, 6.4), (1.4, 0.08, 6.6), METAL)
    for x in (-1.3, 1.3):
        p.cylinder((x, 0, 6.6), 0.06, 0.25, TRIM, segments=8)
        p.sphere((x, 0, 6.95), 0.12, GLOW_CYAN, "glow", segments=10)
    p.torus((0, 0, 7.3), 0.35, 0.05, GLOW_PINK, "glow", segments=20, sides=6)
    p.sphere((0, 0, 7.55), 0.1, TRIM, segments=10)
    return p


def roof_ac():
    """A roof air unit with a big fan on top; roofs get several."""
    p = Part("roof_ac", "prop")
    p.box((-0.8, -0.55, 0), (0.8, 0.55, 0.95), METAL)
    for k in range(5):
        x = -0.6 + k * 0.3
        p.box((x - 0.04, -0.58, 0.15), (x + 0.04, -0.55, 0.8), DARK)
    p.cylinder((0, 0, 0.95), 0.45, 0.06, DARK, segments=16)
    p.torus((0, 0, 1.01), 0.45, 0.04, TRIM, segments=20, sides=6)
    return p


def roof_dish():
    """A dish on a mast, listening to something far away."""
    p = Part("roof_dish", "prop")
    p.box((-0.4, -0.4, 0), (0.4, 0.4, 0.15), DARK)
    p.cylinder((0, 0, 0.15), 0.07, 1.6, METAL, segments=8)
    p.cylinder((0, 0.1, 1.75), 0.18, 0.45, TRIM, radius_top=1.0, segments=20, axis="Y")
    p.cylinder((0, -0.35, 1.75), 0.03, 0.6, METAL, segments=6, axis="Y")
    p.sphere((0, -0.95, 1.75), 0.08, GLOW_CYAN, "glow", segments=8)
    return p


def market_stall():
    """A stall with a striped canopy and a counter of strange produce."""
    p = Part("market_stall", "prop")
    for x in (-1.4, 1.4):
        for y in (-0.8, 0.8):
            p.cylinder((x, y, 0), 0.05, 2.4 if y < 0 else 2.0, METAL, segments=8)
    p.box((-1.5, -0.9, 0.0), (1.5, -0.3, 1.0), TEAL)
    p.box((-1.55, -0.95, 1.0), (1.55, -0.25, 1.08), TRIM)
    stripes = 6
    for k in range(stripes):
        x0 = -1.6 + 3.2 * k / stripes
        p.prism([(-1.2, 2.45), (1.0, 2.0), (1.0, 2.1), (-1.2, 2.55)], x0, x0 + 3.2 / stripes,
                RUST if k % 2 else TRIM, plane="YZ")
    for k, (x, c) in enumerate(((-1.1, GLOW_PINK), (-0.6, MUSTARD), (-0.1, colour("#7fc46a")), (0.4, PLUM),
                                (0.9, GLOW_CYAN))):
        p.sphere((x, -0.6, 1.2), 0.17 + 0.03 * (k % 2), c, "glow" if c in (GLOW_PINK, GLOW_CYAN) else "paint",
                 segments=10)
    p.box((-0.8, -1.0, 2.1), (0.8, -0.94, 2.4), DARK)
    p.box((-0.7, -1.02, 2.15), (0.7, -1.0, 2.35), GLOW_CYAN, "glow")
    return p


def fence_panel():
    """A 4 m fence segment: posts with ball tops, two rails, a glowing strip. Placed end to end."""
    p = Part("fence_panel", "prop")
    for x in (-1.88, 1.88):
        p.box((x - 0.06, -0.06, 0), (x + 0.06, 0.06, 1.3), METAL)
        p.sphere((x, 0, 1.38), 0.1, TRIM, segments=10)
    for z in (0.35, 1.05):
        p.box((-1.95, -0.03, z), (1.95, 0.03, z + 0.08), METAL)
    p.box((-1.95, -0.02, 0.43), (1.95, 0.02, 1.05), colour("#5b6478"))
    p.box((-1.95, -0.04, 0.7), (1.95, 0.04, 0.76), GLOW_CYAN, "glow")
    return p


def sign_post():
    """A post with arrow signs pointing three ways, none of them helpful."""
    p = Part("sign_post", "prop")
    p.cylinder((0, 0, 0), 0.08, 3.2, METAL, segments=8)
    p.sphere((0, 0, 3.25), 0.12, GLOW_PINK, "glow", segments=10)
    for z, turn, c in ((2.7, 0, TEAL), (2.25, 110, RUST), (1.8, 230, MUSTARD)):
        a = math.radians(turn)
        # A thin arrow board in the front view, turned about the post.
        arrow = [(0.1, -0.2), (1.0, -0.2), (1.3, 0.0), (1.0, 0.2), (0.1, 0.2)]
        with p.placed(Matrix.Rotation(a, 4, "Z")):
            p.prism([(u, z + v) for u, v in arrow], -0.03, 0.03, c, plane="XZ")
            p.box((0.3, -0.035, z - 0.05), (0.9, -0.03, z + 0.05), TRIM)
    return p


def billboard():
    """A billboard on two legs, a glowing face, lamps along the top."""
    p = Part("billboard", "prop")
    for x in (-2.0, 2.0):
        p.box((x - 0.12, -0.12, 0), (x + 0.12, 0.12, 3.0), METAL)
    p.box((-3.2, -0.15, 3.0), (3.2, 0.15, 6.2), DARK)
    p.box((-3.0, -0.2, 3.2), (3.0, -0.15, 6.0), GLOW_PINK, "glow")
    p.box((-1.6, -0.22, 3.6), (1.6, -0.2, 5.6), MUSTARD)
    p.cylinder((0, -0.22, 4.6), 0.7, 0.05, TEAL, segments=20, axis="Y")
    for x in (-2.4, -0.8, 0.8, 2.4):
        p.box((x - 0.04, -0.6, 6.2), (x + 0.04, 0.0, 6.28), METAL)
        p.sphere((x, -0.6, 6.15), 0.1, LIT_BULB, "glow", segments=8)
    return p


def booth_tele():
    """A booth that is either a phone or a teleporter, depending on who you ask."""
    p = Part("booth_tele", "prop")
    p.cylinder((0, 0, 0), 0.75, 0.15, DARK, segments=20)
    p.torus((0, 0, 0.15), 0.72, 0.05, GLOW_CYAN, "glow", segments=20, sides=6)
    p.rounded_box((-0.6, -0.6, 0.15), (0.6, 0.6, 2.4), 0.3, PLUM, segments=4)
    p.box((-0.42, -0.63, 0.4), (0.42, -0.6, 2.1), GLASS, "glass")
    p.dome((0, 0, 2.4), 0.6, PLUM, squash=0.6, segments=16)
    p.sphere((0, 0, 2.85), 0.15, GLOW_PINK, "glow", segments=10)
    p.box((-0.4, -0.65, 2.15), (0.4, -0.6, 2.32), GLOW_CYAN, "glow")
    return p


def mail_tube():
    """A pneumatic post: a hatch on a stand, a glass tube up into the sky with a capsule in it."""
    p = Part("mail_tube", "prop")
    p.box((-0.3, -0.25, 0), (0.3, 0.25, 1.1), RUST)
    p.box((-0.2, -0.28, 0.6), (0.2, -0.25, 0.95), TRIM)
    p.cylinder((0, 0.05, 1.1), 0.16, 3.4, GLASS, "glass", segments=12)
    p.sphere((0, 0.05, 2.6), 0.12, MUSTARD, squash=1.8, segments=10)
    p.torus((0, 0.05, 4.5), 0.18, 0.04, METAL, segments=12, sides=6)
    p.sphere((0, 0.05, 4.6), 0.14, GLOW_CYAN, "glow", segments=10)
    return p


def lamp_arc():
    """A street lamp with a swan-neck arm and a hanging glow globe."""
    p = Part("lamp_arc", "prop")
    p.cylinder((0, 0, 0), 0.22, 0.3, DARK, segments=12)
    p.cylinder((0, 0, 0.3), 0.08, 4.7, TRIM, radius_top=0.06, segments=10)
    # Swan neck: up from the mast top, over, and down to where the globe hangs.
    arm = [(-0.8 * (1 - math.cos(math.radians(a))), 4.6 + 0.5 * math.sin(math.radians(a))) for a in range(0, 181, 15)]
    outer = [(x, z + 0.06) for x, z in arm]
    inner = [(x, z - 0.06) for x, z in arm[::-1]]
    p.prism(outer + inner, -0.05, 0.05, TRIM, plane="XZ")
    p.cylinder((-1.6, 0, 4.2), 0.03, 0.35, METAL, segments=6)
    p.sphere((-1.6, 0, 4.0), 0.25, LIT_BULB, "glow", segments=12)
    return p


def robot_sweeper():
    """A street-sweeping robot on a hover disc, one eye, one broom, no opinions."""
    p = Part("robot_sweeper", "prop")
    p.cylinder((0, 0, 0.0), 0.45, 0.1, GLOW_CYAN, "glow", segments=16)
    p.cylinder((0, 0, 0.1), 0.5, 0.1, METAL, segments=16)
    p.cylinder((0, 0, 0.2), 0.35, 0.7, MUSTARD, radius_top=0.3, segments=16)
    p.dome((0, 0, 0.9), 0.3, TRIM, segments=16)
    p.sphere((0, -0.27, 1.02), 0.08, GLOW_PINK, "glow", segments=8)
    p.cylinder((0.33, -0.05, 0.6), 0.03, 0.6, METAL, segments=6, axis="X")
    p.box((0.88, -0.3, 0.1), (0.98, 0.2, 0.62), TEAL)
    p.box((0.86, -0.32, 0.1), (1.0, 0.22, 0.22), MUSTARD)
    return p


def npc_marker():
    """Placeholder for a quest giver: a plain figure with a glowing exclamation mark over its head.
    Stands where the game will put the character."""
    p = Part("npc_marker", "prop")
    p.cylinder((0, 0, 0), 0.3, 1.2, PLUM, radius_top=0.25, segments=12)
    p.sphere((0, 0, 1.45), 0.25, colour("#e9d7c4"), segments=12)
    # A hologram: a thin beam from the head carries the mark.
    p.cylinder((0, 0, 1.65), 0.015, 0.5, MUSTARD, "glow", segments=6)
    p.sphere((0, 0, 1.95), 0.06, MUSTARD, "glow", segments=8)
    p.box((-0.05, -0.05, 2.1), (0.05, 0.05, 2.6), MUSTARD, "glow")
    p.torus((0, 0, 0.05), 0.5, 0.04, MUSTARD, "glow", segments=20, sides=4)
    return p


MODELS = {
    "bin_bot": (bin_bot, (-0.45, -0.45, 0.45, 0.45)),
    "bench_float": (bench_float, (-1.0, -0.4, 1.0, 0.5)),
    "vending_tube": (vending_tube, (-0.55, -0.5, 0.55, 0.45)),
    "crate_stack": (crate_stack, (-0.9, -0.5, 0.9, 0.5)),
    "planter_blob": (planter_blob, (-0.65, -0.65, 0.65, 0.65)),
    "lane_pylon": (lane_pylon, (-1.5, -0.4, 1.5, 0.4)),
    "roof_ac": (roof_ac, (-0.85, -0.65, 0.85, 0.6)),
    "roof_dish": (roof_dish, (-1.05, -1.1, 1.05, 0.5)),
    "market_stall": (market_stall, (-1.65, -1.25, 1.65, 1.05)),
    "fence_panel": (fence_panel, (-2.0, -0.1, 2.0, 0.1)),
    "sign_post": (sign_post, (-1.4, -1.4, 1.4, 1.4)),
    "billboard": (billboard, (-3.25, -0.7, 3.25, 0.2)),
    "booth_tele": (booth_tele, (-0.8, -0.8, 0.8, 0.8)),
    "mail_tube": (mail_tube, (-0.35, -0.3, 0.35, 0.3)),
    "lamp_arc": (lamp_arc, (-1.95, -0.3, 0.3, 0.3)),
    "robot_sweeper": (robot_sweeper, (-0.55, -0.55, 1.05, 0.55)),
    "npc_marker": (npc_marker, (-0.55, -0.55, 0.55, 0.55)),
}

kit.run(MODELS, "content/city")
