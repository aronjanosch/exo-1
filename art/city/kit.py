"""City kit: shared parts, checks, export and review renders for the city scripts.

Every city model is one script in this folder that builds into a `Part` and calls `run()`.
Blender units are metres, up is +Z (glTF +Y). A building stands on its origin, the street
front faces -Y. Measures and look: `art/city/BRIEF.md`.

The same script runs headless (export, report, renders) or inside a live Blender over the
MCP (builds into the open scene, nothing written); see `art/AGENTS.md`.
"""

import argparse
import json
import math
import os
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

# Starting values from BRIEF.md (guide values, not rules).
CELL = 0.5            # facade grid: windows, doors and signs snap to it
BAY = 4.0             # facade bay width
GROUND_STOREY = 4.5   # shop floor
STOREY = 3.5          # upper floors
TILE = 10.0           # road tile edge
SHARP_ANGLE = math.radians(40)
TRI_GUIDE = {"building": 10_000, "road": 1_000, "prop": 1_000, "vehicle": 3_000}

# Materials: vertex colours carry the paint, the material only says how it shines.
MATERIALS = {
    "paint": {"rgb": (1, 1, 1), "roughness": 0.75},
    "glass": {"rgb": (1, 1, 1), "roughness": 0.15},
    "glow": {"rgb": (1, 1, 1), "roughness": 0.5, "emit": 1.5},
}


def colour(hexstr):
    """'#rrggbb' -> linear rgb, so palette values read like a paint chip."""
    h = hexstr.lstrip("#")
    srgb = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return tuple(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in srgb)


# ---------------------------------------------------------------- building parts

class Part:
    """Collects geometry in one bmesh; each face carries a material slot and a vertex colour."""

    def __init__(self, name, kind="building"):
        self.name = name
        self.kind = kind
        self.bm = bmesh.new()
        self.col = self.bm.loops.layers.color.new("Col")
        self.anchors = []   # (name, location, extras): doors, signs, spawn points for the game
        self.slots = list(MATERIALS)

    def _paint(self, faces, rgb, mat):
        idx = self.slots.index(mat)
        for f in faces:
            f.material_index = idx
            for loop in f.loops:
                loop[self.col] = (*rgb, 1.0)

    def box(self, lo, hi, rgb, mat="paint"):
        """Axis-aligned box from corner lo to corner hi."""
        lo, hi = Vector(lo), Vector(hi)
        size = hi - lo
        m = Matrix.Translation((lo + hi) / 2) @ Matrix.Diagonal((*size, 1))
        res = bmesh.ops.create_cube(self.bm, size=1.0, matrix=m)
        self._paint({f for v in res["verts"] for f in v.link_faces}, rgb, mat)

    def cylinder(self, at, radius, height, rgb, mat="paint", segments=12, radius_top=None, axis="Z"):
        rot = {"Z": Matrix(), "X": Matrix.Rotation(math.pi / 2, 4, "Y"), "Y": Matrix.Rotation(math.pi / 2, 4, "X")}[axis]
        m = Matrix.Translation(Vector(at)) @ rot @ Matrix.Translation((0, 0, height / 2))
        res = bmesh.ops.create_cone(self.bm, cap_ends=True, segments=segments, radius1=radius,
                                    radius2=radius if radius_top is None else radius_top, depth=height, matrix=m)
        self._paint({f for v in res["verts"] for f in v.link_faces}, rgb, mat)

    def dome(self, at, radius, rgb, mat="paint", squash=1.0, segments=16):
        """Half sphere sitting on `at`, open at the bottom."""
        m = Matrix.Translation(Vector(at)) @ Matrix.Diagonal((1, 1, squash, 1))
        res = bmesh.ops.create_uvsphere(self.bm, u_segments=segments, v_segments=segments // 2,
                                        radius=radius, matrix=m)
        below = [v for v in res["verts"] if v.co.z < at[2] - 1e-4]
        bmesh.ops.delete(self.bm, geom=below, context="VERTS")
        self._paint({f for v in res["verts"] if v.is_valid for f in v.link_faces}, rgb, mat)

    def sphere(self, at, radius, rgb, mat="paint", squash=1.0, segments=16):
        m = Matrix.Translation(Vector(at)) @ Matrix.Diagonal((1, 1, squash, 1))
        res = bmesh.ops.create_uvsphere(self.bm, u_segments=segments, v_segments=segments // 2,
                                        radius=radius, matrix=m)
        self._paint({f for v in res["verts"] for f in v.link_faces}, rgb, mat)

    def torus(self, at, major, minor, rgb, mat="paint", segments=32, sides=8):
        """A ring lying flat around `at`: glow rings, railings, bumpers."""
        c = Vector(at)
        grid = []
        for i in range(segments):
            a = 2 * math.pi * i / segments
            row = []
            for j in range(sides):
                b = 2 * math.pi * j / sides
                r = major + minor * math.cos(b)
                row.append(self.bm.verts.new(c + Vector((r * math.cos(a), r * math.sin(a), minor * math.sin(b)))))
            grid.append(row)
        faces = []
        for i in range(segments):
            n = (i + 1) % segments
            for j in range(sides):
                k = (j + 1) % sides
                faces.append(self.bm.faces.new((grid[i][j], grid[n][j], grid[n][k], grid[i][k])))
        self._paint(faces, rgb, mat)

    def prism(self, outline, a, b, rgb, mat="paint", plane="XY"):
        """Extrude a 2D outline from a to b. XY: outline in plan, a/b are heights.
        XZ: outline in the front view, a/b are y. YZ: outline in the side view, a/b are x."""
        def at(u, v, w):
            return {"XY": (u, v, w), "XZ": (u, w, v), "YZ": (w, u, v)}[plane]
        bot = [self.bm.verts.new(at(u, v, a)) for u, v in outline]
        top = [self.bm.verts.new(at(u, v, b)) for u, v in outline]
        faces = [self.bm.faces.new(bot[::-1]), self.bm.faces.new(top)]
        for i in range(len(outline)):
            j = (i + 1) % len(outline)
            faces.append(self.bm.faces.new((bot[i], bot[j], top[j], top[i])))
        self._paint(faces, rgb, mat)

    def rounded_box(self, lo, hi, radius, rgb, mat="paint", segments=6):
        """Box with rounded vertical edges: the basic futuristic block."""
        r = radius
        corners = [(hi[0] - r, hi[1] - r, 0), (lo[0] + r, hi[1] - r, 90), (lo[0] + r, lo[1] + r, 180), (hi[0] - r, lo[1] + r, 270)]
        pts = []
        for cx, cy, start in corners:
            for k in range(segments + 1):
                ang = math.radians(start + 90 * k / segments)
                pts.append((cx + r * math.cos(ang), cy + r * math.sin(ang)))
        self.prism(pts, lo[2], hi[2], rgb, mat)

    def anchor(self, name, at, **extras):
        """An empty in the export; the game reads its custom properties (glTF extras)."""
        self.anchors.append((name, Vector(at), extras))

    # -------------------------------------------------- facade helpers

    def window(self, x, z, w, h, front_y, frame_rgb, glass_rgb, lit=False, depth=0.12):
        """A framed pane on a wall face at y = front_y (the wall faces -Y). x is the centre, z the sill."""
        self.box((x - w / 2 - 0.1, front_y - depth, z - 0.1), (x + w / 2 + 0.1, front_y, z + h + 0.1), frame_rgb)
        self.box((x - w / 2, front_y - depth - 0.02, z), (x + w / 2, front_y - 0.02, z + h), glass_rgb,
                 "glow" if lit else "glass")

    def door(self, name, x, front_y, frame_rgb, door_rgb, w=1.5, h=2.5, **extras):
        """Door panel plus an anchor 1 m in front of it, where a walker stands to use it."""
        self.box((x - w / 2 - 0.15, front_y - 0.15, 0), (x + w / 2 + 0.15, front_y, h + 0.15), frame_rgb)
        self.box((x - w / 2, front_y - 0.17, 0), (x + w / 2, front_y - 0.02, h), door_rgb)
        self.anchor(name, (x, front_y - 1.0, 0), kind="door", **extras)

    def sign(self, x, z, w, h, front_y, board_rgb, glow_rgb, depth=0.2):
        """A sign board with a glowing face; the lettering comes later as a decal or texture."""
        self.box((x - w / 2, front_y - depth, z), (x + w / 2, front_y, z + h), board_rgb)
        self.box((x - w / 2 + 0.15, front_y - depth - 0.03, z + 0.15), (x + w / 2 - 0.15, front_y - depth, z + h - 0.15),
                 glow_rgb, "glow")

    # -------------------------------------------------- finish

    def build(self, collection):
        mesh = bpy.data.meshes.new(self.name)
        bmesh.ops.recalc_face_normals(self.bm, faces=self.bm.faces)
        self.bm.normal_update()
        self.bm.to_mesh(mesh)
        self.bm.free()
        for key in self.slots:
            mesh.materials.append(material(key))
        obj = bpy.data.objects.new(self.name, mesh)
        collection.objects.link(obj)
        for p in mesh.polygons:
            p.use_smooth = True
        mod = obj.modifiers.new("hard edges", "EDGE_SPLIT")
        mod.split_angle = SHARP_ANGLE
        for name, at, extras in self.anchors:
            e = bpy.data.objects.new(name, None)
            e.empty_display_type = "SINGLE_ARROW"
            e.location = at
            e.parent = obj
            for k, v in extras.items():
                e[k] = v
            collection.objects.link(e)
        return obj


def material(key):
    spec = MATERIALS[key]
    m = bpy.data.materials.get(key)
    if m:
        return m
    m = bpy.data.materials.new(key)
    m["exo_kit"] = True
    nodes, links = m.node_tree.nodes, m.node_tree.links
    bsdf = nodes["Principled BSDF"]
    attr = nodes.new("ShaderNodeVertexColor")
    attr.layer_name = "Col"
    links.new(attr.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = spec["roughness"]
    if "emit" in spec:
        links.new(attr.outputs["Color"], bsdf.inputs["Emission Color"])
        bsdf.inputs["Emission Strength"].default_value = spec["emit"]
    return m


# ---------------------------------------------------------------- checks

def check(obj, part, footprint):
    """Measured facts plus a list of problems. footprint = (x0, y0, x1, y1) the model must stay inside."""
    dg = bpy.context.evaluated_depsgraph_get()
    ev = obj.evaluated_get(dg).data
    tris = sum(len(p.vertices) - 2 for p in ev.polygons)
    xs = [v.co.x for v in ev.vertices]
    ys = [v.co.y for v in ev.vertices]
    zs = [v.co.z for v in ev.vertices]
    bounds = [round(min(xs), 3), round(min(ys), 3), round(min(zs), 3), round(max(xs), 3), round(max(ys), 3), round(max(zs), 3)]
    problems = []
    guide = TRI_GUIDE[part.kind]
    if tris > guide:
        problems.append(f"{tris} triangles, guide value {guide} (allowed to break, say why)")
    if part.kind != "road" and min(zs) < -0.01:
        problems.append(f"geometry below the ground: {min(zs):.2f} m")
    x0, y0, x1, y1 = footprint
    if min(xs) < x0 - 0.01 or max(xs) > x1 + 0.01 or min(ys) < y0 - 0.01 or max(ys) > y1 + 0.01:
        problems.append(f"outside the footprint {footprint}: bounds {bounds}")
    if part.kind == "building" and not any(e.get("kind") == "door" for _, _, e in part.anchors):
        problems.append("no door anchor")
    loose = loose_islands(ev)
    if loose:
        problems.append(f"{len(loose)} floating parts (not touching ground or another part), first boxes: {loose[:3]}")
    return {"name": obj.name, "triangles": tris, "bounds": bounds,
            "anchors": [{"name": n, "at": [round(c, 2) for c in at], **e} for n, at, e in part.anchors],
            "problems": problems}


def loose_islands(mesh):
    """Islands that neither touch the ground nor overlap another island's bounding box."""
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.verts.ensure_lookup_table()
    seen, boxes = set(), []
    for v in bm.verts:
        if v.index in seen:
            continue
        stack, island = [v], []
        seen.add(v.index)
        while stack:
            w = stack.pop()
            island.append(w.co.copy())
            for e in w.link_edges:
                o = e.other_vert(w)
                if o.index not in seen:
                    seen.add(o.index)
                    stack.append(o)
        lo = Vector((min(c.x for c in island), min(c.y for c in island), min(c.z for c in island)))
        hi = Vector((max(c.x for c in island), max(c.y for c in island), max(c.z for c in island)))
        boxes.append((lo, hi))
    bm.free()
    eps = 0.02

    def touch(a, b):
        return all(a[0][i] <= b[1][i] + eps and b[0][i] <= a[1][i] + eps for i in range(3))

    # Grounded islands spread support to everything they touch.
    supported = {i for i, (lo, _) in enumerate(boxes) if lo.z <= eps}
    grew = True
    while grew:
        grew = False
        for i, b in enumerate(boxes):
            if i not in supported and any(touch(b, boxes[j]) for j in supported):
                supported.add(i)
                grew = True
    return [[round(c, 2) for c in (*b[0], *b[1])] for i, b in enumerate(boxes) if i not in supported]


# ---------------------------------------------------------------- review renders

def stage(collection, night=False):
    scene = bpy.context.scene
    bpy.ops.mesh.primitive_plane_add(size=300, location=(0, 0, -0.005))
    ground = bpy.context.active_object
    for c in ground.users_collection:
        c.objects.unlink(ground)
    collection.objects.link(ground)
    gm = bpy.data.materials.new("Ground")
    gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.08, 0.07, 0.1, 1) if night else (0.36, 0.33, 0.38, 1)
    ground.data.materials.append(gm)
    sun = bpy.data.objects.new("Sun", bpy.data.lights.new("Sun", "SUN"))
    sun.rotation_euler = (math.radians(50), 0, math.radians(-35))
    sun.data.energy = 0.3 if night else 3.5
    collection.objects.link(sun)
    world = bpy.data.worlds.get("Sky") or bpy.data.worlds.new("Sky")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.005, 0.005, 0.02, 1) if night else (0.3, 0.26, 0.4, 1)
    scene.world = world
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 48
    scene.cycles.device = "GPU"
    scene.view_settings.view_transform = "AgX"


def shoot(collection, path, location, target, lens=35, size=(1600, 900)):
    scene = bpy.context.scene
    cam = bpy.data.objects.new("Cam", bpy.data.cameras.new("Cam"))
    collection.objects.link(cam)
    cam.location = location
    cam.rotation_euler = (Vector(target) - Vector(location)).to_track_quat("-Z", "Y").to_euler()
    cam.data.lens = lens
    scene.camera = cam
    scene.render.resolution_x, scene.render.resolution_y = size
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    bpy.data.objects.remove(cam)


def review(collection, obj, out):
    """Fixed views, the same for every model: three-quarter, street level, top, night."""
    lo = Vector(obj.bound_box[0])
    hi = Vector(obj.bound_box[6])
    c = (lo + hi) / 2
    r = max((hi - lo).length / 2, 3.0)
    stage(collection)
    # Far enough that the bounding sphere fits the 35 mm lens's vertical field (about 32 degrees).
    shoot(collection, f"{out}/{obj.name}_three_quarter.png", c + Vector((0.55, -0.75, 0.38)).normalized() * r * 3.9, c)
    # A walker on the far sidewalk: eye height 1.7 m, 9 m in front, wide lens.
    shoot(collection, f"{out}/{obj.name}_street.png", (c.x - 2.0, lo.y - 9.0, 1.7), (c.x, lo.y, max(2.5, c.z * 0.8)), lens=18)
    shoot(collection, f"{out}/{obj.name}_top.png", (c.x, c.y, hi.z + r * 3.0), c, lens=35, size=(900, 900))
    for o in [o for o in collection.objects if o.type == "LIGHT" or o.name.startswith("Plane")]:
        bpy.data.objects.remove(o)
    stage(collection, night=True)
    shoot(collection, f"{out}/{obj.name}_night.png", (c.x + r * 0.9, lo.y - r * 2.2, 1.7), (c.x, c.y, c.z), lens=20)


# ---------------------------------------------------------------- entry point

def fresh_collection(name="exo"):
    """Empty working collection; in a live Blender only this collection is cleared."""
    old = bpy.data.collections.get(name)
    if old:
        for o in list(old.objects):
            bpy.data.objects.remove(o, do_unlink=True)
        bpy.data.collections.remove(old)
    for block in (bpy.data.meshes, bpy.data.materials, bpy.data.cameras, bpy.data.lights):
        for b in list(block):
            if b.users == 0:
                block.remove(b)
    col = bpy.data.collections.new(name)
    bpy.context.scene.collection.children.link(col)
    return col


def run(models, out_default):
    """models: {id: (make() -> Part, footprint)}. Headless: export and report; live: build only."""
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=out_default, help="where the .glb files go")
    ap.add_argument("--renders", help="directory for the review renders")
    ap.add_argument("--only", help="comma-separated model ids")
    args = ap.parse_args(argv if bpy.app.background else [])
    only = set(args.only.split(",")) if args.only else None
    live = not bpy.app.background

    if live:
        col = fresh_collection()
        x = 0.0
        for mid, (make, footprint) in models.items():
            if only and mid not in only:
                continue
            part = make()
            obj = part.build(col)
            obj.location.x = x - footprint[0]
            x += footprint[2] - footprint[0] + 4.0
            print(json.dumps(check(obj, part, footprint)))
        return

    reports = []
    for mid, (make, footprint) in models.items():
        if only and mid not in only:
            continue
        bpy.ops.wm.read_factory_settings(use_empty=True)
        col = fresh_collection()
        part = make()
        obj = part.build(col)
        report = check(obj, part, footprint)
        reports.append(report)
        os.makedirs(args.out, exist_ok=True)
        path = f"{args.out}/{mid}.glb"
        for o in bpy.data.objects:
            o.select_set(o == obj or o.parent == obj)
        bpy.context.view_layer.objects.active = obj
        bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True, export_apply=True,
                                  export_extras=True, export_yup=True,
                                  # Only "Col", as COLOR_0. The 5.2 default adds a white COLOR_0 and
                                  # moves the paint to COLOR_1, which Bevy ignores.
                                  export_vertex_color="NAME", export_vertex_color_name="Col",
                                  export_all_vertex_colors=False)
        print(f"CITY {mid}: {report['triangles']} triangles -> {path}")
        for p in report["problems"]:
            print(f"  PROBLEM {mid}: {p}")
        if args.renders:
            os.makedirs(args.renders, exist_ok=True)
            review(col, obj, args.renders.rstrip("/"))
    print("REPORT " + json.dumps(reports))
