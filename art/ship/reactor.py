"""The engine room's antimatter reactor (#313) and its antimatter canister.

Source: player journal `transcripts/2026-10-17-reaktor.md` and WALKTHROUGH section 4 in the
concept repo, then the initiator's redesign (2026-10-17): bigger and futuristic, the engineer runs
injection, field converter and AUTO/MANUAL on one large screen; only the emergency and start-up
parts are physical (SCRAM under a cover, START lever, boot toggles, core eject under a cover) plus
the canister slots. Start-up has its own small stand on the left flank (initiator's idea). Node names, pivots, axes and travel: `reactor.md` next to this script.

The look: a "magnetic bottle". A glass column in the middle holds the reaction, a glowing spindle
whose waist is the annihilation point; three confinement rings float around it (the game may
spin them); four pylons carry the crown with the feed and conduits to the ceiling. The control
console stands in front of the column, the start-up stand on the left flank (-X), the canister
feed bay on the right (+X).

Frame: metres, origin at the floor under the column's axis, Blender Z up. The console faces -Y
here, which the exporter turns into +Z in the game (glTF): Blender (x, y, z) is glTF (x, z, -y).
Every moving part is its own object with its origin at its pivot; glows and texture quads
(screen_*, label_*) carry their own material so the game can light or texture them one by one.

Run headless (writes reactor.glb and canister_antimatter.glb, renders optional):
    blender -b --python-exit-code 1 -P art/ship/reactor.py -- --out content/ship [--renders DIR]
Live (open Blender over the MCP; builds into the collections `reactor`, `canister`, `reference`):
    import runpy; runpy.run_path("<repo>/art/ship/reactor.py", run_name="__main__")
"""

import argparse
import math
import os
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from mesh_checks import closed, face_islands, orientation_problems, signed_volume  # noqa: E402

TRIANGLE_BUDGET = 12000  # guide for the reactor with all parts; the canister has its own
CANISTER_BUDGET = 600

SEG = 24            # round parts of the reactor
TOP = 3.60          # height to the ceiling flanges; the column alone ends at 2.96

# Column (radius, z) values.
GLASS_R = 0.46
COLUMN = (1.22, 2.78)               # glass from, to
RINGS = (1.62, 2.38)                # confinement ring heights, either side of the waist
RING_R = (0.60, 0.84)               # inner, outer

# Pylons: four, at the diagonals.
PYLON_R = 1.06                      # axis distance of a pylon's centre
PYLON = (0.34, 0.30)                # tangential width, radial depth
PYLON_Z = (0.95, 2.98)

# Console in front (-Y): a profile in (y, z), extruded over x.
CONSOLE_X = 0.80
DESK = ((-1.85, 0.90), (-1.55, 1.02))      # front edge, back edge of the sloped desk
FACE = ((-1.55, 1.02), (-1.41, 1.78))      # bottom, top of the screen face (tilted back)
SCREEN = (1.10, 0.62)               # screen_main, the engineer's one screen
SCREEN_Z = 1.40
# The supply strip above the visor (canisters, battery, coolant): a slim second screen on the
# console's top edge, tilted back so it reads from the desk and still clears the core above.
SUPPLY = (1.10, 0.12)               # screen_supply, at the main screen's pixel density
SUPPLY_AT = (-1.36, 1.86)           # (y, z) of its centre
SUPPLY_RX = -30.0
START_DETENT = 35.0                 # degrees per detent, OFF -> PRIME -> RUN
BOOT_TOGGLES = 3

# Start-up stand on the left flank (-X), the mirror of the feed bay. Built like the console in a
# local frame facing -Y (x from, to; desk front and back in (y, z); backboard y front, y back, top),
# then turned by STAND_M so its front faces -X.
STAND_X = (-0.30, 0.30)
STAND_DESK = ((-1.75, 0.98), (-1.33, 1.10))
STAND_BACK = (-1.33, -1.19, 1.66)
STAND_M = Matrix.Rotation(math.radians(-90.0), 4, "Z")
PRIME_SEGMENTS = 5

# Core eject: in the front-left pylon (225°), niche width, height, depth.
EJECT_Z = 1.25
EJECT_NICHE = (0.18, 0.40, 0.12)

# Feed bay (+X): the two canister slots, axis along +X, centres (y, z).
BAY = (1.15, 1.62, 0.55, 1.45)      # x from, x to, half width, top
SLOTS = [(-0.25, 1.12), (0.25, 1.12)]
SLOT_DEPTH = 0.26                   # how far a canister sits inside the bay

# Canister: long axis along local +Z, origin at the end that enters the slot first.
CAN_R = 0.075
CAN_L = 0.42


def srgb(h):
    return tuple(((c / 255) / 12.92) if c / 255 <= 0.04045 else (((c / 255) + 0.055) / 1.055) ** 2.4
                 for c in bytes.fromhex(h))


# name: (hex colour, roughness, metallic, emission strength, alpha)
MATERIALS = {
    "reactor_hull": ("c3c6c0", 0.6, 0.15, 0.0, 1.0),
    "reactor_dark": ("2b3036", 0.7, 0.3, 0.0, 1.0),
    "reactor_steel": ("8e979c", 0.4, 0.7, 0.0, 1.0),
    "reactor_ring": ("3b4148", 0.35, 0.8, 0.0, 1.0),
    "rubber_black": ("16181a", 0.9, 0.0, 0.0, 1.0),
    "scram_red": ("c8231b", 0.5, 0.0, 0.0, 1.0),
    "hazard_yellow": ("e8b923", 0.6, 0.1, 0.0, 1.0),
    "lamp_off": ("2a3a40", 0.3, 0.0, 0.0, 1.0),
    "core_glow": ("78c8ff", 0.3, 0.0, 6.0, 1.0),
    "core_glass": ("a8d8f0", 0.05, 0.0, 0.0, 0.18),
    "ring_glow": ("6fc3ff", 0.3, 0.0, 3.0, 1.0),
    "trim_glow": ("4fa8e8", 0.3, 0.0, 2.0, 1.0),
    "canister_shell": ("c9ccc8", 0.45, 0.4, 0.0, 1.0),
    "canister_glow": ("7ad0ff", 0.3, 0.0, 3.0, 1.0),
    "quad_blank": ("d8d4c8", 0.8, 0.0, 0.0, 1.0),
    "screen_off": ("05080a", 0.2, 0.0, 0.0, 1.0),
    "figure": ("8e979c", 0.8, 0.0, 0.0, 1.0),
}


def material(name, base=None):
    """One material per name; `base` copies the look of a palette entry under a new name."""
    if name in bpy.data.materials:
        return bpy.data.materials[name]
    hexc, rough, metal, emit, alpha = MATERIALS[base or name]
    rgb = srgb(hexc)
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    if emit:
        bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Emission Strength"].default_value = emit
    if alpha < 1.0:
        bsdf.inputs["Alpha"].default_value = alpha
        m.surface_render_method = "BLENDED"
    m.diffuse_color = (*rgb, alpha)
    return m


# ---------------------------------------------------------------- mesh helpers (local coords)

AXES = {"Z": Matrix.Identity(4), "X": Matrix.Rotation(math.pi / 2, 4, "Y"),
        "Y": Matrix.Rotation(-math.pi / 2, 4, "X")}


def frame(location, rx=0.0, rz=0.0):
    """A mounting frame: local -Y is the surface's outward normal, local +Z its 'up'."""
    return (Matrix.Translation(location) @ Matrix.Rotation(math.radians(rz), 4, "Z")
            @ Matrix.Rotation(math.radians(rx), 4, "X"))


class Mesh:
    """Collects geometry for one object in its local frame, one material slot per name."""

    def __init__(self):
        self.bm = bmesh.new()
        self.slots = []

    def slot(self, mat):
        if mat not in self.slots:
            self.slots.append(mat)
        return self.slots.index(mat)

    def _place(self, verts, mat, xf):
        bmesh.ops.transform(self.bm, matrix=xf, verts=list(verts))
        i = self.slot(mat)
        faces = {f for v in verts for f in v.link_faces}
        for f in faces:
            f.material_index = i
        return faces

    def box(self, mat, size, centre=(0, 0, 0), m=None):
        res = bmesh.ops.create_cube(self.bm, size=1.0)
        xf = Matrix.Translation(centre) @ Matrix.Diagonal((*size, 1.0))
        return self._place(res["verts"], mat, (m or Matrix.Identity(4)) @ xf)

    def cyl(self, mat, r, depth, centre=(0, 0, 0), axis="Z", seg=12, r2=None, m=None):
        """A capped cylinder (or cone with r2) along a local axis."""
        res = bmesh.ops.create_cone(self.bm, cap_ends=True, segments=seg, radius1=r,
                                    radius2=r if r2 is None else r2, depth=depth)
        xf = Matrix.Translation(centre) @ AXES[axis]
        return self._place(res["verts"], mat, (m or Matrix.Identity(4)) @ xf)

    def revolve(self, mat, profile, seg=SEG, centre=(0, 0, 0), axis="Z", loop=False, m=None, phase=0.0):
        """Turn a (radius, height) profile about the local axis; open ends with r > 0 get caps."""
        rings = []
        for r, z in profile:
            if r < 1e-9:
                rings.append([self.bm.verts.new((0, 0, z))] * seg)
            else:
                rings.append([self.bm.verts.new((r * math.cos(phase + 2 * math.pi * i / seg),
                                                 r * math.sin(phase + 2 * math.pi * i / seg), z))
                              for i in range(seg)])
        pairs = list(zip(rings, rings[1:])) + ([(rings[-1], rings[0])] if loop else [])
        for a, b in pairs:
            for i in range(seg):
                j = (i + 1) % seg
                quad = [a[i], a[j], b[j], b[i]]
                uniq = [v for k, v in enumerate(quad) if v not in quad[:k]]
                if len(uniq) >= 3:
                    self.bm.faces.new(uniq)
        if not loop:
            for ring in (rings[0], rings[-1]):
                if len(set(ring)) > 1:
                    self.bm.faces.new(ring)
        verts = {v for ring in rings for v in ring}
        xf = Matrix.Translation(centre) @ AXES[axis]
        return self._place(verts, mat, (m or Matrix.Identity(4)) @ xf)

    def ring(self, mat, r_out, r_in, depth, centre=(0, 0, 0), axis="Z", seg=16, m=None):
        h = depth / 2
        return self.revolve(mat, [(r_in, -h), (r_out, -h), (r_out, h), (r_in, h)], seg, centre, axis,
                            loop=True, m=m)

    def extrude_x(self, mat, outline, x0, x1, m=None):
        """A prism from a (y, z) outline between two x positions."""
        a = [self.bm.verts.new((x0, y, z)) for y, z in outline]
        b = [self.bm.verts.new((x1, y, z)) for y, z in outline]
        self.bm.faces.new(a)
        self.bm.faces.new(list(reversed(b)))
        n = len(outline)
        for i in range(n):
            j = (i + 1) % n
            self.bm.faces.new((a[i], b[i], b[j], a[j]))
        return self._place(a + b, mat, m or Matrix.Identity(4))

    def quad(self, mat, w, h, m=None):
        """A flat quad in local XZ facing -Y, UVs 0..1 upright."""
        vs = [self.bm.verts.new(p) for p in
              ((-w / 2, 0, -h / 2), (w / 2, 0, -h / 2), (w / 2, 0, h / 2), (-w / 2, 0, h / 2))]
        f = self.bm.faces.new(vs)
        uv = self.bm.loops.layers.uv.verify()
        for loop, c in zip(f.loops, ((0, 0), (1, 0), (1, 1), (0, 1))):
            loop[uv].uv = c
        return self._place(vs, mat, m or Matrix.Identity(4))

    def finish(self, name, collection, xf=None, parent=None):
        bm = self.bm
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-6)
        # Generated parts come out with mixed winding: point every closed island outwards.
        bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
        for island in face_islands(bm):
            if closed(island) and signed_volume(island) < 0:
                bmesh.ops.reverse_faces(bm, faces=list(island))
        mesh = bpy.data.meshes.new(name)
        bm.to_mesh(mesh)
        bm.free()
        for mat in self.slots:
            mesh.materials.append(material(mat) if mat in MATERIALS else bpy.data.materials[mat])
        obj = bpy.data.objects.new(name, mesh)
        collection.objects.link(obj)
        # With a parent, xf is the transform relative to it.
        if parent is not None:
            obj.parent = parent
        if xf is not None:
            obj.matrix_basis = xf
        return obj


def quad_obj(name, coll, w, h, xf, base="quad_blank"):
    """A texture quad with its own material (named like the object), origin at its centre."""
    material(name, base)
    q = Mesh()
    q.quad(name, w, h)
    return q.finish(name, coll, xf)


def carve(main, build_part, build_cutter):
    """Boolean-cut a closed part and add the result to `main`; cut walls take the cutter's material."""
    scene = bpy.context.scene.collection
    part, cutter = Mesh(), Mesh()
    build_part(part)
    build_cutter(cutter)
    pobj, cobj = part.finish("_carve_part", scene), cutter.finish("_carve_cutter", scene)
    mod = pobj.modifiers.new("cut", "BOOLEAN")
    mod.operation, mod.solver, mod.object, mod.material_mode = "DIFFERENCE", "EXACT", cobj, "TRANSFER"
    mesh = bpy.data.meshes.new_from_object(pobj.evaluated_get(bpy.context.evaluated_depsgraph_get()))
    before = set(main.bm.faces)
    main.bm.from_mesh(mesh)
    for f in main.bm.faces:
        if f not in before:
            f.material_index = main.slot(mesh.materials[f.material_index].name)
    for obj in (pobj, cobj):
        data = obj.data
        bpy.data.objects.remove(obj)
        bpy.data.meshes.remove(data)
    bpy.data.meshes.remove(mesh)


def lerp2(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


def slope(a, b):
    """The rx of a frame on a surface running from a (front) to b (back) in (y, z)."""
    return -(90.0 - math.degrees(math.atan2(b[1] - a[1], b[0] - a[0])))


DESK_RX = slope(*DESK)          # about -68°
FACE_RX = slope(*FACE)          # about -10°
STAND_RX = slope(*STAND_DESK)   # about -74°


def face_point(z):
    """(y, z) on the screen face at height z."""
    (y0, z0), (y1, z1) = FACE
    return (y0 + (y1 - y0) * (z - z0) / (z1 - z0), z)


def pylon_frame(angle_deg, z):
    """A frame on a pylon's outer face; the pylon at angle_deg around the axis."""
    a = math.radians(angle_deg)
    rr = PYLON_R + PYLON[1] / 2
    return frame((rr * math.cos(a), rr * math.sin(a), z), rz=angle_deg + 90.0)


# ---------------------------------------------------------------- the reactor

def build_body(coll):
    b = Mesh()
    # Plinth, base drum with a glowing trim, the deck the column stands on.
    b.revolve("reactor_dark", [(1.48, 0.0), (1.48, 0.15)], seg=12, phase=math.pi / 12)
    b.revolve("reactor_hull", [(1.36, 0.15), (1.36, 0.84), (1.24, 0.96)], seg=12, phase=math.pi / 12)
    b.revolve("trim_glow", [(1.37, 0.80), (1.37, 0.83)], seg=12, phase=math.pi / 12)
    b.revolve("reactor_dark", [(1.12, 0.95), (1.12, 1.04)], seg=SEG)
    # Vents around the drum, away from the console and the feed bay.
    for deg in (60, 120, 150, 210, 330, 30):
        a = math.radians(deg)
        m = frame((1.36 * math.cos(a), 1.36 * math.sin(a), 0.48), rz=deg + 90.0)
        b.box("reactor_dark", (0.42, 0.04, 0.42), (0, 0, 0), m=m)
        for k in range(4):
            b.box("reactor_steel", (0.38, 0.05, 0.03), (0, 0, -0.135 + k * 0.09), m=m)
    # Injector housings below and above the glass, the crown and the conduits to the ceiling.
    b.revolve("reactor_steel", [(0.64, 1.04), (0.64, 1.14), (0.52, 1.24)])
    b.revolve("reactor_steel", [(0.52, 2.76), (0.64, 2.86), (0.64, 2.96)])
    b.revolve("reactor_hull", [(1.28, 2.96), (1.28, 3.12), (1.06, 3.28), (0.72, 3.36)], seg=12,
              phase=math.pi / 12)
    b.revolve("reactor_dark", [(0.34, 3.36), (0.34, TOP - 0.06), (0.44, TOP - 0.06), (0.44, TOP)])
    for deg in (0, 90, 180, 270):
        a = math.radians(deg)
        c = (0.86 * math.cos(a), 0.86 * math.sin(a))
        b.cyl("reactor_dark", 0.09, TOP - 3.20, (c[0], c[1], (TOP + 3.20) / 2), seg=10)
        b.cyl("reactor_steel", 0.13, 0.05, (c[0], c[1], TOP - 0.025), seg=10)

    # Four pylons carry the crown; the core eject niche is cut into the front-left one (225°).
    nw, nh, nd = EJECT_NICHE
    for deg in (45, 135, 225, 315):
        a = math.radians(deg)
        m = frame((PYLON_R * math.cos(a), PYLON_R * math.sin(a), 0), rz=deg + 90.0)

        def pylon(p, m=m):
            p.box("reactor_hull", (PYLON[0], PYLON[1], PYLON_Z[1] - PYLON_Z[0]),
                  (0, 0, sum(PYLON_Z) / 2), m=m)

        if deg == 225:
            f = pylon_frame(225, EJECT_Z)

            def niche(c, f=f):
                c.box("rubber_black", (nw, nd + 0.02, nh), (0, nd / 2 - 0.01, 0), m=f)

            carve(b, pylon, niche)
        else:
            pylon(b)
        # A dark spine down the pylon's inner face, toward the rings.
        b.box("reactor_dark", (0.12, 0.04, 1.6), (0, PYLON[1] / 2 + 0.01, 2.0), m=m)

    # Console: desk and screen face in one profile; the screen recess is cut into the face.
    (dy0, dz0), (dy1, dz1) = DESK
    (fy1, fz1) = FACE[1]
    outline = [(-1.20, 0.0), (-1.74, 0.0), (-1.74, 0.10), (-1.80, 0.10), (dy0, dz0), (dy1, dz1),
               (fy1, fz1), (-1.20, fz1)]
    sy, sz = face_point(SCREEN_Z)
    sf = frame((0, sy, sz), rx=FACE_RX)

    def console(p):
        p.extrude_x("reactor_hull", outline, -CONSOLE_X, CONSOLE_X)

    def recess(c):
        c.box("rubber_black", (SCREEN[0] + 0.06, 0.08, SCREEN[1] + 0.06), (0, 0.0, 0), m=sf)

    carve(b, console, recess)
    # The supply strip's housing on the console's top edge, with a foot down onto it.
    upf = frame((0, *SUPPLY_AT), rx=SUPPLY_RX)
    b.box("reactor_dark", (SUPPLY[0] + 0.08, 0.05, SUPPLY[1] + 0.05), (0, 0.025, 0), m=upf)
    b.box("reactor_dark", (SUPPLY[0] - 0.20, 0.10, 0.07), (0, -1.30, FACE[1][1] + 0.02))
    # A visor over the screen and a dark band under the desk's front edge.
    b.box("reactor_dark", (SCREEN[0] + 0.20, 0.14, 0.035), (0, -0.05, SCREEN[1] / 2 + 0.07), m=sf)
    b.box("reactor_dark", (2 * CONSOLE_X + 0.02, 0.05, 0.05), (0, dy0 + 0.01, dz0 - 0.03))
    # Start-up stand on the left flank: a small sloped cabinet with a backboard, no screen.
    (sy0, sz0), (sy1, sz1) = STAND_DESK
    b.extrude_x("reactor_hull", [(STAND_BACK[1], 0.0), (sy0 + 0.06, 0.0), (sy0 + 0.06, 0.10),
                                 (sy0, 0.10), (sy0, sz0), (sy1, sz1), (STAND_BACK[1], sz1)], *STAND_X,
                m=STAND_M)
    b.box("reactor_dark", (STAND_X[1] - STAND_X[0] + 0.02, 0.05, 0.05), (sum(STAND_X) / 2, sy0 + 0.01, sz0 - 0.03),
          m=STAND_M)
    by0, by1, btop = STAND_BACK
    b.box("reactor_dark", (0.24, by1 - by0, btop - sz1), (sum(STAND_X) / 2, (by0 + by1) / 2, (btop + sz1) / 2),
          m=STAND_M)

    # Feed bay (+X) with the slot holes; a transfer line from it into the lower injector.
    x0, x1, hw, top = BAY

    def bay(p):
        p.extrude_x("reactor_hull", [(-hw, 0.15), (hw, 0.15), (hw, top), (-hw, top)], x0, x1)
        p.box("reactor_hull", (x1 - x0 + 0.04, 2 * hw + 0.04, 0.06), ((x0 + x1) / 2, 0, top))

    def holes(c):
        for y, z in SLOTS:
            c.cyl("rubber_black", CAN_R + 0.008, SLOT_DEPTH + 0.04, (x1 - SLOT_DEPTH / 2 + 0.01, y, z),
                  axis="X", seg=12)

    carve(b, bay, holes)
    b.cyl("reactor_steel", 0.08, x0 - 0.50, ((x0 + 0.50) / 2, 0, 1.12), axis="X", seg=10)
    for y, z in SLOTS:
        b.ring("reactor_steel", CAN_R + 0.055, CAN_R + 0.03, 0.05, (x1 + 0.02, y, z), axis="X", seg=12)

    # Details: bolts round the crown, a band under its rim, kick plates and worn steel edges where
    # hands and boots meet the console and the stand, cable runs up the rear pylons.
    for k in range(12):
        a = 2 * math.pi * (k + 0.5) / 12
        b.cyl("reactor_steel", 0.028, 0.03, (1.18 * math.cos(a), 1.18 * math.sin(a), 3.13), seg=6)
    b.revolve("reactor_dark", [(1.30, 2.96), (1.30, 3.02)], seg=12, phase=math.pi / 12)
    b.box("reactor_dark", (2 * CONSOLE_X - 0.04, 0.02, 0.10), (0, -1.75, 0.15))
    b.box("reactor_steel", (2 * CONSOLE_X - 0.10, 0.03, 0.02), (0, DESK[0][0] - 0.005, DESK[0][1] + 0.005))
    b.box("reactor_dark", (STAND_X[1] - STAND_X[0] - 0.04, 0.02, 0.10), (0, STAND_DESK[0][0] + 0.06 - 0.01, 0.15),
          m=STAND_M)
    b.box("reactor_steel", (STAND_X[1] - STAND_X[0] - 0.08, 0.03, 0.02),
          (0, STAND_DESK[0][0] - 0.005, STAND_DESK[0][1] + 0.005), m=STAND_M)
    b.box("reactor_dark", (0.02, 2 * BAY[2] - 0.04, 0.10), (BAY[1] + 0.01, 0, 0.25))
    for deg in (45, 135):
        a = math.radians(deg)
        m = frame((PYLON_R * math.cos(a), PYLON_R * math.sin(a), 0), rz=deg + 90.0)
        for x in (-0.10, 0.10):
            b.cyl("reactor_dark", 0.03, PYLON_Z[1] - PYLON_Z[0], (x, -PYLON[1] / 2 - 0.03, sum(PYLON_Z) / 2),
                  seg=8, m=m)
            for z in (1.3, 2.0, 2.7):
                b.box("reactor_steel", (0.08, 0.04, 0.03), (x, -PYLON[1] / 2 - 0.03, z), m=m)
    # Screen bezel ears and the eject port round the column's foot (the core leaves through it).
    for x in (-1, 1):
        b.box("reactor_dark", (0.05, 0.06, SCREEN[1] + 0.10), (x * (SCREEN[0] / 2 + 0.05), -0.02, 0), m=sf)
    b.ring("hazard_yellow", 0.70, 0.66, 0.02, (0, 0, 1.045), seg=SEG)

    # Coolant lines out of the back into the floor.
    for x in (-0.45, 0.45):
        b.cyl("reactor_dark", 0.12, 0.60, (x, 1.50, 0.55), axis="Y", seg=10)
        b.box("reactor_dark", (0.28, 0.28, 0.28), (x, 1.80, 0.55))
        b.cyl("reactor_dark", 0.12, 0.45, (x, 1.80, 0.21), seg=10)
        b.cyl("reactor_steel", 0.16, 0.04, (x, 1.80, 0.02), seg=10)
    return b.finish("reactor_body", coll)


def build_core(coll):
    """The ejectable core: an empty at the column's foot, the glass and the glow under it."""
    core = bpy.data.objects.new("core", None)
    coll.objects.link(core)
    core.location = (0, 0, COLUMN[0])
    g = Mesh()
    z0, z1 = COLUMN
    mid = (z0 + z1) / 2
    g.revolve("core_glow", [(0.10, z0 + 0.02), (0.18, z0 + 0.30), (0.34, mid - 0.08), (0.34, mid + 0.08),
                            (0.18, z1 - 0.30), (0.10, z1 - 0.02)], seg=16, centre=(0, 0, -z0))
    g.finish("core_glow", coll, parent=core)
    s = Mesh()
    s.revolve("core_glass", [(GLASS_R, 0.0), (GLASS_R, z1 - z0)])
    s.finish("core_glass", coll, parent=core)
    return core


def build_rings(coll):
    r0, r1 = RING_R
    for i, z in enumerate(RINGS, start=1):
        m = Mesh()
        m.revolve("reactor_ring", [(r0, -0.04), (r1 - 0.03, -0.04), (r1, -0.015), (r1, 0.015),
                                   (r1 - 0.03, 0.04), (r0, 0.04)], loop=True)
        # Emitter blocks so the turn reads; their glow faces the column.
        for k in range(4):
            a = k * math.pi / 2 + i * math.pi / 8
            rot = Matrix.Rotation(a, 4, "Z")
            m.box("reactor_ring", (0.16, 0.10, 0.13), ((r0 + r1) / 2 + 0.02, 0, 0), m=rot)
            m.box("ring_glow", (0.03, 0.08, 0.09), (r0 - 0.005, 0, 0), m=rot)
        m.finish(f"ring_{i}", coll, Matrix.Translation((0, 0, z)))


def build_hollow_cover(m, w, h, d, t=0.012):
    """A shallow lid hanging down from its hinge (origin), open toward +Y (the surface)."""
    # Plate and lips overlap a little instead of sharing corners (no merged, nonmanifold edges).
    m.box("hazard_yellow", (w + 0.002, t, h + 0.002), (0, -d + t / 2, -h / 2))
    for x in (-w / 2 + t / 2, w / 2 - t / 2):
        m.box("hazard_yellow", (t, d, h), (x, -d / 2, -h / 2))
    for z in (-t / 2, -h + t / 2):
        m.box("hazard_yellow", (w - 2 * t - 0.002, d, t), (0, -d / 2, z))


def build_controls(coll):
    # The screen in its recess.
    sy, sz = face_point(SCREEN_Z)
    sf = frame((0, sy, sz), rx=FACE_RX)
    material("screen_main", "screen_off")
    quad_obj("screen_main", coll, *SCREEN, sf @ Matrix.Translation((0, 0.035, 0)), base="screen_off")
    quad_obj("screen_supply", coll, *SUPPLY, frame((0, *SUPPLY_AT), rx=SUPPLY_RX)
             @ Matrix.Translation((0, -0.001, 0)), base="screen_off")
    quad_obj("label_reactor", coll, 0.50, 0.07, frame((0, -1.7401, 0.55)))

    # Main desk: SCRAM under its cover, right of centre, in reach from the screen.
    f = frame((0.50, *lerp2(*DESK, 0.5)), rx=DESK_RX)
    base = Mesh()
    base.box("hazard_yellow", (0.20, 0.02, 0.20), (0, -0.01, 0), m=f)
    base.finish("scram_base", coll)
    btn = Mesh()
    btn.cyl("scram_red", 0.055, 0.04, (0, -0.02, 0), axis="Y", seg=12)
    btn.cyl("reactor_dark", 0.065, 0.012, (0, -0.006, 0), axis="Y", seg=12)
    btn.finish("button_scram", coll, f @ Matrix.Translation((0, -0.02, 0)))
    cov = Mesh()
    build_hollow_cover(cov, 0.18, 0.18, 0.07)
    cov.finish("cover_scram", coll, f @ Matrix.Translation((0, -0.02, 0.09)))
    quad_obj("label_scram", coll, 0.20, 0.05, f @ Matrix.Translation((0, -0.001, -0.14)))

    # Start-up stand: boot toggles at the front of its slope, the START lever behind them,
    # the prime column on the backboard fills segment by segment while the trap charges.
    px = sum(STAND_X) / 2
    for i in range(BOOT_TOGGLES):
        f = STAND_M @ frame((px + (i - (BOOT_TOGGLES - 1) / 2) * 0.15, *lerp2(*STAND_DESK, 0.22)), rx=STAND_RX)
        t = Mesh()
        t.box("reactor_dark", (0.06, 0.02, 0.08), (0, 0.0, 0))
        t.cyl("reactor_steel", 0.011, 0.09, (0, -0.045, 0), axis="Y", seg=8)
        t.cyl("rubber_black", 0.017, 0.03, (0, -0.09, 0), axis="Y", seg=8)
        # Rest pose OFF: tipped 20° toward the player about local X.
        t.finish(f"toggle_boot_{i + 1}", coll, f @ Matrix.Rotation(math.radians(20), 4, "X"))
    quad_obj("label_boot", coll, 0.46, 0.05,
             STAND_M @ frame((px, *lerp2(*STAND_DESK, 0.06)), rx=STAND_RX) @ Matrix.Translation((0, -0.001, 0)))
    f = STAND_M @ frame((px, *lerp2(*STAND_DESK, 0.62)), rx=STAND_RX)
    gate = Mesh()
    for x in (-0.034, 0.034):
        gate.box("reactor_dark", (0.032, 0.03, 0.32), (x, -0.015, 0), m=f)
    for k in (-1, 0, 1):
        gate.box("reactor_steel", (0.012, 0.034, 0.016), (0.055, -0.017, -k * 0.11), m=f)
    gate.finish("lever_start_gate", coll)
    lv = Mesh()
    # Arm out of the desk along local -Y, a ball grip at its end; turns about local X.
    lv.cyl("reactor_steel", 0.035, 0.05, (0, 0, 0), axis="X", seg=10)
    lv.box("reactor_steel", (0.03, 0.28, 0.03), (0, -0.14, 0))
    lv.cyl("rubber_black", 0.04, 0.07, (0, -0.30, 0), axis="Y", seg=10)
    # Rest pose OFF: leaning away from the player by one detent.
    lv.finish("lever_start", coll, f @ Matrix.Rotation(math.radians(-START_DETENT), 4, "X"))
    quad_obj("label_start", coll, 0.10, 0.30, f @ Matrix.Translation((0.12, -0.001, 0)))
    by = STAND_BACK[0] - 0.001
    for i in range(PRIME_SEGMENTS):
        name = f"lamp_prime_{i + 1}"
        material(name, "lamp_off")
        seg = Mesh()
        seg.box(name, (0.14, 0.02, 0.05), (0, 0, 0))
        seg.finish(name, coll, STAND_M @ Matrix.Translation((px, by - 0.01, 1.20 + i * 0.075)))
    quad_obj("label_startup", coll, 0.30, 0.07, STAND_M @ frame((px, by, STAND_BACK[2] - 0.06)))

    # Core eject in the front-left pylon: lever at the niche's foot, a lid hinged at its top.
    nw, nh, nd = EJECT_NICHE
    f = pylon_frame(225, EJECT_Z)
    ej = Mesh()
    ej.box("hazard_yellow", (0.03, 0.03, nh * 0.65), (0, 0, nh * 0.325))
    ej.box("rubber_black", (0.13, 0.035, 0.035), (0, 0, nh * 0.65))
    ej.finish("lever_eject", coll, f @ Matrix.Translation((0, nd / 2, -nh / 2 + 0.04)))
    ec = Mesh()
    build_hollow_cover(ec, nw + 0.06, nh + 0.06, 0.03)
    ec.finish("cover_eject", coll, f @ Matrix.Translation((0, 0, nh / 2 + 0.03)))
    quad_obj("label_eject", coll, 0.26, 0.07, f @ Matrix.Translation((0, -0.001, nh / 2 + 0.11)))

    # Feed bay slots: the slot's origin is a seated canister's origin, local +Z out of the bay.
    x1 = BAY[1]
    for i, (y, z) in enumerate(SLOTS, start=1):
        s = Mesh()
        s.cyl("reactor_steel", CAN_R, 0.012, (0, 0, -0.006), seg=12)  # the socket plate
        s.finish(f"slot_{i}", coll, Matrix.Translation((x1 - SLOT_DEPTH, y, z))
                 @ Matrix.Rotation(math.pi / 2, 4, "Y"))
        material(f"lamp_slot_{i}", "lamp_off")
        lamp = Mesh()
        lamp.ring(f"lamp_slot_{i}", CAN_R + 0.03, CAN_R + 0.01, 0.012, (0, 0, 0), axis="X", seg=12)
        lamp.finish(f"lamp_slot_{i}", coll, Matrix.Translation((x1 + 0.006, y, z)))
        quad_obj(f"label_slot_{i}", coll, 0.22, 0.06, frame((x1 + 0.001, y, z + 0.20), rz=90.0))


def build_canister(coll, xf=None, name="canister_antimatter"):
    """Heavy canister with its own magnetic trap: a capsule shell, coil bands, a glow window, a bail."""
    c = Mesh()
    r, ln = CAN_R, CAN_L
    c.revolve("canister_shell", [(0.0, 0.0), (r - 0.02, 0.0), (r, 0.025), (r, ln - 0.025), (r - 0.02, ln),
                                 (0.0, ln)], seg=12)
    for z in (0.06, ln - 0.06):
        c.revolve("reactor_dark", [(r + 0.008, z - 0.025), (r + 0.008, z + 0.025)], seg=12)
    c.revolve("canister_glow", [(r + 0.003, ln / 2 - 0.07), (r + 0.003, ln / 2 + 0.07)], seg=12)
    for x in (-0.045, 0.045):
        c.box("rubber_black", (0.02, 0.02, 0.07), (x, 0, ln + 0.03))
    c.box("rubber_black", (0.11, 0.026, 0.026), (0, 0, ln + 0.07))
    return c.finish(name, coll, xf)


# ---------------------------------------------------------------- scene, checks, output

def fresh_collection(name):
    old = bpy.data.collections.get(name)
    if old is not None:
        for obj in list(old.objects):
            bpy.data.objects.remove(obj, do_unlink=True)
        bpy.data.collections.remove(old)
    coll = bpy.data.collections.new(name)
    bpy.context.scene.collection.children.link(coll)
    return coll


def scale_figure(coll, location=(1.15, -2.45, 0.0)):
    """A 1.80 m stand-in crew member for the renders and the live scene, never exported."""
    f = Mesh()
    f.box("figure", (0.40, 0.24, 0.80), (0, 0, 1.10))
    f.box("figure", (0.30, 0.22, 0.75), (0, 0, 0.375))
    f.box("figure", (0.22, 0.24, 0.26), (0, 0, 1.67))
    return f.finish("scale_figure", coll, Matrix.Translation(location))


def build():
    # Live, a factory reset would also drop the MCP add-on: clear only our own collections.
    if bpy.app.background:
        bpy.ops.wm.read_factory_settings(use_empty=True)
    reactor = fresh_collection("reactor")
    canister = fresh_collection("canister")
    build_body(reactor)
    build_core(reactor)
    build_rings(reactor)
    build_controls(reactor)
    build_canister(canister)
    return reactor, canister


def dress(coll):
    """Review extras, never exported: a seated canister in slot 1, one on the floor, a figure."""
    slot = bpy.data.objects["slot_1"]
    build_canister(coll, slot.matrix_world.copy(), "preview_canister_seated")
    build_canister(coll, Matrix.Translation((1.9, -1.0, 0.0)), "preview_canister_floor")
    scale_figure(coll)


def triangles(coll):
    tris = 0
    for obj in coll.objects:
        if obj.type != "MESH":
            continue
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        tris += sum(len(f.verts) - 2 for f in bm.faces)
        for msg in orientation_problems(bm):
            print(f"PROBLEM {obj.name}: {msg}")
        bm.free()
    return tris


REQUIRED = (["reactor_body", "screen_main", "screen_supply", "button_scram", "cover_scram", "lever_start", "lever_eject",
             "cover_eject", "slot_1", "slot_2", "lamp_slot_1", "lamp_slot_2", "core", "core_glow",
             "core_glass"]
            + [f"ring_{i + 1}" for i in range(len(RINGS))]
            + [f"toggle_boot_{i + 1}" for i in range(BOOT_TOGGLES)]
            + [f"lamp_prime_{i + 1}" for i in range(PRIME_SEGMENTS)])


def report(reactor, canister):
    tris = triangles(reactor)
    print(f"reactor: {len(reactor.objects)} objects, {tris} triangles (guide {TRIANGLE_BUDGET})")
    if tris > TRIANGLE_BUDGET:
        print(f"PROBLEM reactor: over the triangle guide ({tris} > {TRIANGLE_BUDGET})")
    ctris = triangles(canister)
    print(f"canister_antimatter: {ctris} triangles (guide {CANISTER_BUDGET})")
    if ctris > CANISTER_BUDGET:
        print(f"PROBLEM canister_antimatter: over the triangle guide ({ctris} > {CANISTER_BUDGET})")
    names = set(reactor.objects.keys())
    for need in REQUIRED:
        if need not in names:
            print(f"PROBLEM reactor: node {need} missing")


def export(coll, path):
    for obj in bpy.data.objects:
        obj.select_set(obj.name in coll.objects.keys())
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True,
                              export_apply=True, export_yup=True, export_vertex_color="NONE")
    print("wrote", path)


def render(directory):
    os.makedirs(directory, exist_ok=True)
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x, scene.render.resolution_y = 900, 900
    scene.view_settings.view_transform = "AgX"
    world = bpy.data.worlds.new("w")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.05, 0.06, 0.07, 1.0)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 2.0
    scene.world = world
    for name, energy, rot in (("sun", 2.5, (35, 10, -35)), ("fill", 1.2, (60, 0, 150))):
        light = bpy.data.objects.new(name, bpy.data.lights.new(name, "SUN"))
        light.data.energy = energy
        light.rotation_euler = tuple(math.radians(a) for a in rot)
        scene.collection.objects.link(light)
    floor = Mesh()
    floor.box("reactor_dark", (10, 8, 0.02), (0, 0, -0.01))
    floor.finish("floor", scene.collection)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    scene.collection.objects.link(cam)
    scene.camera = cam
    views = {
        # name: (camera position, look-at, lens mm)
        "three_quarter": ((4.6, -5.6, 2.6), (0, 0, 1.6), 28),
        "front_eye": ((0.0, -4.6, 1.7), (0, 0, 1.75), 24),
        "side": ((6.2, 0.0, 1.7), (0, 0, 1.7), 28),
        "top": ((0.0, -0.01, 11.0), (0, 0, 0), 28),
        "startup_close": ((-3.6, -1.6, 1.9), (-1.4, 0.0, 1.2), 35),
    }
    for name, (pos, look, lens) in views.items():
        cam.location = pos
        cam.data.lens = lens
        cam.rotation_euler = (Vector(look) - Vector(pos)).to_track_quat("-Z", "Y").to_euler()
        scene.render.filepath = os.path.join(directory, f"reactor_{name}.png")
        bpy.ops.render.render(write_still=True)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="content/ship")
    ap.add_argument("--renders")
    args = ap.parse_args(argv if bpy.app.background else [])
    reactor, canister = build()
    report(reactor, canister)
    if bpy.app.background:
        os.makedirs(args.out, exist_ok=True)
        export(reactor, os.path.join(args.out, "reactor.glb"))
        export(canister, os.path.join(args.out, "canister_antimatter.glb"))
        if args.renders:
            dress(bpy.context.scene.collection)
            render(args.renders)
    else:
        dress(fresh_collection("reference"))


if __name__ == "__main__":
    main()
