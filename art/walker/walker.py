"""Walker figures: a painted human with four hairstyles and three aliens.

This script is the only source of the figures (concept repo DECISIONS.md,
model source). Blender units are metres; each figure stands on its origin,
faces -Y and fits roughly into the walker capsule (1.8 m, radius 0.35 m).

Run headless:
    blender -b -P art/walker/walker.py -- --renders OUT_DIR [--blend FILE]
"""

import argparse
import math
from pathlib import Path
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.geometry import intersect_ray_tri

SHARP_ANGLE = math.radians(50)
BODY_BUDGET = 2800
HAIR_BUDGET = 1100


# ---------------------------------------------------------------- materials

def material(name, rgb, roughness=0.6):
    m = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1)
    bsdf.inputs["Roughness"].default_value = roughness
    return m


def shared_materials():
    return {
        "eye": material("EyeWhite", (0.95, 0.95, 0.92), 0.3),
        "pupil": material("Pupil", (0.02, 0.02, 0.02), 0.3),
        "mouth": material("Mouth", (0.25, 0.04, 0.06)),
        "tongue": material("Tongue", (0.8, 0.32, 0.38)),
        "tooth": material("Tooth", (0.9, 0.88, 0.75)),
        "pants": material("Pants", (0.12, 0.13, 0.22)),
        "shoes": material("Shoes", (0.85, 0.85, 0.82)),
        "sole": material("Sole", (0.18, 0.18, 0.2)),
        "trim": material("Trim", (0.9, 0.88, 0.82)),
        "belt": material("Belt", (0.28, 0.16, 0.08)),
        "metal": material("Metal", (0.75, 0.75, 0.78), 0.3),
        "hair": material("Hair", (0.32, 0.2, 0.1)),
    }


# ---------------------------------------------------------------- body plan

class Figure:
    """Collects parts in local coordinates; build() joins them into one mesh."""

    def __init__(self, name, skin, suit):
        self.name = name
        self.parts = []
        self.m = shared_materials()
        self.m["skin"] = material(f"Skin_{name}", skin)
        self.m["skin_dark"] = material(f"SkinDark_{name}", tuple(c * 0.7 for c in skin))
        # Every figure has one material named Suit in its export; in the
        # blockout scene each one gets its own slot colour.
        self.m["suit"] = material(f"Suit_{name}", suit)

    def _add(self, obj, mat):
        obj.data.materials.append(self.m[mat])
        self.parts.append(obj)
        return obj

    # -- primitives

    def blob(self, mat, loc, size, segments=None, rings=None, rot=(0, 0, 0)):
        """Ellipsoid with half-axes `size` (x, y, z); small ones get fewer faces."""
        small = max(size) < 0.04
        segments = segments or (10 if small else 16)
        rings = rings or (6 if small else 10)
        bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=rings, radius=1, location=loc)
        o = bpy.context.active_object
        o.scale = size
        o.rotation_euler = tuple(math.radians(a) for a in rot)
        return self._add(o, mat)

    def cap(self, mat, loc, size, keep_above, tilt=0.0, segments=16, rings=10):
        """Top of an ellipsoid shell (z above `keep_above` of its height), tilted
        forward by `tilt` degrees: eyelids, hair caps, hoods."""
        bpy.ops.mesh.primitive_uv_sphere_add(segments=segments, ring_count=rings, radius=1, location=loc)
        o = bpy.context.active_object
        bm = bmesh.new()
        bm.from_mesh(o.data)
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if v.co.z < keep_above - 1e-4], context="VERTS")
        bm.to_mesh(o.data)
        bm.free()
        o.scale = size
        o.rotation_euler = (math.radians(tilt), 0, 0)
        return self._add(o, mat)

    def tube(self, mat, points, radius, taper=None, sides=None):
        """Smooth noodle through `points`; `taper` scales the radius per point.

        Thick tubes get more sides: below 40 degrees per side they shade round.
        """
        sides = sides or (12 if radius >= 0.04 else 8)
        curve = bpy.data.curves.new("tube", "CURVE")
        curve.dimensions = "3D"
        curve.bevel_depth = radius
        curve.bevel_resolution = (sides - 4) // 2  # ring of 4 + 2 * resolution
        curve.resolution_u = 4
        curve.use_fill_caps = True
        spline = curve.splines.new("NURBS" if len(points) > 2 else "POLY")
        spline.points.add(len(points) - 1)
        for i, p in enumerate(points):
            spline.points[i].co = (*p, 1)
            spline.points[i].radius = taper[i] if taper else 1
        if len(points) > 2:
            spline.order_u = 3
            spline.use_endpoint_u = True
        o = bpy.data.objects.new("tube", curve)
        bpy.context.collection.objects.link(o)
        bpy.ops.object.select_all(action="DESELECT")
        o.select_set(True)
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.convert(target="MESH")
        return self._add(bpy.context.active_object, mat)

    def torus(self, mat, loc, major, minor, squash=1.0, segments=24):
        bpy.ops.mesh.primitive_torus_add(major_segments=segments, minor_segments=8,
                                         major_radius=major, minor_radius=minor, location=loc)
        o = bpy.context.active_object
        o.scale = (1, squash, 1)
        return self._add(o, mat)

    def box(self, mat, loc, size):
        bpy.ops.mesh.primitive_cube_add(size=1, location=loc)
        o = bpy.context.active_object
        o.scale = size
        return self._add(o, mat)

    # -- features

    def eye(self, centre, radius, look=(0, 0), lid=0.3, lid_tilt=30, lid_mat="skin", low_lid=0.0):
        """Eyeball with a dot pupil and a heavy upper lid.

        look shifts the pupil (x, z) in fractions of the radius; lid is how far
        down the lid comes (0 open, 0.6 half shut), lid_tilt turns it forward;
        low_lid does the same from below.
        """
        centre = Vector(centre)
        seg, rings = (24, 14) if radius > 0.08 else (14, 8)
        self.blob("eye", centre, (radius,) * 3, segments=seg, rings=rings)
        pupil = centre + Vector((look[0] * radius, -radius * 0.92, look[1] * radius))
        self.blob("pupil", pupil, (radius * 0.24, radius * 0.12, radius * 0.24), segments=8, rings=4)
        if lid:
            self.cap(lid_mat, centre, (radius * 1.08,) * 3, keep_above=1 - 2 * lid, tilt=lid_tilt,
                     segments=seg, rings=rings)
        if low_lid:
            self.cap(lid_mat, centre, (radius * 1.06,) * 3, keep_above=1 - 2 * low_lid, tilt=180 - 15,
                     segments=seg, rings=rings)

    def eye_on(self, head, dx, dz, radius, **kw):
        self.eye(surface_front(head, dx, dz, inset=radius * 0.45), radius, **kw)

    def brow(self, head, dx, dz, width, angle=0.0, mat="hair"):
        """Short thick brow over an eye; angle in degrees, positive = outer end up."""
        a = math.radians(angle)
        side = 1 if dx >= 0 else -1
        pts = []
        for t in (-1, 0, 1):
            x = dx + t * width / 2
            z = dz + t * side * math.tan(a) * width / 2 + (0.006 if t == 0 else 0)
            pts.append(surface_front(head, x, z, inset=-0.006))
        self.tube(mat, pts, 0.011, taper=[0.6, 1, 0.6], sides=6)

    def ear(self, head, side, dz=0.0, size=1.0):
        c, (rx, ry, rz) = head.location, head.scale
        loc = (c.x + side * rx * 0.97, c.y + 0.01, c.z + dz)
        self.blob("skin", loc, (0.022 * size, 0.04 * size, 0.055 * size), rot=(0, side * -15, 0))
        self.blob("skin_dark", (loc[0] + side * 0.01, loc[1] - 0.004, loc[2]),
                  (0.012 * size, 0.026 * size, 0.038 * size))

    def mouth(self, head, dz, width, open_=0.015, lip=True):
        loc = surface_front(head, 0, dz, inset=0.012)
        self.blob("mouth", loc, (width, 0.015, open_))
        if lip:
            self.tube("skin_dark", [loc + Vector((-width * 1.05, -0.006, -open_ * 0.6)),
                                    loc + Vector((0, -0.012, -open_ * 1.3)),
                                    loc + Vector((width * 1.05, -0.006, -open_ * 0.6))], 0.008, sides=6)
        return loc

    def hand(self, wrist, side, fingers=3, size=1.0, mat="skin"):
        """Cartoon hand hanging down: palm, `fingers` fingers and a thumb."""
        w = Vector(wrist)
        s = size
        self.blob(mat, w + Vector((0, 0, -0.035 * s)), (0.02 * s, 0.035 * s, 0.04 * s))
        for i in range(fingers):
            y = (-0.022 + i * 0.044 / max(1, fingers - 1)) * s
            top = w + Vector((0, y, -0.065 * s))
            self.tube(mat, [top, top + Vector((side * 0.006, y * 0.2, -0.045 * s))], 0.01 * s, sides=6)
        t = w + Vector((side * -0.008, -0.03 * s, -0.03 * s))
        self.tube(mat, [t, t + Vector((0, -0.02 * s, -0.03 * s))], 0.01 * s, sides=6)

    def shoe(self, x, side, y=0.0, size=1.0):
        s = size
        self.blob("sole", (x, y - 0.035 * s, 0.018 * s), (0.07 * s, 0.13 * s, 0.018 * s))
        self.blob("shoes", (x, y - 0.03 * s, 0.05 * s), (0.065 * s, 0.12 * s, 0.045 * s))
        for k in (0, 1):
            self.tube("trim", [(x - 0.03 * s, y - (0.07 + k * 0.025) * s, 0.085 * s),
                               (x + 0.03 * s, y - (0.07 + k * 0.025) * s, 0.085 * s)], 0.005, sides=4)

    def spots(self, blob, where, mat="skin_dark"):
        """Flat bumps on an ellipsoid: where = [(azimuth deg, elevation deg, radius)]."""
        c, (rx, ry, rz) = blob.location, blob.scale
        for az, el, r in where:
            a, e = math.radians(az), math.radians(el)
            n = Vector((math.cos(e) * math.sin(a), -math.cos(e) * math.cos(a), math.sin(e)))
            p = c + Vector((n.x * rx, n.y * ry, n.z * rz)) * 0.985
            self.blob(mat, p, (r, r, r))

    def skin_body(self, joints, bones, levels=2):
        """One seamless body from a joint skeleton (skin modifier + subdivision).

        joints: {name: ((x, y, z), (radius_a, radius_b))}; the first joint is the root.
        bones: [(joint_a, joint_b, zones)]; zones is a material name or
        ((t, material), ...) to colour a bone from joint_a (t=0) to joint_b (t=1),
        each entry from its t on. Clothes are colour zones, as on a painted body.
        """
        names = list(joints)
        mesh = bpy.data.meshes.new(f"{self.name}_body")
        mesh.from_pydata([joints[n][0] for n in names],
                         [(names.index(a), names.index(b)) for a, b, _ in bones], [])
        o = bpy.data.objects.new(f"{self.name}_body", mesh)
        bpy.context.collection.objects.link(o)
        o.modifiers.new("skin", "SKIN")
        for i, n in enumerate(names):
            v = mesh.skin_vertices[0].data[i]
            v.radius = joints[n][1]
            v.use_root = i == 0
        sub = o.modifiers.new("subdiv", "SUBSURF")
        sub.levels = levels
        bpy.ops.object.select_all(action="DESELECT")
        o.select_set(True)
        bpy.context.view_layer.objects.active = o
        for m in list(o.modifiers):
            bpy.ops.object.modifier_apply(modifier=m.name)

        tris = sum(len(p.vertices) - 2 for p in o.data.polygons)
        if tris > BODY_BUDGET:
            dec = o.modifiers.new("body_budget", "DECIMATE")
            dec.ratio = BODY_BUDGET / tris
            bpy.ops.object.modifier_apply(modifier=dec.name)

        mats = sorted({z for _, _, zs in bones for z in ([zs] if isinstance(zs, str) else [m for _, m in zs])})
        for m in mats:
            o.data.materials.append(self.m[m])
        segs = [(Vector(joints[a][0]), Vector(joints[b][0]), zs) for a, b, zs in bones]
        radii = [(sum(joints[a][1]) / 2, sum(joints[b][1]) / 2) for a, b, _ in bones]

        def nearest(c):
            """Bone whose surface is closest (distance minus its radius), and t."""
            best = None
            for i, (a, b, _) in enumerate(segs):
                ab = b - a
                t = max(0.0, min(1.0, (c - a).dot(ab) / ab.length_squared))
                d = (a + ab * t - c).length - (radii[i][0] + (radii[i][1] - radii[i][0]) * t)
                if best is None or d < best[0]:
                    best = (d, i, t)
            return best[1], best[2]

        # Cut a clean edge loop where a bone changes colour (hems, cuffs),
        # only through the faces that belong to that bone.
        bm = bmesh.new()
        bm.from_mesh(o.data)
        for i, (a, b, zs) in enumerate(segs):
            if isinstance(zs, str):
                continue
            for t0, _ in zs[1:]:
                bm.faces.ensure_lookup_table()
                faces = [fc for fc in bm.faces if nearest(fc.calc_center_median())[0] == i]
                geom = list({e for fc in faces for e in fc.edges}) + faces + list({v for fc in faces for v in fc.verts})
                bmesh.ops.bisect_plane(bm, geom=geom, plane_co=a + (b - a) * t0, plane_no=(b - a).normalized())
        for fc in bm.faces:
            i, t = nearest(fc.calc_center_median())
            zs = segs[i][2]
            name = zs if isinstance(zs, str) else [m for t0, m in zs if t >= t0][-1]
            fc.material_index = mats.index(name)
        bm.to_mesh(o.data)
        bm.free()
        o.data.calc_loop_triangles()
        vs = o.data.vertices
        self.body_tris = [tuple(vs[i].co.copy() for i in t.vertices) for t in o.data.loop_triangles]
        self.parts.append(o)
        self.body = o
        return o

    def surface(self, origin, direction):
        """First hit on the body from origin along direction, or None."""
        origin, direction = Vector(origin), Vector(direction).normalized()
        hits = [p for tri in self.body_tris if (p := intersect_ray_tri(*tri, direction, origin, True))]
        return min(hits, key=lambda p: (p - origin).length, default=None)

    def hair_shell(self, hairline, displace, thickness=0.008, mat="hair"):
        """Hair as one mesh: the scalp above `hairline(co) -> bool` is copied off
        the body, its edge smoothed, moved by `displace(co, normal) -> Vector`,
        then given thickness and smoothed."""
        bm = bmesh.new()
        bm.from_mesh(self.body.data)
        # Work on a finer copy of the upper head so the hairline can have detail.
        top = max(v.co.z for v in bm.verts)
        bmesh.ops.delete(bm, geom=[fc for fc in bm.faces if fc.calc_center_median().z < top - 0.3],
                         context="FACES")
        bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=2, use_grid_fill=True, smooth=1.0)
        keep = {fc for fc in bm.faces if all(hairline(v.co) for v in fc.verts)}
        bmesh.ops.delete(bm, geom=[fc for fc in bm.faces if fc not in keep], context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
        for _ in range(1):
            bmesh.ops.smooth_vert(bm, verts=[v for v in bm.verts if v.is_boundary], factor=0.5,
                                  use_axis_x=True, use_axis_y=True, use_axis_z=True)
        bm.normal_update()
        moved = [(v, displace(v.co.copy(), v.normal.copy())) for v in bm.verts]
        for v, d in moved:
            v.co += d
        me = bpy.data.meshes.new("hair")
        bm.to_mesh(me)
        bm.free()
        for poly in me.polygons:
            poly.material_index = 0
        o = bpy.data.objects.new("hair", me)
        bpy.context.collection.objects.link(o)
        me.materials.clear()
        sol = o.modifiers.new("solidify", "SOLIDIFY")
        sol.thickness = thickness
        sol.offset = -1
        bpy.ops.object.select_all(action="DESELECT")
        o.select_set(True)
        bpy.context.view_layer.objects.active = o
        for m in list(o.modifiers):
            bpy.ops.object.modifier_apply(modifier=m.name)
        return self._add(o, mat)

    def fuse(self, parts, mat, voxel=0.003, smooth=6, triangles=2500):
        """Melt overlapping parts into one closed surface (voxel remesh), round
        off the joins and bring it back to about `triangles`."""
        for o in parts:
            self.parts.remove(o)
        bpy.ops.object.select_all(action="DESELECT")
        for o in parts:
            o.select_set(True)
        bpy.context.view_layer.objects.active = parts[0]
        if len(parts) > 1:
            bpy.ops.object.join()
        o = bpy.context.active_object
        bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
        rem = o.modifiers.new("remesh", "REMESH")
        rem.mode = "VOXEL"
        rem.voxel_size = voxel
        sm = o.modifiers.new("smooth", "SMOOTH")
        sm.factor = 0.5
        sm.iterations = smooth
        bpy.ops.object.modifier_apply(modifier=rem.name)
        bpy.ops.object.modifier_apply(modifier=sm.name)
        tris = sum(len(p.vertices) - 2 for p in o.data.polygons)
        if tris > triangles:
            dec = o.modifiers.new("decimate", "DECIMATE")
            dec.ratio = triangles / tris
            bpy.ops.object.modifier_apply(modifier=dec.name)
        o.data.materials.clear()
        return self._add(o, mat)

    def merge_surfaces(self, parts, budgets=None):
        """Unify organic and cloth volumes; keep eyes and garment edges separate.

        Pass body parts collected before adding facial details. Overlapping
        volumes merge;
        disconnected hands and limbs keep their silhouette.
        """
        budgets = budgets or {"skin": BODY_BUDGET, "suit": 1400, "pants": 400}
        merged = {}
        for key, budget in budgets.items():
            group = [o for o in parts if o in self.parts and
                     len(o.data.materials) == 1 and o.data.materials[0] == self.m[key]]
            if len(group) > 1:
                merged[key] = self.fuse(group, key, voxel=0.004, smooth=4, triangles=budget)
        return merged

    def build(self, x=0.0):
        bpy.ops.object.select_all(action="DESELECT")
        for p in self.parts:
            p.select_set(True)
        bpy.context.view_layer.objects.active = self.parts[0]
        bpy.ops.object.join()
        o = bpy.context.active_object
        o.name = self.name
        bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
        # Joined primitives retain the first primitive's origin. Put every
        # model in the same coordinate frame before positioning the lineup.
        bpy.context.scene.cursor.location = (0, 0, 0)
        bpy.ops.object.origin_set(type="ORIGIN_CURSOR")
        ground_character(o)
        if self.name in {"NorbPainted", "NorbMullet", "NorbSidePart", "NorbSpikes"}:
            refine_norb(o)
        shade_character(o)
        o.location.x = x
        return o


def ground_character(obj):
    """Place the lowest point at local z=0, keeping the shared foot origin."""
    floor = min(v.co.z for v in obj.data.vertices)
    for vertex in obj.data.vertices:
        vertex.co.z -= floor


def refine_norb(obj):
    """Reproduce the jaw and hair refinement selected in live Blender MCP.

    Paint stays attached to the skin while the lower face becomes shorter and
    the jaw separates more clearly from the neck. Separate face parts retain
    their shape. Positions use the grounded mesh's metre coordinates.
    """
    skin_slots = {i for i, mat in enumerate(obj.data.materials)
                  if mat.name.startswith(f"{obj.name}_skin")}
    vertices = {v for poly in obj.data.polygons if poly.material_index in skin_slots
                for v in poly.vertices}
    for index in vertices:
        vertex = obj.data.vertices[index]
        z = vertex.co.z
        jaw = math.exp(-((z - 1.48) / 0.045) ** 2)
        throat = math.exp(-((z - 1.405) / 0.026) ** 2)
        vertex.co.x *= 1 + 0.16 * jaw - 0.08 * throat
        if vertex.co.y < 0:
            vertex.co.y -= 0.006 * jaw
        vertex.co.z += 0.012 * math.exp(-((z - 1.45) / 0.025) ** 2)

    if obj.name in {"NorbPainted", "NorbSidePart"}:
        hair_slots = {i for i, mat in enumerate(obj.data.materials) if mat.name == "Hair"}
        vertices = sorted({v for poly in obj.data.polygons if poly.material_index in hair_slots
                           for v in poly.vertices})
        group = obj.vertex_groups.new(name="HairRefinement")
        group.add(vertices, 1.0, "REPLACE")
        smooth = obj.modifiers.new("HairRefinement", "SMOOTH")
        smooth.vertex_group = group.name
        smooth.factor = 0.4
        smooth.iterations = 2
        bpy.context.view_layer.objects.active = obj
        group_name = group.name
        bpy.ops.object.modifier_apply(modifier=smooth.name)
        group = obj.vertex_groups.get(group_name)
        if group is not None:
            obj.vertex_groups.remove(group)

        if obj.name == "NorbPainted":
            skin_vertices = {v for poly in obj.data.polygons if poly.material_index in skin_slots
                             for v in poly.vertices}
            hair_vertices = {v for poly in obj.data.polygons if poly.material_index in hair_slots
                             for v in poly.vertices}
            scalp = max(obj.data.vertices[i].co.z for i in skin_vertices)
            top = max(obj.data.vertices[i].co.z for i in hair_vertices)
            pivot = scalp - 0.025
            target = scalp + 0.022
            ratio = min(1.0, (target - pivot) / (top - pivot))
            for index in hair_vertices:
                vertex = obj.data.vertices[index]
                if vertex.co.z > pivot:
                    vertex.co.z = pivot + (vertex.co.z - pivot) * ratio
    # Enlarge the complete head, including eyes, ears and hair, together.
    # Smoothly blend into the upper neck to retain a continuous silhouette.
    for vertex in obj.data.vertices:
        z = vertex.co.z
        t = max(0.0, min(1.0, (z - 1.39) / 0.08))
        weight = t * t * (3.0 - 2.0 * t)
        vertex.co.x *= 1.0 + 0.25 * weight
        vertex.co.y *= 1.0 + 0.25 * weight
        vertex.co.z += 0.25 * weight * (z - 1.48)
    obj.data.update()


def shade_character(obj):
    """Keep true garment corners sharp, organic surfaces continuously smooth.

    A decimated curved surface can have steep triangulation edges; those are
    not designed hard edges. Material names identify the organic surfaces.
    """
    mesh = obj.data
    mesh.shade_smooth()
    mesh.set_sharp_from_angle(angle=SHARP_ANGLE)
    organic = {"EyeWhite", "Pupil", "Tongue", "Tooth", "Hair"}
    indices = {i for i, mat in enumerate(mesh.materials)
               if mat.name in organic or mat.name.startswith(("Skin_", "SkinDark_"))
               or mat.name.split(".", 1)[0] == f"{obj.name}_skin"}
    edge_lookup = {tuple(sorted(e.vertices)): e.index for e in mesh.edges}
    touches = [[] for _ in mesh.edges]
    for poly in mesh.polygons:
        for key in poly.edge_keys:
            touches[edge_lookup[tuple(sorted(key))]].append(poly.material_index)
    sharp = mesh.attributes.get("sharp_edge")
    if sharp:
        for i, materials in enumerate(touches):
            if materials and all(m in indices for m in materials):
                sharp.data[i].value = False


# ---------------------------------------------------------------- painted layers

def linear_to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.power(c, 1 / 2.4) - 0.055)


class Paint:
    """A texture painted by functions of the body surface, like Schedule I's layers.

    Every texel knows where it sits on the body (P, metres) and which zone it
    belongs to; painters set colours with masks over P. Clothes in the Suit
    zone are painted in grey so the slot colour can be multiplied on top.
    """

    def __init__(self, body, zone_names, size=1024):
        self.size = size
        me = body.data
        me.calc_loop_triangles()
        uv = me.uv_layers.active.data
        co = np.array([v.co for v in me.vertices])
        pos = np.zeros((size, size, 3))
        zone = np.full((size, size), -1, dtype=np.int32)
        for t in me.loop_triangles:
            uvs = np.array([uv[i].uv for i in t.loops]) * size - 0.5
            pts = co[list(t.vertices)]
            x0, y0 = np.maximum(np.floor(uvs.min(0)).astype(int), 0)
            x1, y1 = np.minimum(np.ceil(uvs.max(0)).astype(int), size - 1)
            if x1 < x0 or y1 < y0:
                continue
            xs, ys = np.meshgrid(np.arange(x0, x1 + 1), np.arange(y0, y1 + 1))
            a, b, c = uvs
            m = np.array([[b[0] - a[0], c[0] - a[0]], [b[1] - a[1], c[1] - a[1]]])
            det = np.linalg.det(m)
            if abs(det) < 1e-12:
                continue
            inv = np.linalg.inv(m)
            d = np.stack([xs - a[0], ys - a[1]], -1) @ inv.T
            w = np.stack([1 - d[..., 0] - d[..., 1], d[..., 0], d[..., 1]], -1)
            inside = (w >= -0.02).all(-1)
            pos[ys[inside], xs[inside]] = w[inside] @ pts
            zone[ys[inside], xs[inside]] = t.material_index
        self.covered = zone >= 0
        self.P = pos
        self.zone = zone
        self.zone_names = zone_names
        self.C = np.ones((size, size, 3))

    def mask(self, zone):
        return self.zone == self.zone_names.index(zone)

    def fill(self, zone, rgb):
        self.C[self.mask(zone)] = rgb

    def put(self, where, rgb, opacity=1.0):
        """Blend rgb over the texels in mask `where` (opacity may be an array)."""
        a = np.broadcast_to(np.asarray(opacity, dtype=float), where.shape)[where][:, None]
        self.C[where] = self.C[where] * (1 - a) + np.asarray(rgb) * a

    def stroke(self, zone, points, width, rgb, front=True):
        """A line through `points` (x, z) on the front (or back) of a zone."""
        P = self.P
        x, z = P[..., 0], P[..., 2]
        dist = np.full(x.shape, np.inf)
        for (ax, az), (bx, bz) in zip(points, points[1:]):
            vx, vz = bx - ax, bz - az
            t = np.clip(((x - ax) * vx + (z - az) * vz) / (vx * vx + vz * vz), 0, 1)
            dist = np.minimum(dist, np.hypot(x - (ax + t * vx), z - (az + t * vz)))
        side = P[..., 1] < 0 if front else P[..., 1] > 0
        self.put(self.mask(zone) & side & (dist < width), rgb)

    def image(self, name):
        """Bleed colours into the empty texels (no seams when mipmapped) and
        return the result as a packed sRGB image."""
        C, filled = self.C.copy(), self.covered.copy()
        for _ in range(8):
            for dy, dx in ((0, 1), (0, -1), (1, 0), (-1, 0)):
                src = np.roll(np.roll(filled, dy, 0), dx, 1)
                take = src & ~filled
                C[take] = np.roll(np.roll(C, dy, 0), dx, 1)[take]
                filled |= take
        img = bpy.data.images.new(name, self.size, self.size)
        rgba = np.concatenate([linear_to_srgb(C), np.ones((self.size, self.size, 1))], -1)
        img.pixels.foreach_set(rgba.astype(np.float32).ravel())
        img.pack()
        return img


def painted_material(name, img, tint=None):
    """Material showing `img`; tint (the slot colour) is multiplied on top."""
    m = bpy.data.materials.new(name)
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.6
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = img
    out = tex.outputs["Color"]
    if tint:
        mix = nt.nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        mix.blend_type = "MULTIPLY"
        mix.inputs["Factor"].default_value = 1.0
        nt.links.new(out, mix.inputs["A"])
        mix.inputs["B"].default_value = (*tint, 1)
        out = mix.outputs["Result"]
    nt.links.new(out, bsdf.inputs["Base Color"])
    return m


def unwrap(body, zoom=None):
    """UV-unwrap the body. zoom=(predicate, factor) gives matching vertices
    (the face) more texels by unwrapping them scaled up."""
    me = body.data
    saved = [v.co.copy() for v in me.vertices]
    if zoom:
        pick, factor, centre = zoom
        for v in me.vertices:
            if pick(v.co):
                v.co = centre + (v.co - centre) * factor
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=0.01)
    bpy.ops.object.mode_set(mode="OBJECT")
    for v, co in zip(me.vertices, saved):
        v.co = co


def surface_front(blob, dx, dz, inset=0.0):
    """Point on the front (-Y) of an ellipsoid blob, pushed `inset` inwards."""
    c, (rx, ry, rz) = blob.location, blob.scale
    k = max(0.0, 1 - (dx / rx) ** 2 - (dz / rz) ** 2)
    return Vector((c.x + dx, c.y - ry * math.sqrt(k) + inset, c.z + dz))


def paint_alien_face(f, body, mouth, width, opening):
    """Use the same painted face language as Norb on a fused alien body.

    Eyes and brows stay geometry. Mouth and cheek colour sit on the skin,
    avoiding separate floating lip volumes.
    """
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    unwrap(body)
    pt = Paint(body, ["skin"])
    skin = f.m["skin"].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value[:3]
    pt.fill("skin", skin)
    x, y, z = (pt.P[..., i] for i in range(3))
    front = y < mouth.y + 0.025
    # Slightly crooked smile, with a soft lower lip painted into the skin.
    line = mouth.z + 0.18 * (x - mouth.x) ** 2 / width
    d = ((x - mouth.x) / width) ** 2 + ((z - line) / opening) ** 2
    pt.put(pt.mask("skin") & front & (d < 1), (0.25, 0.04, 0.06))
    lip = np.hypot((x - mouth.x) / width, (z - line + opening * 0.4) / (opening * 1.15))
    pt.put(pt.mask("skin") & front & (lip < 1.1) & (d >= 1),
           tuple(c * 0.72 for c in skin), 0.5)
    for side in (-1, 1):
        cheek = np.hypot(x - mouth.x - side * width * 1.05, z - mouth.z - opening * 1.8)
        radius = width * 0.28
        pt.put(pt.mask("skin") & front & (cheek < radius),
               tuple(c * 0.85 for c in skin), np.clip(1 - cheek / radius, 0, 1) * 0.3)
    body.data.materials[0] = painted_material(f"{f.name}_skin", pt.image(f"{f.name}_face"))


def paint_alien_clothes(f, body, strokes):
    """Paint broad clothing seams in grey; the shared slot tint colours them."""
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    unwrap(body)
    pt = Paint(body, ["suit"], size=512)
    pt.fill("suit", (1, 1, 1))
    for points, width, shade in strokes:
        pt.stroke("suit", points, width, (shade,) * 3)
    tint = f.m["suit"].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value[:3]
    body.data.materials[0] = painted_material(f"{f.name}_suit", pt.image(f"{f.name}_clothes"), tint=tint)


def mirrored(f, fn):
    for s in (-1, 1):
        fn(f, s)


# ---------------------------------------------------------------- the four

def norb_body(f):
    """The human base body: one seamless mesh, no face; returns the skeleton."""
    # Skeleton: (position, (radius across, radius front-back)); front is -Y.
    j = {
        "pelvis": ((0, 0, 0.9), (0.13, 0.09)),
        "waist": ((0, 0, 1.03), (0.12, 0.085)),
        "chest": ((0, 0, 1.2), (0.14, 0.09)),
        "neck": ((0, 0, 1.365), (0.038, 0.038)),
        "neck_top": ((0, 0, 1.43), (0.038, 0.038)),
        "jaw": ((0, -0.012, 1.485), (0.112, 0.095)),
        "head": ((0, -0.005, 1.57), (0.12, 0.12)),
        "crown": ((0, 0.005, 1.67), (0.1, 0.1)),
    }
    for s, side in ((-1, "l"), (1, "r")):
        j |= {
            f"hip_{side}": ((s * 0.085, 0, 0.84), (0.065, 0.065)),
            f"knee_{side}": ((s * 0.09, 0.0, 0.47), (0.047, 0.047)),
            f"ankle_{side}": ((s * 0.09, 0.01, 0.09), (0.04, 0.04)),
            f"toe_{side}": ((s * 0.095, -0.15, 0.045), (0.055, 0.04)),
            f"shoulder_{side}": ((s * 0.17, 0, 1.3), (0.045, 0.045)),
            f"elbow_{side}": ((s * 0.23, 0.02, 1.04), (0.034, 0.034)),
            f"wrist_{side}": ((s * 0.255, -0.01, 0.8), (0.026, 0.022)),
            f"hand_{side}": ((s * 0.265, -0.02, 0.69), (0.032, 0.014)),
            f"thumb_{side}": ((s * 0.245, -0.06, 0.74), (0.011, 0.011)),
        }
    shirt_then_skin = ((0.0, "suit"), (0.45, "skin"))
    bones = [("pelvis", "waist", ((0.0, "pants"), (0.25, "suit"))), ("waist", "chest", "suit"),
             ("chest", "neck", ((0.0, "suit"), (0.8, "skin"))), ("neck", "neck_top", "skin"), ("neck_top", "jaw", "skin"),
             ("jaw", "head", "skin"), ("head", "crown", "skin")]
    for side in "lr":
        bones += [("pelvis", f"hip_{side}", "pants"), (f"hip_{side}", f"knee_{side}", "pants"),
                  (f"knee_{side}", f"ankle_{side}", ((0.0, "pants"), (0.93, "shoes"))),
                  (f"ankle_{side}", f"toe_{side}", "shoes"),
                  ("chest", f"shoulder_{side}", "suit"), (f"shoulder_{side}", f"elbow_{side}", shirt_then_skin),
                  (f"elbow_{side}", f"wrist_{side}", "skin"), (f"wrist_{side}", f"hand_{side}", "skin"),
                  (f"wrist_{side}", f"thumb_{side}", "skin")]
    f.skin_body(j, bones)
    return j



def head_frame(f):
    head_vs = [v.co for v in f.body.data.vertices if v.co.z > 1.47]
    lo = Vector([min(v[i] for v in head_vs) for i in range(3)])
    hi = Vector([max(v[i] for v in head_vs) for i in range(3)])
    return (lo + hi) / 2, (hi - lo) / 2, hi


def norb_head_parts(f, hc, he, hi, front, hair="mop"):
    """Eyes, brows, ears, nose and hair: the parts in 3D."""
    for s, look, lid, low in ((-1, (0.18, 0.0), 0.34, 0.12), (1, (-0.12, -0.05), 0.27, 0.1)):
        r = 0.036 if s < 0 else 0.039
        f.eye(front(s * 0.045, hc.z + 0.01, inset=r * 0.55), r, look, lid=lid, lid_tilt=18, low_lid=low)
    for s, angle, dz in ((-1, -6, 0.0), (1, 10, 0.006)):
        pts = [front(s * x, hc.z + 0.055 + dz + (x - 0.04) * math.tan(math.radians(angle)), inset=-0.004)
               for x in (0.016, 0.04, 0.064)]
        f.tube("hair", pts, 0.009, taper=[0.7, 1, 0.8], sides=6)
    for s in (-1, 1):
        loc = f.surface((s, hc.y, hc.z - 0.005), (-s, 0, 0))
        f.blob("skin", loc + Vector((s * 0.008, 0.005, 0)), (0.014, 0.024, 0.036), rot=(0, s * -15, 0))

    first = len(f.parts)
    HAIRSTYLES[hair](f, hc, he)
    f.fuse(f.parts[first:], "hair", triangles=HAIR_BUDGET)
    first = len(f.parts)
    bridge = front(0, hc.z - 0.008, inset=0.009)
    tip = bridge + Vector((0, -0.028, -0.038))
    f.tube("skin", [bridge, bridge + Vector((0, -0.016, -0.02)), tip], 0.012, taper=[0.6, 0.9, 1.1], sides=10)
    f.blob("skin", tip + Vector((0, 0.002, 0)), (0.019, 0.017, 0.016))
    for s in (-1, 1):
        f.blob("skin", tip + Vector((s * 0.014, 0.008, 0.002)), (0.01, 0.01, 0.009))
    f.fuse(f.parts[first:], "skin", voxel=0.0015, smooth=4, triangles=240)


# ---------------------------------------------------------------- hairstyles
#
# A hairstyle is a short cap hugging the scalp (one shell cut from the body)
# plus a few chunky locks that make the style readable from afar.

def head_point(f, hc, he, az, el, lift=0.0):
    """Point on the head surface: az degrees around (0 front, 90 right, 180
    back), el degrees up from the head centre; lifted `lift` metres outwards."""
    a, e = math.radians(az), math.radians(el)
    d = Vector((math.sin(a) * math.cos(e), -math.cos(a) * math.cos(e), math.sin(e)))
    hit = f.surface(hc + d * 0.6, -d)
    if hit is None:
        hit = hc + Vector((d.x * he.x, d.y * he.y, d.z * he.z))
    return hit + d * lift


def outward(az):
    a = math.radians(az)
    return Vector((math.sin(a), -math.cos(a), 0))


def short_cap(f, hc, he, front, side, back, bumps=(), lift=0.006):
    """Close-cropped hair: hairline heights (relative to the head centre) at
    the front, sides and back; bumps = [((x, y, z) offset, height)]."""
    def hairline(co):
        fb = max(-1.0, min(1.0, (co.y - hc.y) / he.y))
        line = side + (front - side) * -fb if fb < 0 else side + (back - side) * fb
        return co.z > hc.z + line

    def displace(co, normal):
        rel = co - hc
        return normal * (lift + sum(h * math.exp(-((rel - Vector(c)).length / 0.04) ** 2) for c, h in bumps))

    f.hair_shell(hairline, displace, thickness=0.009)


def hair_mop(f, hc, he):
    """Messy mop with pointed bangs."""
    def front_line(x):
        phase = (x / 0.032) % 1.0
        return hc.z + 0.06 + 0.022 * abs(phase - 0.5) * 2

    def hairline(co):
        fb = max(-1.0, min(1.0, (co.y - hc.y) / he.y))
        side, back = hc.z + 0.03, hc.z - 0.085
        line = side + (front_line(co.x) - side) * -fb if fb < 0 else side + (back - side) * fb
        return co.z > line

    rng = np.random.default_rng(3)
    tufts = [((x + rng.uniform(-0.01, 0.01), y, 0.11), rng.uniform(0.03, 0.05))
             for x in (-0.06, -0.02, 0.02, 0.06) for y in (-0.05, 0.0, 0.05)]
    tufts += [((s * 0.1, y, 0.04), 0.02) for s in (-1, 1) for y in (-0.02, 0.04)]

    def displace(co, normal):
        rel = co - hc
        d = normal * (0.005 + sum(a * math.exp(-((rel - Vector(c)).length / 0.03) ** 2) for c, a in tufts))
        if co.y < hc.y:
            d += Vector((0, -0.012, -0.016)) * math.exp(-((co.z - front_line(co.x)) / 0.025) ** 2)
        return d

    f.hair_shell(hairline, displace)


def hair_mullet(f, hc, he):
    """Business in front, party in the back: short on top, a mane down the neck."""
    short_cap(f, hc, he, front=0.075, side=0.035, back=-0.09,
              bumps=[((0.0, -0.02, 0.12), 0.012), ((0.0, 0.04, 0.11), 0.01)])
    for i, az in enumerate(np.linspace(118, 242, 9)):
        start = head_point(f, hc, he, az, 30, 0.004)
        mid = head_point(f, hc, he, az, -12, 0.022)
        end = mid + outward(az) * 0.02 + Vector((0, 0, -0.13 - 0.02 * (i % 2)))
        flip = end + outward(az) * 0.03 + Vector((0, 0, 0.012))
        f.tube("hair", [start, mid, end, flip], 0.026, taper=[0.8, 1.0, 0.75, 0.35])


def hair_side_part(f, hc, he):
    """Side parting with a big swoosh across the forehead."""
    short_cap(f, hc, he, front=0.08, side=0.03, back=-0.07)
    for i, az0 in enumerate((-20, -45, -70, -95, -120, -145)):
        az1 = az0 + 110 - i * 8
        start = head_point(f, hc, he, az0, 52, 0.004)
        mid = head_point(f, hc, he, (az0 + az1) / 2, 72 - i * 3, 0.03 - i * 0.003)
        end = head_point(f, hc, he, az1, 32 + i * 4, 0.012)
        f.tube("hair", [start, mid, end], 0.03 - i * 0.002, taper=[0.6, 1.0, 0.45])


def hair_spikes(f, hc, he):
    """Short spikes standing up, a little swept back."""
    short_cap(f, hc, he, front=0.075, side=0.04, back=-0.06)
    for el, azs in ((48, range(-150, 180, 40)), (72, range(-135, 180, 60)), (88, (0,))):
        for az in azs:
            base = head_point(f, hc, he, az, el, -0.004)
            a, e = math.radians(az), math.radians(min(el + 12, 90))
            d = Vector((math.sin(a) * math.cos(e), -math.cos(a) * math.cos(e), math.sin(e)))
            tip = base + d * 0.07 + Vector((0, 0.02, 0))
            f.tube("hair", [base, tip], 0.024, taper=[1.0, 0.08], sides=8)


HAIRSTYLES = {"mop": hair_mop, "mullet": hair_mullet, "side_part": hair_side_part, "spikes": hair_spikes}


def front_of(f):
    def front(x, z, inset=0.0):
        loc = f.surface((x, -1, z), (0, 1, 0))
        assert loc is not None, f"nothing on the body at x={x:.3f}, z={z:.3f}"
        return loc + Vector((0, inset, 0))
    return front


def norb():
    """Human with a modelled face: nose and mouth as shapes on the body."""
    f = Figure("Norb", skin=(0.93, 0.72, 0.58), suit=(0.1, 0.55, 0.85))
    norb_body(f)
    hc, he, hi = head_frame(f)
    front = front_of(f)
    norb_head_parts(f, hc, he, hi, front)
    f.tube("mouth", [front(x, hc.z - 0.085 + 6 * x * x, inset=0.002) for x in (-0.042, -0.02, 0.0, 0.02, 0.038)],
           0.0055, sides=6)
    return f


def norb_painted(name="NorbPainted", hair="mop", slot=(0.1, 0.55, 0.85)):
    """Human with painted layers: face details and clothes live on a texture,
    only eyes, brows, nose, ears and hair are shapes (as in Schedule I)."""
    skin = (0.93, 0.72, 0.58)
    f = Figure(name, skin=skin, suit=slot)
    j = norb_body(f)
    body = f.body
    hc, he, hi = head_frame(f)
    norb_head_parts(f, hc, he, hi, front_of(f), hair=hair)

    unwrap(body, zoom=(lambda co: co.z > 1.42, 2.5, Vector(hc)))
    zones = [m.name for m in body.data.materials]
    names = {f.m[k].name: k for k in ("skin", "suit", "pants", "shoes")}
    pt = Paint(body, [names[z] for z in zones])
    P = pt.P
    x, y, z = P[..., 0], P[..., 1], P[..., 2]
    skin_dark = tuple(c * 0.62 for c in skin)

    # skin: face lines, blush, freckles
    pt.fill("skin", skin)
    ez, mz = hc.z + 0.01, hc.z - 0.085
    pt.stroke("skin", [(xx, mz + 6 * xx * xx) for xx in np.linspace(-0.042, 0.04, 9)], 0.0042, (0.36, 0.07, 0.08))
    for s in (-1, 1):
        pt.stroke("skin", [(s * xx, ez - 0.05 - 0.15 * (xx - s * 0.045 * s) ** 2) for xx in (0.028, 0.045, 0.062)],
                  0.0018, tuple(c * 0.78 for c in skin))
        cheek = np.hypot(x - s * 0.068, z - (ez - 0.065))
        pt.put(pt.mask("skin") & (y < 0) & (cheek < 0.03), (0.95, 0.5, 0.48), np.clip(1 - cheek / 0.03, 0, 1) * 0.35)
    rng = np.random.default_rng(7)
    for _ in range(14):
        s = rng.choice((-1, 1))
        fx, fz = s * rng.uniform(0.03, 0.085), ez - rng.uniform(0.035, 0.08)
        pt.put(pt.mask("skin") & (y < 0) & (np.hypot(x - fx, z - fz) < 0.0032), (0.7, 0.42, 0.3), 0.7)
    for s in (-1, 1):  # knuckle lines on the fists
        for k in range(3):
            hz = j[f"hand_{'l' if s < 0 else 'r'}"][0][2] + 0.02 - k * 0.012
            pt.put(pt.mask("skin") & (np.abs(z - hz) < 0.0016) & (np.abs(x - s * 0.265) < 0.03) & (y < -0.02),
                   skin_dark)

    # shirt (grey, the slot colour multiplies on top): stitched collar, cuffs, hem, a planet print
    pt.fill("suit", (1, 1, 1))
    seam = (0.62, 0.62, 0.62)
    neck_a, neck_b = Vector(j["chest"][0]), Vector(j["neck"][0])
    nz = neck_a.z + (neck_b.z - neck_a.z) * 0.8
    pt.put(pt.mask("suit") & (z > nz - 0.022), seam)
    pt.put(pt.mask("suit") & (z > nz - 0.03) & (z < nz - 0.026), seam)
    pt.put(pt.mask("suit") & (z < 0.948) & (z > 0.944), seam)
    for side in "lr":
        a, b = Vector(j[f"shoulder_{side}"][0]), Vector(j[f"elbow_{side}"][0])
        plane, normal = a + (b - a) * 0.45, (b - a).normalized()
        rel = P - np.array(plane)
        d = rel @ np.array(normal)
        off_axis = np.linalg.norm(rel - d[..., None] * np.array(normal), axis=-1)
        pt.put(pt.mask("suit") & (d > -0.02) & (d < -0.016) & (off_axis < 0.07), seam)
    px, pz = 0.045, 1.16
    ring = np.hypot((x - px) / 0.055, (z - pz) / 0.016)
    disc = np.hypot(x - px, z - pz)
    front_chest = pt.mask("suit") & (y < 0)
    pt.put(front_chest & (disc < 0.03), (0.35, 0.35, 0.35))
    pt.put(front_chest & (disc < 0.03) & (z > pz + 0.01) & (x < px - 0.005), (0.55, 0.55, 0.55))
    pt.put(front_chest & (np.abs(ring - 1) < 0.12) & ~((disc < 0.03) & (z > pz)), (0.2, 0.2, 0.2))

    # trousers: belt and buckle, fly, pockets, cuffs
    pants = (0.16, 0.18, 0.3)
    pt.fill("pants", pants)
    dark = tuple(c * 0.6 for c in pants)
    pt.put(pt.mask("pants") & (z > 0.902), (0.3, 0.17, 0.08))
    pt.put(pt.mask("pants") & (z > 0.905) & (z < 0.928) & (np.abs(x) < 0.02) & (y < 0), (0.8, 0.75, 0.55))
    pt.stroke("pants", [(0.0, 0.9), (0.0, 0.81), (0.012, 0.79)], 0.0025, dark)
    for s in (-1, 1):
        pt.stroke("pants", [(s * 0.055, 0.9), (s * 0.085, 0.86), (s * 0.115, 0.85)], 0.0025, dark)
        az = j[f"knee_{'l' if s < 0 else 'r'}"][0][2]
        ankle = j[f"ankle_{'l' if s < 0 else 'r'}"][0][2]
        cuff = az + (ankle - az) * 0.93
        pt.put(pt.mask("pants") & (z < cuff + 0.035) & (z > cuff + 0.031), dark)

    # shoes: sole, toe cap line, laces
    pt.fill("shoes", (0.9, 0.9, 0.87))
    pt.put(pt.mask("shoes") & (z < 0.022), (0.2, 0.2, 0.22))
    pt.put(pt.mask("shoes") & (z < 0.03) & (z > 0.026), (0.55, 0.55, 0.55))
    for lz in (0.062, 0.075, 0.088):
        pt.put(pt.mask("shoes") & (np.abs(z - lz) < 0.0025) & (y < -0.03) & (y > -0.11)
               & (np.abs(np.abs(x) - 0.095) < 0.022), (0.3, 0.3, 0.32))

    img = pt.image(f"{name}_paint")
    for i, zname in enumerate(zones):
        key = names[zname]
        body.data.materials[i] = painted_material(f"{name}_{key}", img, tint=slot if key == "suit" else None)
    return f


def glibbo():
    """Alien: mostly head on stilt legs, one huge eye, toothy mouth, scarf."""
    f = Figure("Glibbo", skin=(0.85, 0.68, 0.12), suit=(0.9, 0.2, 0.5))

    def leg(f, s):
        x = s * 0.12
        for t in (-1, 0, 1):
            f.blob("skin", (x + t * 0.03, -0.09, 0.02), (0.022, 0.06, 0.02), rot=(0, 0, t * -15))
        f.blob("skin", (x, -0.02, 0.03), (0.04, 0.05, 0.03))
        f.tube("skin", [(x, 0, 0.03), (x, 0.05, 0.55), (x, 0, 1.0)], 0.028)
        f.blob("skin", (x, 0.04, 0.55), (0.04, 0.04, 0.045))  # knobbly knee

    def arm(f, s):
        f.tube("skin", [(s * 0.3, 0, 1.25), (s * 0.4, -0.02, 1.08), (s * 0.38, -0.08, 0.95)], 0.022)
        f.blob("skin", (s * 0.4, -0.02, 1.08), (0.03, 0.03, 0.03))  # elbow
        f.hand((s * 0.38, -0.08, 0.95), s, fingers=3, size=0.9)

    mirrored(f, leg)
    mirrored(f, arm)
    # scarf: two knitted rings and a hanging end
    f.torus("suit", (0, 0, 1.06), 0.22, 0.06)
    f.torus("trim", (0, 0, 1.11), 0.21, 0.04)
    head = f.blob("skin", (0, 0, 1.33), (0.33, 0.29, 0.33), segments=24, rings=14)
    surfaces = list(f.parts)
    f.eye_on(head, 0, 0.1, 0.13, look=(0.1, -0.05), lid=0.22, lid_tilt=12)
    f.brow(head, 0, 0.27, 0.2, angle=0, mat="skin_dark")
    mouth = surface_front(head, 0, -0.13, inset=0.012)
    for dx, h in ((-0.07, 0.03), (-0.02, 0.022), (0.05, 0.034)):
        f.blob("tooth", mouth + Vector((dx, -0.035, 0.035 - h * 0.4)), (0.02, 0.012, h), segments=8, rings=4)
    f.blob("tongue", mouth + Vector((0.02, -0.03, -0.02)), (0.07, 0.03, 0.025))
    for x, curl in ((-0.05, -1), (0.0, 1), (0.06, 1)):
        base = (x, 0.02, 1.64)
        f.tube("skin_dark", [base, (x + curl * 0.02, 0.0, 1.72), (x + curl * 0.05, -0.02, 1.71)], 0.006, sides=4)
    merged = f.merge_surfaces(surfaces)
    paint_alien_face(f, merged["skin"], mouth, 0.15, 0.05)
    return f


def zorp():
    """Alien: heavy pear body, no neck, three eyes on stalks, hoodie."""
    f = Figure("Zorp", skin=(0.5, 0.3, 0.62), suit=(0.95, 0.5, 0.1))

    def leg(f, s):
        x = s * 0.13
        f.shoe(x, s, size=1.15)
        f.tube("pants", [(x, 0, 0.08), (x, 0, 0.36)], 0.075)
        f.torus("pants", (x, 0, 0.11), 0.072, 0.014, segments=16)

    def arm(f, s):
        f.tube("suit", [(s * 0.21, 0, 1.0), (s * 0.32, -0.02, 0.88), (s * 0.35, -0.05, 0.8)], 0.06)
        f.torus("trim", (s * 0.35, -0.05, 0.79), 0.045, 0.012, segments=16)
        f.hand((s * 0.35, -0.06, 0.79), s, fingers=2, size=1.2)

    mirrored(f, leg)
    mirrored(f, arm)
    # hoodie: belly, chest, waistband, pocket, zip, hood and strings
    f.blob("suit", (0, 0, 0.62), (0.32, 0.27, 0.31), segments=20, rings=12)
    f.blob("suit", (0, 0, 0.95), (0.24, 0.21, 0.2))
    f.torus("trim", (0, 0, 0.38), 0.21, 0.025, squash=0.85)
    f.blob("suit", (0, 0.13, 1.12), (0.2, 0.1, 0.12))
    for s in (-1, 1):
        top = (s * 0.06, -0.17, 1.08)
        f.tube("trim", [top, (s * 0.07, -0.21, 0.96), (s * 0.065, -0.22, 0.88)], 0.006, sides=4)
        f.blob("metal", (s * 0.065, -0.22, 0.87), (0.012, 0.012, 0.018))
    head = f.blob("skin", (0, -0.02, 1.16), (0.17, 0.16, 0.12))
    surfaces = list(f.parts)
    mouth = surface_front(head, 0, -0.03, inset=0.012)
    for tip, r, look, lid in (((-0.13, -0.06, 1.52), 0.055, (0.2, 0), 0.35),
                              ((0.0, -0.09, 1.62), 0.06, (0, -0.15), 0.2),
                              ((0.14, -0.05, 1.48), 0.05, (-0.25, 0.1), 0.45)):
        base = (tip[0] * 0.4, -0.02, 1.25)
        mid = (tip[0] * 0.8, tip[1] * 0.5, (1.25 + tip[2]) / 2 + 0.03)
        stalk = f.tube("skin", [base, mid, (tip[0], tip[1] + 0.02, tip[2] - r * 0.8)], 0.025, taper=[1.4, 1.0, 0.85])
        surfaces.append(stalk)
        f.eye(tip, r, look, lid=lid, lid_tilt=20)
    merged = f.merge_surfaces(surfaces)
    paint_alien_face(f, merged["skin"], mouth, 0.09, 0.016)
    paint_alien_clothes(f, merged["suit"], [
        ([(0, 1.08), (0, 0.68)], 0.006, 0.55),
        ([(-0.13, 0.62), (-0.1, 0.53), (0.1, 0.53), (0.13, 0.62)], 0.005, 0.65),
    ])
    return f


def wobbel():
    """Alien: tentacles for legs, four arms, lumpy head, bathrobe."""
    f = Figure("Wobbel", skin=(0.5, 0.78, 0.25), suit=(0.25, 0.35, 0.9))

    for i in range(6):
        a = math.radians(30 + i * 60)
        c, s = math.cos(a), math.sin(a)
        curl = math.radians(30 + i * 60 + 35)
        f.tube("skin", [(0, 0, 0.55), (c * 0.12, s * 0.12, 0.3), (c * 0.28, s * 0.28, 0.05),
                        (c * 0.38, s * 0.38, 0.03), (math.cos(curl) * 0.4, math.sin(curl) * 0.4, 0.09)],
               0.065, taper=[1, 0.9, 0.55, 0.35, 0.2], sides=8)

    # bathrobe: body, lapels, belt with knot and ends, pocket
    f.blob("suit", (0, 0, 0.86), (0.23, 0.19, 0.36), segments=20, rings=12)
    for s in (-1, 1):
        f.blob("trim", (s * 0.045, -0.172, 1.0), (0.022, 0.02, 0.15), rot=(0, s * 22, 0))
    f.torus("trim", (0, 0, 0.78), 0.215, 0.022, squash=0.85)
    f.blob("trim", (0.05, -0.17, 0.78), (0.035, 0.025, 0.03))
    for dx in (0.03, 0.07):
        f.tube("trim", [(dx, -0.17, 0.77), (dx + 0.01, -0.175, 0.68), (dx, -0.17, 0.6)], 0.012, sides=6)

    def arms(f, s):
        for z, reach in ((1.1, 0.0), (0.95, 0.05)):
            f.blob("suit", (s * 0.19, 0, z), (0.06, 0.06, 0.05))
            f.torus("trim", (s * 0.22, -0.01, z - 0.04), 0.035, 0.014, segments=12)
            f.tube("skin", [(s * 0.2, 0, z), (s * (0.31 + reach), -0.03, z - 0.12),
                            (s * (0.3 + reach), -0.1, z - 0.24)], 0.025)
            f.hand((s * (0.3 + reach), -0.1, z - 0.24), s, fingers=2, size=0.9)

    mirrored(f, arms)
    f.tube("skin", [(0, 0, 1.14), (0, -0.02, 1.26), (0, -0.03, 1.38)], 0.05)
    head = f.blob("skin", (0, -0.03, 1.5), (0.17, 0.15, 0.14), segments=20, rings=12)
    f.blob("skin", (0.08, 0.0, 1.61), (0.12, 0.11, 0.1))
    f.blob("skin", (-0.09, 0.02, 1.58), (0.08, 0.08, 0.07))
    surfaces = list(f.parts)
    f.eye_on(head, -0.07, 0.03, 0.065, look=(0.2, 0.1), lid=0.2, lid_tilt=25)
    f.eye_on(head, 0.07, 0.0, 0.042, look=(-0.1, -0.2), lid=0.42, lid_tilt=25)
    f.brow(head, -0.075, 0.1, 0.07, angle=-15, mat="skin_dark")
    f.brow(head, 0.07, 0.06, 0.05, angle=20, mat="skin_dark")
    f.blob("skin_dark", surface_front(head, 0, -0.03, inset=0.006), (0.012, 0.01, 0.008))
    mouth = surface_front(head, 0, -0.08, inset=0.012)
    merged = f.merge_surfaces(surfaces)
    paint_alien_face(f, merged["skin"], mouth, 0.07, 0.022)
    paint_alien_clothes(f, merged["suit"], [
        ([(-0.15, 0.72), (-0.15, 0.64), (-0.06, 0.64), (-0.06, 0.72)], 0.004, 0.65),
        ([(0, 0.51), (0, 0.73)], 0.004, 0.75),
    ])
    return f


def norb_mullet():
    return norb_painted("NorbMullet", "mullet", slot=(0.9, 0.3, 0.15))


def norb_side_part():
    return norb_painted("NorbSidePart", "side_part", slot=(0.2, 0.7, 0.3))


def norb_spikes():
    return norb_painted("NorbSpikes", "spikes", slot=(0.6, 0.3, 0.8))


FIGURES = [norb_painted, norb_mullet, norb_side_part, norb_spikes, glibbo, zorp, wobbel]
# The earlier geometric-face study remains available explicitly via --only Norb.
STUDIES = [norb]


# ---------------------------------------------------------------- review renders

def stage(night=False):
    scene = bpy.context.scene
    bpy.ops.mesh.primitive_plane_add(size=400)
    ground = bpy.context.active_object
    ground.data.materials.append(material("Ground", (0.08, 0.07, 0.1) if night else (0.42, 0.38, 0.45), 0.9))
    bpy.ops.object.light_add(type="SUN", rotation=(math.radians(55), 0, math.radians(-30)))
    bpy.context.active_object.data.energy = 1.2 if night else 3.5
    world = bpy.data.worlds.new("Sky")
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (
        (0.004, 0.004, 0.015, 1) if night else (0.25, 0.22, 0.35, 1))
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.0
    scene.world = world
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 48
    scene.cycles.device = "GPU"
    return ground


def shoot(path, location, target, lens=None, fov_deg=None, size=(1600, 900)):
    scene = bpy.context.scene
    bpy.ops.object.camera_add(location=location)
    cam = bpy.context.active_object
    cam.rotation_euler = (Vector(target) - Vector(location)).to_track_quat("-Z", "Y").to_euler()
    if fov_deg:
        cam.data.lens_unit = "FOV"
        cam.data.angle = math.radians(fov_deg)
    else:
        cam.data.lens = lens
    scene.camera = cam
    scene.render.resolution_x, scene.render.resolution_y = size
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    bpy.data.objects.remove(cam)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--renders", help="directory for the review renders")
    ap.add_argument("--samples", type=int, default=48, help="Cycles samples per review image")
    ap.add_argument("--blend", help="save the lineup scene for a look in Blender")
    ap.add_argument("--only", help="build just these figures, e.g. Norb,NorbPainted")
    args = ap.parse_args(argv)

    bpy.ops.wm.read_factory_settings(use_empty=True)
    only = [n.strip().lower() for n in args.only.split(",")] if args.only else None
    makers = [m for m in (FIGURES + STUDIES if only else FIGURES)
              if not only or m.__name__.replace("_", "") in only]
    if not makers:
        ap.error("--only matched no figures")
    spacing = 1.2
    xs = [(i - (len(makers) - 1) / 2) * spacing for i in range(len(makers))]
    figures = [make().build(x) for make, x in zip(makers, xs)]
    for o in figures:
        tris = sum(len(p.vertices) - 2 for p in o.data.polygons)
        top = max((o.matrix_world @ v.co).z for v in o.data.vertices)
        half = max(abs(v.co.x) for v in o.data.vertices)
        print(f"FIGURE {o.name}: {tris} triangles, {top:.2f} m tall, {half:.2f} m half width")

    ground = stage()
    bpy.context.scene.cycles.samples = max(1, args.samples)
    if args.blend:
        bpy.ops.wm.save_as_mainfile(filepath=args.blend)
    if args.renders:
        d = args.renders.rstrip("/")
        Path(d).mkdir(parents=True, exist_ok=True)
        distance = max(5.0, len(figures) * spacing * 1.35)
        shoot(f"{d}/lineup_three_quarter.png", (distance * 0.28, -distance, 2.1), (0, 0, 0.9), lens=45)
        shoot(f"{d}/lineup_front.png", (0, -distance, 0.95), (0, 0, 0.9), lens=45)
        shoot(f"{d}/lineup_back.png", (0, distance, 1.4), (0, 0, 0.9), lens=45)
        for o in figures:
            top = max((o.matrix_world @ v.co).z for v in o.data.vertices)
            # Eye stalks and the cyclops sit much higher than the human face.
            # Use recipe framing instead of cropping their expression off.
            face_z = {"Glibbo": 1.4, "Zorp": 1.4, "Wobbel": 1.53}.get(o.name, top - 0.15)
            face = (o.location.x, 0, face_z)
            shoot(f"{d}/face_{o.name.lower()}.png", (o.location.x + 0.45, -1.3, face[2] + 0.05), face,
                  lens=50, size=(800, 800))
        shoot(f"{d}/at_60m_day.png", (0, -60, 1.0), (0, 0, 1.0), fov_deg=80, size=(1920, 1080))
        # A game-like view: 80 degree field of view, eye height, 30 m away, night.
        bpy.data.objects.remove(ground)
        for light in [o for o in bpy.data.objects if o.type == "LIGHT"]:
            bpy.data.objects.remove(light)
        stage(night=True)
        bpy.context.scene.cycles.samples = max(1, args.samples)
        shoot(f"{d}/at_30m_night.png", (0, -30, 1.0), (0, 0, 1.0), fov_deg=80, size=(1920, 1080))


if __name__ == "__main__":
    main()
