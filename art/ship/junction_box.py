"""A junction box (#322): a wall cabinet that feeds the consumer groups of its area.

Source: decision "Junction boxes: repair objects, routing central" (2026-10-18) in the concept
repo. The box is mainly a repair object: no screen, one toggle with a lamp per consumer group of
its area, a fault lamp, and a door over the bus bars that opens for the repair.
The limits per group live on the central power screen in the engine room, not here. The ship
has three boxes with two groups each (JB-1 engines + jump drive, JB-2 life support + lights,
JB-3 shields + weapons), so one model with two toggles serves all three; the labels say which.

Frame: metres, origin on the wall surface at floor height under the box's centre line, Blender
Z up. The box faces -Y here, which the exporter turns into +Z in the game (glTF): Blender
(x, y, z) is glTF (x, z, -y). Moving parts have their origin at their pivot; lamps and texture
quads (label_*) carry their own material. Nodes, pivots and travel: `junction_box.md`.

Run headless (writes content/ship/junction_box.glb and the working file
build/ship/junction_box.blend, renders optional):
    blender -b --python-exit-code 1 -P art/ship/junction_box.py -- --out content/ship [--renders DIR]
Live (open Blender over the MCP; builds into the collections `junction_box`, `reference`):
    import runpy, sys; sys.modules.pop("reactor", None); runpy.run_path("<repo>/art/ship/junction_box.py", run_name="__main__")
"""

import argparse
import math
import os
import sys

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import reactor  # noqa: E402  (shares the mesh helpers and the ship palette)
from reactor import Mesh, fresh_collection, frame, material, quad_obj, scale_figure, triangles  # noqa: E402

TRIANGLE_BUDGET = 2500

# Cabinet: width, height, depth off the wall; bottom edge above the floor.
BODY = (0.56, 0.84, 0.17)
BODY_Z = 0.92
GROUPS = 2

# Door over the bus bars, upper part of the front, hinged on its left edge (vertical axis).
DOOR = (0.48, 0.40)                 # width, height
DOOR_Z = 1.32                       # bottom edge
DOOR_T = 0.022

# Control strip under the door: the fault lamp in the middle, then the toggles row. No load gauge:
# the load per group is on the central power screen (initiator, 2026-10-18: "ja raus damit").
FAULT_Z = 1.225
TOGGLE_Z = 1.06
TOGGLE_DX = 0.13                    # each toggle's x from the centre line

reactor.MATERIALS.update({
    "jb_hull": ("6f7a80", 0.55, 0.35, 0.0, 1.0),
    "jb_door": ("8a949a", 0.5, 0.4, 0.0, 1.0),
    "jb_bus": ("b87333", 0.35, 0.9, 0.0, 1.0),
    "preview_wall": ("39424a", 0.8, 0.0, 0.0, 1.0),
})


def front(z, x=0.0, out=0.0):
    """A frame on the cabinet's front at height z, local -Y out of the front."""
    return frame((x, -(BODY[2] + out), z))


def build_body(coll):
    w, h, d = BODY
    b = Mesh()
    # Shell as a box open at the door: back, sides, top, bottom and the lower front.
    t = 0.015
    b.box("jb_hull", (w - 2 * t + 0.004, t, h), (0, -t / 2, BODY_Z + h / 2))       # back plate, overlaps the sides
    for x in (-w / 2 + t / 2, w / 2 - t / 2):
        b.box("jb_hull", (t, d, h), (x, -d / 2, BODY_Z + h / 2))                   # sides
    for z in (BODY_Z + t / 2, BODY_Z + h - t / 2):
        b.box("jb_hull", (w - 2 * t - 0.002, d, t), (0, -d / 2, z))                # bottom, top
    # Parts overlap instead of sharing corners: shared corners merge into nonmanifold edges.
    low0 = BODY_Z + t / 2
    low_h = DOOR_Z - 0.02 - low0
    b.box("jb_hull", (w - 2 * t - 0.002, t, low_h), (0, -d + t / 2, low0 + low_h / 2))  # lower front
    # Door frame: a lip round the opening, so the closed door sits in it.
    top = BODY_Z + h
    lip = 0.03
    dw, dh = DOOR
    for x in (-dw / 2 - lip / 2, dw / 2 + lip / 2):
        b.box("reactor_dark", (lip, t, dh + 2 * lip), (x, -d + t / 2, DOOR_Z + dh / 2))
    b.box("reactor_dark", (dw + 0.002, t, top - (DOOR_Z + dh) - t), (0, -d + t / 2, (DOOR_Z + dh + top - t) / 2))
    # Mounting rails on the wall, wider than the box, and the conduits into the ceiling and floor.
    for z in (BODY_Z + 0.08, BODY_Z + h - 0.08):
        b.box("reactor_steel", (w + 0.12, 0.03, 0.05), (0, -0.015, z))
    for x in (-0.14, 0.0, 0.14):
        b.cyl("reactor_ring", 0.028, 0.50, (x, -0.06, top + 0.25), seg=10)
        b.cyl("reactor_ring", 0.036, 0.05, (x, -0.06, top + 0.02), seg=10)        # gland
    b.cyl("reactor_ring", 0.035, BODY_Z, (0.0, -0.07, BODY_Z / 2), seg=10)        # feed from below
    # Inside, behind the door: three copper bus bars and two breaker modules (seen when open).
    for k in range(3):
        b.box("jb_bus", (dw - 0.06, 0.012, 0.025), (0, -0.03, DOOR_Z + 0.08 + k * 0.06))
    for x in (-0.11, 0.11):
        b.box("reactor_dark", (0.12, 0.07, 0.14), (x, -0.05, DOOR_Z + dh - 0.11))
        b.box("hazard_yellow", (0.10, 0.004, 0.03), (x, -0.087, DOOR_Z + dh - 0.06))
    # Toggle bezels on the lower front and the fault lamp's bezel.
    for i in range(GROUPS):
        x = (i - (GROUPS - 1) / 2) * 2 * TOGGLE_DX
        b.box("reactor_dark", (0.08, 0.012, 0.11), (x, -d - 0.006, TOGGLE_Z))
    b.cyl("reactor_dark", 0.045, 0.02, (0, -d - 0.01, FAULT_Z), axis="Y", seg=16)
    # Hazard band along the bottom edge of the front.
    b.box("hazard_yellow", (w - 0.04, 0.004, 0.025), (0, -d - 0.002, BODY_Z + 0.03))
    b.finish("junction_box_body", coll)


def build_door(coll):
    dw, dh = DOOR
    d = BODY[2]
    m = Mesh()
    # Built in the hinge's frame: the door runs along +X from the hinge on its left edge.
    m.box("jb_door", (dw, DOOR_T, dh), (dw / 2, DOOR_T / 2, dh / 2))
    # Vent slots and a handle on the right edge.
    for k in range(5):
        m.box("reactor_dark", (dw * 0.55, 0.004, 0.012), (dw * 0.45, -0.001, dh * 0.42 + k * 0.035))
    m.box("reactor_steel", (0.025, 0.03, 0.12), (dw - 0.045, -0.015, dh * 0.5))
    for z in (0.08, dh - 0.08):
        m.cyl("reactor_steel", 0.012, 0.05, (0.0, DOOR_T / 2, z), seg=8)          # hinge knuckles
    m.finish("door", coll, Matrix.Translation((-dw / 2, -d, DOOR_Z)))


def build_controls(coll):
    d = BODY[2]
    for i in range(GROUPS):
        x = (i - (GROUPS - 1) / 2) * 2 * TOGGLE_DX
        n = i + 1
        t = Mesh()
        t.cyl("reactor_steel", 0.010, 0.07, (0, -0.035, 0), axis="Y", seg=8)
        t.cyl("rubber_black", 0.016, 0.03, (0, -0.07, 0), axis="Y", seg=8)
        # Rest pose OFF: bat tipped 25° down about local X; ON is -50° from rest, bat up.
        t.finish(f"toggle_{n}", coll, front(TOGGLE_Z, x, 0.012) @ Matrix.Rotation(math.radians(25), 4, "X"))
        material(f"lamp_group_{n}", "lamp_off")
        lamp = Mesh()
        lamp.cyl(f"lamp_group_{n}", 0.018, 0.02, (0, -0.01, 0), axis="Y", seg=12)
        lamp.finish(f"lamp_group_{n}", coll, front(TOGGLE_Z + 0.08, x))
        quad_obj(f"label_group_{n}", coll, 0.12, 0.035, front(TOGGLE_Z - 0.085, x, 0.001))
    material("lamp_fault", "lamp_off")
    f = Mesh()
    f.cyl("lamp_fault", 0.032, 0.03, (0, -0.015, 0), axis="Y", seg=16)
    f.finish("lamp_fault", coll, front(FAULT_Z, 0.0, 0.02))
    quad_obj("label_box", coll, 0.30, 0.05, front(BODY_Z + BODY[1] - 0.045, 0.0, 0.001))
    # Where the game spawns sparks and fire: the door's centre, just in front of it.
    spark = bpy.data.objects.new("spark_point", None)
    spark.empty_display_size = 0.05
    spark.matrix_basis = front(DOOR_Z + DOOR[1] / 2, 0.0, 0.03)
    coll.objects.link(spark)


def build():
    # Live, a factory reset would also drop the MCP add-on: clear only our own collections.
    if bpy.app.background:
        bpy.ops.wm.read_factory_settings(use_empty=True)
    coll = fresh_collection("junction_box")
    build_body(coll)
    build_door(coll)
    build_controls(coll)
    return coll


REQUIRED = ["junction_box_body", "door", "lamp_fault", "label_box", "spark_point"] + [
    f"{p}_{i + 1}" for p in ("toggle", "lamp_group", "label_group") for i in range(GROUPS)]


def report(coll):
    tris = triangles(coll)
    print(f"junction_box: {len(coll.objects)} objects, {tris} triangles (guide {TRIANGLE_BUDGET})")
    if tris > TRIANGLE_BUDGET:
        print(f"PROBLEM junction_box: over the triangle guide ({tris} > {TRIANGLE_BUDGET})")
    names = set(coll.objects.keys())
    for need in REQUIRED:
        if need not in names:
            print(f"PROBLEM junction_box: node {need} missing")


def dress(coll):
    """Review extras, never exported: a stand-in wall and a 1.80 m figure."""
    w = Mesh()
    w.box("preview_wall", (2.4, 0.1, 2.6), (0, 0.05, 1.3))
    w.box("reactor_dark", (2.4, 1.6, 0.02), (0, -0.8, -0.01))
    w.finish("preview_wall", coll)
    scale_figure(coll, (-0.75, -0.45, 0.0))


def render(directory):
    os.makedirs(directory, exist_ok=True)
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x, scene.render.resolution_y = 700, 700
    scene.view_settings.view_transform = "AgX"
    world = bpy.data.worlds.new("w")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.05, 0.06, 0.07, 1.0)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 2.0
    scene.world = world
    for name, energy, rot in (("sun", 2.5, (50, 10, -30)), ("fill", 1.0, (70, 0, 150))):
        light = bpy.data.objects.new(name, bpy.data.lights.new(name, "SUN"))
        light.data.energy = energy
        light.rotation_euler = tuple(math.radians(a) for a in rot)
        scene.collection.objects.link(light)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    scene.collection.objects.link(cam)
    scene.camera = cam
    views = {
        "three_quarter": ((1.4, -1.9, 1.7), (0, -0.1, 1.3), 35),
        "front_eye": ((0.0, -1.6, 1.65), (0, 0, 1.3), 35),
        "side": ((1.8, -0.1, 1.4), (0, -0.1, 1.3), 40),
    }
    for name, (pos, look, lens) in views.items():
        cam.location = pos
        cam.data.lens = lens
        cam.rotation_euler = (Vector(look) - Vector(pos)).to_track_quat("-Z", "Y").to_euler()
        scene.render.filepath = os.path.join(directory, f"junction_box_{name}.png")
        bpy.ops.render.render(write_still=True)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="content/ship")
    ap.add_argument("--renders")
    ap.add_argument("--blend", default="build/ship/junction_box.blend")
    args = ap.parse_args(argv if bpy.app.background else [])
    coll = build()
    report(coll)
    if bpy.app.background:
        os.makedirs(args.out, exist_ok=True)
        reactor.export(coll, os.path.join(args.out, "junction_box.glb"))
        # The .blend is a working file to open by hand, saved headless only: saving from the
        # live session has crashed Blender 5.2.1.
        os.makedirs(os.path.dirname(os.path.abspath(args.blend)), exist_ok=True)
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(args.blend))
        if args.renders:
            dress(bpy.context.scene.collection)
            render(args.renders)
    else:
        dress(fresh_collection("reference"))


if __name__ == "__main__":
    main()
