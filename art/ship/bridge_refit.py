"""Expedition bridge study; metres, bow +X, up +Z.

Live: run this file through MCP. Builds a separate scene beside the old study.
Headless: blender -b -t 4 --python-exit-code 1 -P art/ship/bridge_refit.py -- --render
The source hull is rebuilt from Python when absent. Outputs stay in build/ship-refit.
Display graphics are composition studies, not connected game instruments.
"""
from pathlib import Path
import argparse
import hashlib
import json
import math
import sys

import bpy
from mathutils import Vector

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.modules.pop('bridge', None)
import bridge as br

th = br.th
PREFIX = 'Bridge refit'
OUT = HERE.parents[1] / 'build/ship-refit'
original_build = br.build


def box(name, p, size, mat, bevel=.025):
    return th.cube(name, p, size, mat, bevel=bevel)


def sign(body, p, size, mat, rotation=(math.pi/2, 0, math.pi/2)):
    return th.label(body, p, size, mat, th.kit, rotation)


def remove_parts(stems):
    # Only this newly generated variant owns these objects.
    for o in list(th.kit.objects):
        if any(o.name.startswith(PREFIX+' '+s) for s in stems):
            bpy.data.objects.remove(o, do_unlink=True)


def chair(name, u, y, z, yaw, m):
    """A floor-mounted shell, separate upholstery and exposed suspension."""
    c, s = math.cos(yaw), math.sin(yaw)

    def part(label, offset, size, mat, bevel=.035):
        x_, y_, z_ = offset
        o = box(name+' '+label, (br.X(u)+c*x_-s*y_, y+s*x_+c*y_, z+z_), size, mat, bevel)
        o.rotation_euler.z = yaw
        return o

    part('foot', (0, 0, .055), (.65, .64, .11), m['steel'])
    part('suspension', (-.13, 0, .28), (.20, .30, .38), m['metal'])
    part('seat shell', (0, 0, .48), (.70, .71, .14), m['teal'])
    part('seat pad', (.025, 0, .565), (.59, .60, .10), m['cloth'])
    back = part('back shell', (-.32, 0, .99), (.15, .72, .91), m['teal'])
    back.rotation_euler.y = -.12
    pad = part('back pad', (-.22, 0, 1.02), (.095, .58, .72), m['cloth'])
    pad.rotation_euler.y = -.12
    part('headrest', (-.39, 0, 1.53), (.22, .43, .24), m['cloth'])
    for side in (-1, 1):
        part('arm support', (-.13, side*.41, .69), (.11, .09, .37), m['metal'])
        part('armrest', (.03, side*.41, .86), (.55, .14, .10), m['steel'])
        part('harness', (-.155, side*.19, 1.01), (.025, .045, .63), m['ochre'], .005)


def flank_console(side, m):
    y = side*5.20
    # The continuous casing reaches the wall; the workstation is part of the ship.
    box('Flank service plinth', (br.X(3.0), side*5.58, .15), (3.70, 1.55, .30), m['steel'])
    box('Flank equipment casing', (br.X(3.0), side*5.65, .62), (3.65, 1.40, 1.10), m['teal'], .06)
    box('Flank worktop', (br.X(3.0), y, 1.02), (3.78, 1.32, .13), m['lining'], .05)
    box('Flank screen spine', (br.X(3.0), side*5.57, 1.61), (3.72, .30, 1.04), m['steel'])
    box('Flank canopy', (br.X(3.0), side*5.28, 2.14), (3.85, 1.10, .18), m['teal'], .06)
    box('Flank task diffuser', (br.X(3.0), side*4.95, 2.035), (3.35, .06, .035), m['task'], .008)
    rot = (math.pi/2, 0, 0 if side > 0 else math.pi)
    for k, u in enumerate((1.88, 3.12, 4.12)):
        width = .98 if k < 2 else .65
        box('Flank monitor bezel', (br.X(u), side*5.385, 1.60), (width+.09, .11, .66), m['metal'])
        box('Flank display glass', (br.X(u), side*5.32, 1.60), (width, .025, .56), m['screen'], .012)
        for row in range(4):
            box('Display trace', (br.X(u)-width*.30+row*.035, side*5.300, 1.73-row*.09),
                (width*(.55 if row % 2 else .72), .008, .012), m['cyan'], .002)
        sign(('NAV / 02' if side > 0 else 'SURVEY / 03') if k == 0 else 'SYSTEMS',
             (br.X(u), side*5.29, 1.97), .085, m['cream'], rot)
        for j in range(4):
            box('Recessed key', (br.X(u)-.22+j*.14, side*4.97, 1.104), (.09, .13, .025),
                m['ochre'] if j == 0 else m['steel'], .008)
    for u in (1.55, 2.35, 3.15, 3.95, 4.55):
        box('Service panel', (br.X(u), side*4.937, .60), (.57, .045, .62), m['metal'])
        box('Service handle', (br.X(u), side*4.90, .82), (.20, .045, .035), m['cream'], .008)
    chair('Flank chair', 2.8, side*4.12, 0, side*math.pi/2, m)
    th.area_light('Flank work light', (br.X(3), side*4.9, 2.01), (br.X(3), side*4.7, .8),
                  (.61, .83, 1), 90, 2.3)


def build_refit(m):
    m.update(metal=th.material('machined aluminium', (.23, .28, .29), .7),
             cloth=th.material('woven charcoal pads', (.035, .048, .05)),
             screen=th.material('inactive instrument glass', (.009, .027, .031), .15),
             cyan=th.material('instrument phosphor', (.20, .70, .75), emission=1.4),
             task=th.material('task diffuser', (.65, .83, .91), emission=2.2))
    original_build(m)
    remove_parts(('Grey placeholder', 'Display zone', 'Globe', 'Captain screen', 'Screen arm',
                  'Floor glow', 'Tier seam', 'Ramp stripe', 'Dome cove', 'Cage post cap'))
    for o in list(th.review.objects):
        if o.type == 'LIGHT':
            bpy.data.objects.remove(o, do_unlink=True)
    # Rear glazing becomes framed panels and a high, narrow viewing band.
    for o in list(th.kit.objects):
        if o.name.startswith(PREFIX+' Pod glass'):
            xs = [v.co.x for v in o.data.vertices]
            if max(xs) <= br.X(4.6)+.001:
                bpy.data.objects.remove(o, do_unlink=True)
    for side in (-1, 1):
        for a, b in ((0, 2.28), (2.28, 4.6)):
            pa = Vector((br.X(a), side*br.hw(a), 0))
            pb = Vector((br.X(b), side*br.hw(b), 0))
            span = pb-pa
            length = span.length
            yaw = math.atan2(span.y, span.x)
            for name, lo, hi, material in (
                ('Solid flank', 0, 1.82, m['lining']),
                ('Upper flank', 2.34, 2.80, m['teal'])):
                o = box(name, (*((pa+pb)/2)[:2], (lo+hi)/2), (length, .22, hi-lo), material)
                o.rotation_euler.z = yaw
            br.mesh_obj('Rear band glazing', [(pa.x, pa.y, 1.82), (pb.x, pb.y, 1.82),
                       (pb.x, pb.y, 2.34), (pa.x, pa.y, 2.34)], [(0, 1, 2, 3)], m['glass'])
            for z in (1.82, 2.34):
                th.beam('Band frame', (pa.x, pa.y, z), (pb.x, pb.y, z), .09, .10, m['steel'])
        flank_console(side, m)
        # Shoulder frames give the panoramic nose a visible load path.
        th.beam('Nose shoulder', (br.X(4.72), side*5.9, .02),
                (br.X(5.00), side*5.9, 2.9), .34, .32, m['teal'])
        box('Shoulder service collar', (br.X(4.76), side*5.9, .64), (.49, .46, .38), m['metal'])
        # Cables arrive above the door and terminate at the equipment banks.
        for j in range(3):
            yy = side*(2.75+j*.14)
            th.pipe('Signal trunk', (br.X(-.12), yy, 2.65), (br.X(1.05), yy, 2.65), .045,
                    m['ochre'] if j == 0 else m['metal'])
            th.pipe('Signal turn', (br.X(1.05), yy, 2.65), (br.X(1.05), side*(5.15+j*.14), 2.65),
                    .045, m['ochre'] if j == 0 else m['metal'])
        box('Service trunk cover', (br.X(.75), side*3.95, 2.80), (.62, 3.10, .19), m['teal'])
        for u in (2.00, 3.6):
            box('Overhead task housing', (br.X(u), side*2.65, 3.12), (1.10, .36, .12), m['steel'])
            box('Overhead task lens', (br.X(u), side*2.65, 3.05), (.89, .22, .025), m['amber'])
        # Low cockpit coamings frame the view; instrument faces remain below the horizon.
        box('Pilot coaming foot', (br.X(7.15), side*1.07, br.NEST+.19), (1.35, .65, .38), m['steel'])
        box('Pilot coaming', (br.X(7.15), side*1.07, br.NEST+.58), (1.48, .67, .43), m['teal'], .07)
        top = box('Pilot panel', (br.X(7.24), side*1.07, br.NEST+.83), (1.02, .53, .075), m['metal'])
        top.rotation_euler.y = -.14
        face = box('Pilot display', (br.X(7.24), side*1.07, br.NEST+.875), (.72, .42, .018), m['screen'])
        face.rotation_euler.y = -.14
        for k in range(3):
            box('Pilot indicator', (br.X(7.03+k*.13), side*1.07, br.NEST+.91), (.045, .24, .013), m['cyan'], .003)
        th.beam('Pilot grabrail', (br.X(6.60), side*.72, br.NEST+.70),
                (br.X(7.60), side*.72, br.NEST+.70), .045, .045, m['metal'])
    chair('Command chair', br.SEAT_U-.15, 0, .30, 0, m)
    chair('Pilot chair', 6.68, 0, br.NEST, 0, m)
    for side in (-1, 1):
        box('Command arm console', (br.X(br.SEAT_U+.27), side*.68, 1.02), (.95, .34, .36), m['teal'])
        box('Command arm screen', (br.X(br.SEAT_U+.39), side*.68, 1.21), (.45, .25, .025), m['screen'])
        box('Command status', (br.X(br.SEAT_U+.39), side*.68, 1.227), (.31, .04, .008), m['cyan'], .002)
    box('Pilot control pedestal', (br.X(7.35), 0, br.NEST+.40), (.24, .28, .8), m['steel'])
    th.beam('Pilot yoke', (br.X(7.20), -.30, br.NEST+.84), (br.X(7.20), .30, br.NEST+.84), .07, .07, m['metal'])
    for y in (-.30, .30):
        box('Pilot grip', (br.X(7.17), y, br.NEST+.90), (.12, .095, .19), m['cloth'])
    # Solid, flush walking surface beneath both seats; glass remains beside the pilot.
    br.sloped('Pilot access tread', br.X(5.88), br.X(6.8), 0, 1.35, br.zf(5.88)-.025, br.NEST-.025, .05, m['steel'])
    box('Pilot seat tread', (br.X(7.35), 0, br.NEST-.025), (1.10, 1.35, .05), m['steel'])
    # Wayfinding belongs to the threshold, with a short covered arrival space.
    for y in br.DOORS:
        box('Threshold soffit', (br.X(.48), y, 2.55), (1.25, 2.04, .23), m['teal'])
        box('Threshold tread', (br.X(.48), y, .012), (1.24, 1.78, .025), m['steel'], .008)
        for x in (.03, .22, .41, .60, .79, .98):
            box('Threshold grip', (br.X(x), y, .029), (.025, 1.58, .008), m['metal'], .003)
        box('Door identity', (br.X(.10), y, 2.30), (.07, 1.20, .18), m['steel'])
        sign('01 / BRIDGE', (br.X(.145), y, 2.30), .11, m['cream'])
        sign('BRIDGE / 01', (br.X(-.08), y, 2.32), .12, m['cream'], (math.pi/2, 0, -math.pi/2))
    box('Aft equipment rack', (br.X(.28), 0, 1.18), (.40, 1.44, 2.2), m['steel'])
    for z in (.45, .82, 1.19, 1.56):
        box('Aft rack cartridge', (br.X(.51), 0, z), (.08, 1.24, .27), m['metal'])
        for y in (-.46, .46):
            box('Rack latch', (br.X(.56), y, z), (.04, .06, .16), m['ochre'], .008)
    sign('AUX / LOCAL', (br.X(.57), 0, 1.95), .12, m['cream'])
    th.area_light('Command downlight', (br.X(3), 0, 3.25), (br.X(3), 0, .3), (1, .78, .51), 330, 2.4)
    th.area_light('Aft bounce', (br.X(.8), 0, 2.4), (br.X(3), 0, 1.1), (.72, .82, 1), 150, 2.5)
    th.area_light('Window daylight', (br.X(9.2), 0, 3.1), (br.X(4), 0, .3), (.48, .67, 1), 700, 5)


def plan(m):
    scene = bpy.data.scenes.new(PREFIX+' | Plan')
    wanted = ('Floor strip', 'Glass floor', 'Captain tier', 'Pilot', 'Command', 'Flank', 'Threshold', 'Solid flank')
    for o in th.kit.objects:
        if any(o.name.startswith(PREFIX+' '+stem) for stem in wanted):
            scene.collection.objects.link(o)
    scene.camera = th.camera('Plan', (br.X(4), 0, 24), (br.X(4), 0, 0), ortho=15, scene=scene)
    scene.render.engine = 'BLENDER_WORKBENCH'
    scene.display.shading.color_type = 'MATERIAL'
    scene.render.resolution_x = 1200
    scene.render.resolution_y = 1000
    return scene


def validate():
    """Conservative mesh bounds protect the two door approaches and walking lanes."""
    problems = []
    hashes = []
    for o in th.kit.objects:
        if o.type != 'MESH':
            continue
        points = [o.matrix_world @ Vector(v) for v in o.bound_box]
        lo = [min(p[i] for p in points) for i in range(3)]
        hi = [max(p[i] for p in points) for i in range(3)]
        for y in br.DOORS:
            # Floor and high soffits are intentional; all standing-height solids count.
            if hi[2] <= .04 or lo[2] >= 2.20:
                continue
            if lo[0] < br.X(1.0) and hi[0] > br.X(.20) and lo[1] < y+.87 and hi[1] > y-.87:
                problems.append('Door approach blocked: '+o.name)
            if lo[0] < br.X(4.6) and hi[0] > br.X(1.0) and lo[1] < y+.35 and hi[1] > y-.35:
                problems.append('Walking lane blocked: '+o.name)
        vertices = [tuple(round(x, 6) for x in o.matrix_world @ v.co) for v in o.data.vertices]
        if any(not math.isfinite(x) for v in vertices for x in v):
            problems.append('Non-finite vertex: '+o.name)
        payload = (vertices, [tuple(p.vertices) for p in o.data.polygons],
                   [(mod.type, round(mod.width, 6), mod.segments) for mod in o.modifiers if mod.type == 'BEVEL'])
        hashes.append(hashlib.sha256(repr(payload).encode()).hexdigest())
    report = {'problems': sorted(set(problems)), 'kit_meshes': len(hashes),
              'geometry_hash': hashlib.sha256(''.join(sorted(hashes)).encode()).hexdigest(),
              'door_centres_y': br.DOORS, 'door_clear_size': [1.8, 2.2],
              'walking_lane_checked_width': .7,
              'scope': 'New kit bounds only; no collision or movement test in game.'}
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT/('validation-headless.json' if bpy.app.background else 'validation-live.json')).write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report))
    if problems:
        raise RuntimeError('Refit geometry validation failed')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--render', action='store_true')
    parser.add_argument('--replace', action='store_true')
    parser.add_argument('--save', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else [])
    if br.SOURCE not in bpy.data.scenes:
        import starting_ship
        starting_ship.build()
    br.PREFIX, br.OUT = PREFIX, OUT
    br.build, br.make_plan = build_refit, plan
    saved_args = sys.argv
    sys.argv = [__file__, '--'] + (['--replace'] if args.replace else [])
    try:
        br.main()
    finally:
        sys.argv = saved_args
    scene = bpy.data.scenes[PREFIX+' | Architecture']
    pilot = next(o for o in scene.objects if o.type == 'CAMERA' and o.name.startswith(PREFIX+' 04 Pilot'))
    pilot.location = (br.X(6.78), 0, br.NEST+1.25)
    pilot.rotation_euler = (Vector((br.X(14), 0, br.NEST+1.25))-pilot.location).to_track_quat('-Z', 'Y').to_euler()
    pilot.data.lens = 25
    for o in th.review.objects:
        if o.name.startswith(PREFIX+' star'):
            o.scale *= .14
    scene.world.node_tree.nodes['Background'].inputs[1].default_value = .24
    scene['status'] = 'Art study only. Display graphics are static; no game integration.'
    OUT.mkdir(parents=True, exist_ok=True)
    bpy.context.view_layer.update()
    validate()
    if args.render:
        for prefix, filename in [('01 Door', '01-entry'), ('02 Overview', '02-aft'), ('04 Pilot', '03-pilot')]:
            scene.camera = next(o for o in scene.objects if o.type == 'CAMERA' and o.name.startswith(PREFIX+' '+prefix))
            scene.render.filepath = str(OUT/(filename+'.png'))
            bpy.ops.render.render(write_still=True)
    scene.camera = next(o for o in scene.objects if o.type == 'CAMERA' and o.name.startswith(PREFIX+' 01 Door'))
    if args.save:
        destination = OUT/('bridge-refit-rebuilt.blend' if bpy.app.background else 'bridge-refit-live.blend')
        if destination.exists():
            raise RuntimeError('Inspect the existing saved study before choosing another output path: '+str(destination))
        bpy.ops.wm.save_as_mainfile(filepath=str(destination), compress=True)
    print('REFIT READY: '+str(OUT))


if __name__ == '__main__':
    main()
