"""The airlock's wall panel (#308, #309): steel body, proud lip, black glass backing.

Design: Claude Design "Airlock Wall Panel 3D" (2026-10-17), measures in metres. The panel's
origin is the centre of the glass at the wall's surface; the panel faces +Z in the game (glTF),
which is -Y here (the exporter turns Blender's -Y into glTF's +Z). The screen (a live texture,
0.20 x 0.32 m) is not in the model: the game puts it in front of the glass at z = +0.003.
The body is 8 cm deep and sinks into the wall; only the lip stands proud of the wall surface.

Run headless (writes content/ship/airlock_wall_panel.glb, renders optional):
    blender -b -P art/ship/airlock_wall_panel.py -- --out content/ship [--renders DIR]
"""

import argparse
import os
import sys

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from mesh_checks import orientation_problems  # noqa: E402

# (width, height, depth, centre x, centre z-in-spec) in metres; spec z is out of the wall.
GLASS = (0.20, 0.32, 0.002, 0.001)
BODY = (0.225, 0.345, 0.08, -0.045)
LIP_OUTER = (0.25, 0.37)
LIP_INNER = (0.20, 0.32)
LIP_THICK = 0.006
LIP_Z = 0.003
TRIANGLE_BUDGET = 100

STEEL = ("steel_frame", (0x9A / 255, 0xA8 / 255, 0xB2 / 255), 0.45, 0.35)
GLASS_BLACK = ("glass_black", (0x03 / 255, 0x05 / 255, 0x07 / 255), 0.15, 0.0)


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def material(spec):
    name, rgb, rough, metal = spec
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    lin = tuple(srgb_to_linear(c) for c in rgb)
    bsdf.inputs["Base Color"].default_value = (*lin, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    m.diffuse_color = (*lin, 1.0)
    return m


def box(bm, w, h, d, cx, cy, spec_z):
    """A box w (x) by h (up) by d (out of the wall) centred at spec z; Blender y = -spec z."""
    res = bmesh.ops.create_cube(bm, size=1.0)
    for v in res["verts"]:
        v.co = Vector((cx + v.co.x * w, -(spec_z + v.co.y * d), cy + v.co.z * h))
    # Negating y mirrors the box: turn its faces back outwards.
    bmesh.ops.reverse_faces(bm, faces=list({f for v in res["verts"] for f in v.link_faces}))
    return res["verts"]


def part(name, mat, boxes):
    bm = bmesh.new()
    for b in boxes:
        box(bm, *b)
    bm.normal_update()
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def build():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    steel, black = material(STEEL), material(GLASS_BLACK)
    ow, oh = LIP_OUTER
    iw, ih = LIP_INNER
    side = (ow - iw) / 2  # 0.025 m on every side
    lip = [
        (side, oh, LIP_THICK, -(iw + side) / 2, 0.0, LIP_Z),
        (side, oh, LIP_THICK, (iw + side) / 2, 0.0, LIP_Z),
        (iw, side, LIP_THICK, 0.0, (ih + side) / 2, LIP_Z),
        (iw, side, LIP_THICK, 0.0, -(ih + side) / 2, LIP_Z),
    ]
    part("airlock_panel_frame", steel, [(BODY[0], BODY[1], BODY[2], 0.0, 0.0, BODY[3])] + lip)
    part("airlock_panel_glass", black, [(GLASS[0], GLASS[1], GLASS[2], 0.0, 0.0, GLASS[3])])


def report():
    problems = 0
    tris = 0
    for obj in bpy.data.objects:
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        tris += sum(len(f.verts) - 2 for f in bm.faces)
        for msg in orientation_problems(bm):
            print(f"PROBLEM {obj.name}: {msg}")
            problems += 1
        bm.free()
    print(f"airlock_wall_panel: {tris} triangles (guide {TRIANGLE_BUDGET})")
    if tris > TRIANGLE_BUDGET:
        print(f"PROBLEM airlock_wall_panel: over the triangle guide ({tris} > {TRIANGLE_BUDGET})")


def render(directory):
    import math
    os.makedirs(directory, exist_ok=True)
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.light = "STUDIO"
    scene.display.shading.color_type = "MATERIAL"
    scene.render.resolution_x, scene.render.resolution_y = 480, 480
    world = bpy.data.worlds.new("w")
    world.color = (0.23, 0.27, 0.31)  # wall paint, so the lip reads against a wall
    scene.world = world
    # A stand-in wall behind the panel, only in the renders (the game's wall is there).
    wall = bpy.data.objects.new("wall", bpy.data.meshes.new("wall"))
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector((v.co.x * 0.8, -(-0.15 + v.co.y * 0.3), v.co.z * 0.8))
    bm.to_mesh(wall.data)
    bm.free()
    wall.data.materials.append(bpy.data.materials.new("wall_paint"))
    wall.data.materials[0].diffuse_color = (0.04, 0.06, 0.08, 1.0)
    # Cut out the opening is not needed: the body hides inside the stand-in wall, only the lip shows.
    scene.collection.objects.link(wall)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = 0.6
    scene.collection.objects.link(cam)
    scene.camera = cam
    views = {
        "front": ((0.0, -1.0, 0.0), 0.0, 0.5),
        "three_quarter": ((0.6, -0.8, 0.5), 0.0, 0.5),
        "side": ((1.0, 0.0, 0.0), 0.0, 0.0),
    }
    for name, (direction, _, _) in views.items():
        d = Vector(direction).normalized()
        cam.location = d * 2.0
        cam.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
        scene.render.filepath = os.path.join(directory, f"airlock_wall_panel_{name}.png")
        # the side view shows the body only without the stand-in wall
        wall.hide_render = name == "side"
        bpy.ops.render.render(write_still=True)
    wall.hide_render = True
    bpy.data.objects.remove(wall)


def export(out):
    os.makedirs(out, exist_ok=True)
    path = os.path.join(out, "airlock_wall_panel.glb")
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", export_apply=True, export_yup=True)
    print("wrote", path)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="content/ship")
    ap.add_argument("--renders")
    args = ap.parse_args(argv if bpy.app.background else [])
    build()
    report()
    export(args.out)
    if args.renders:
        render(args.renders)


if __name__ == "__main__":
    main()
