"""Lower deck study v2: one wide machine deck (cargo bay + reactor hall), against the live ship.

The lower hull is a hopper: measured inside, the deck is +-4.3 m wide at floor level and +-6.8 m under
the ceiling, because the walls lean outward. v1 used only the middle 6.5 m. Here the sloped hull walls
are part of the room: frames, tie-down rails and pipe runs follow the slope. The hub ladder lands in
the middle of the deck; through a heavy bulkhead arch the glowing reactor core shows at the far end.
Hot side (turbine, amber) and cold side (coolant, cyan) frame the core. Machines are placeholders;
the mechanics (fuel rods, levers, overheat) live on lane/engineer. Reuses turret_hub helpers.
Live: runpy.run_path(..., run_name='__main__'); park the window first (see bridge.py notes).
Headless: blender -b -t 4 <file with the source ship> -P art/ship/lower_deck.py -- --replace --render
"""
from pathlib import Path
import argparse
import json
import math
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.modules.pop('turret_hub', None)
import turret_hub as th

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'build/lower-deck'
PREFIX = 'Lower study'
SOURCE = th.SOURCE

Z_FLOOR, Z_PLINTH, CEIL = -2.90, -2.50, -.24
HW0, HW1 = 4.30, 6.80                 # inner half width at the floor and under the ceiling (measured)
XA, XF = -7.92, 0.0                   # aft bulkhead (doorway to the ramp bay) and front wall
ARCH_X = -2.90                        # the bulkhead frame between cargo bay and reactor hall
CORE = (-1.45, 0.0)
LADDER = (-4.83, 0.0)
HOLE = (-5.04, -3.36, -.84, .84)      # the hub's ladder opening in the ceiling
SLOPE = math.atan2(HW1-HW0, CEIL-Z_FLOOR)
SLOPE_L = math.hypot(HW1-HW0, CEIL-Z_FLOOR)
DROPPED = ('Reactor plinth', 'Plinth nosing', 'Reactor hall floor', 'Reactor step tread', 'Step nosing',
           'Cargo bay liner', 'Reactor protective liner', 'Cargo floor')


def hw_at(z):
    return HW0+(z-Z_FLOOR)*(HW1-HW0)/(CEIL-Z_FLOOR)


def wall_pt(s, t):
    return (s*(HW0+t*(HW1-HW0)), Z_FLOOR+t*(CEIL-Z_FLOOR))


def mesh_obj(name, verts, faces, mat):
    d = bpy.data.meshes.new(PREFIX+' '+name)
    d.from_pydata(verts, [], faces)
    d.materials.append(mat)
    o = bpy.data.objects.new(PREFIX+' '+name, d)
    th.kit.objects.link(o)
    return o


def prism(name, pts, z0, z1, mat):
    n = len(pts)
    v = [(p[0], p[1], z0) for p in pts]+[(p[0], p[1], z1) for p in pts]
    f = [tuple(reversed(range(n))), tuple(range(n, 2*n))]+[(i, (i+1) % n, (i+1) % n+n, i+n) for i in range(n)]
    return mesh_obj(name, v, f, mat)


def yz_prism(name, pts_yz, x0, x1, mat):
    """Extrude a polygon given in (y, z) along x."""
    n = len(pts_yz)
    v = [(x0, y, z) for y, z in pts_yz]+[(x1, y, z) for y, z in pts_yz]
    f = [tuple(reversed(range(n))), tuple(range(n, 2*n))]+[(i, (i+1) % n, (i+1) % n+n, i+n) for i in range(n)]
    return mesh_obj(name, v, f, mat)


def poly(c, apothem, n=8, rot=22.5):
    r = apothem/math.cos(math.pi/n)
    return [(c[0]+r*math.cos(math.radians(rot+360/n*i)), c[1]+r*math.sin(math.radians(rot+360/n*i))) for i in range(n)]


def station(name, loc, size, yaw, mat, purpose):
    o = th.cube(name, loc, size, mat)
    o.rotation_euler.z = yaw
    o['purpose'] = purpose
    return o


def sbox(name, x0, x1, t0, t1, off, thick, s, mat):
    """A box lying on the sloped hull wall of side s (+1 or -1) between wall parameters t0 and t1; off is the stand-off."""
    tc = (t0+t1)/2
    y, z = wall_pt(s, tc)
    ny, nz = -s*math.cos(SLOPE)*0+(-s*math.cos(SLOPE)), math.sin(SLOPE)   # inward normal: (-cos(slope), sin(slope)) mirrored per side
    ny, nz = -s*math.cos(SLOPE), math.sin(SLOPE)
    o = th.cube(name, ((x0+x1)/2, y+ny*(off+thick/2), z+nz*(off+thick/2)), (x1-x0, thick, (t1-t0)*SLOPE_L), mat)
    o.rotation_euler.x = -s*SLOPE
    return o


def build(m):
    cube, beam, pipe = th.cube, th.beam, th.pipe
    # ---- floor: grating plates, crate sockets, ladder foot, exit chevrons -----------------------------------
    cube('Floor slab', ((XA+XF)/2, 0, Z_FLOOR-.1), (XF-XA, 2*HW0, .2), m['steel'])
    for i in range(13):
        for j in range(14):
            xa, ya = XA+.08+i*.6, -HW0+.06+j*.612
            if xa+.58 > XF-.02:
                continue
            cube('Floor plate', (xa+.29, ya+.3, Z_FLOOR+.006), (.575, .60, .012), m['floor'], bevel=.002)
    for x in (-7.0, -5.8, -4.6, -3.4):
        for y in (-3.6, -2.4, -1.2, 0, 1.2, 2.4, 3.6):
            if abs(x-LADDER[0]) < .75 and abs(y-LADDER[1]) < .95:
                continue
            for dx, dy, sx, sy in ((0, .5, 1.0, .05), (0, -.5, 1.0, .05), (.5, 0, .05, 1.0), (-.5, 0, .05, 1.0)):
                cube('Crate socket', (x+dx, y+dy, Z_FLOOR+.016), (sx, sy, .008), m['ochre'], bevel=.001)
    for a in range(8):
        t = math.radians(a*45+22.5)
        cube('Ladder foot mark', (LADDER[0]+.78*math.cos(t), LADDER[1]+.78*math.sin(t), Z_FLOOR+.016), (.26, .12, .008), m['amber'] if a % 2 else m['steel'], bevel=.001)
    for k in range(4):
        cube('Exit chevron', (XA+.4+k*.4, 0, Z_FLOOR+.016), (.16, 2.0, .008), m['amber'] if k % 2 == 0 else m['steel'], bevel=.001)
    # ---- the hopper walls: lining, frames, rails, trunks ---------------------------------------------------------
    frames = (-7.5, -6.3, -5.1, -3.9, -2.2, -.9)
    for s in (-1, 1):
        sbox('Hopper lining', XA+.1, XF-.1, .28, 1.0, 0, .06, s, m['lining'])
        sbox('Hopper kick', XA+.1, XF-.1, 0.0, .28, 0, .09, s, m['teal'])
        sbox('Hopper kick cap', XA+.1, XF-.1, .275, .30, .02, .12, s, m['ochre'])
        for x in frames:
            sbox('Frame rib', x-.09, x+.09, 0, 1.0, .02, .12, s, m['teal'])
            sbox('Frame rib knee', x-.12, x+.12, .0, .06, .02, .16, s, m['steel'])
        for t in (.42, .62):    # tie-down rails on the slope with D-rings
            sbox('Wall rail', XA+.2, XF-.2, t-.012, t+.012, .06, .05, s, m['ochre'])
            for k in range(8):
                x = XA+.55+k*.9
                sbox('D-ring', x-.05, x+.05, t-.03, t+.03, .1, .06, s, m['brass'])
        sbox('Wall trunk', XA+.2, XF-.2, .80, .84, .08, .09, s, m['ochre'] if s < 0 else m['teal'])
    # ---- ceiling with the ladder opening, ribs across the full width, ducts, lamps ------------------------------------------
    for pts in (((XA, -HW1), (HOLE[0], -HW1), (HOLE[0], HW1), (XA, HW1)),
                ((HOLE[1], -HW1), (XF, -HW1), (XF, HW1), (HOLE[1], HW1)),
                ((HOLE[0], -HW1), (HOLE[1], -HW1), (HOLE[1], HOLE[2]), (HOLE[0], HOLE[2])),
                ((HOLE[0], HOLE[3]), (HOLE[1], HOLE[3]), (HOLE[1], HW1), (HOLE[0], HW1))):
        mesh_obj('Ceiling', [(x, y, CEIL) for x, y in pts], [(0, 1, 2, 3)], m['ceil'])
    for x in frames:
        beam('Ceiling rib', (x, -HW1+.05, CEIL-.1), (x, HW1-.05, CEIL-.1), .18, .20, m['teal'])
    for y in (-2.3, 2.3):
        cube('Duct trunk', ((XA+XF)/2, y, CEIL-.42), (XF-XA-.6, .56, .46), m['steel'])
        for x in range(-7, 0):
            cube('Duct clamp', (x+.0, y, CEIL-.42), (.1, .64, .54), m['teal'])
    pipe('Hot trunk', (XA+.2, -1.2, CEIL-.26), (XF-.1, -1.2, CEIL-.26), .09, m['ochre'])
    pipe('Cold trunk', (XA+.2, 1.2, CEIL-.26), (XF-.1, 1.2, CEIL-.26), .09, m['teal'])
    for x in (-7.2, -5.9, -2.0, -.7):
        pipe('Pipe collar', (x-.05, -1.2, CEIL-.26), (x+.05, -1.2, CEIL-.26), .13, m['steel'])
        pipe('Pipe collar', (x-.05, 1.2, CEIL-.26), (x+.05, 1.2, CEIL-.26), .13, m['steel'])
    for i, x in enumerate(frames):    # caged work lamps beside the frames
        for y in (-1.0, 1.0, -3.4, 3.4):
            if abs(x-LADDER[0]) < 1.2 and abs(y) < 1.5:
                continue
            cube('Work lamp', (x+.55, y, CEIL-.28), (.34, .16, .10), m['amber'])
            beam('Lamp cage', (x+.55-.19, y, CEIL-.2), (x+.55-.19, y, CEIL-.36), .03, .03, m['brass'])
            beam('Lamp cage', (x+.55+.19, y, CEIL-.2), (x+.55+.19, y, CEIL-.36), .03, .03, m['brass'])
    beam('Gantry rail', (XA+.3, 0, CEIL-.2), (ARCH_X-.4, 0, CEIL-.2), .16, .16, m['brass'])
    beam('Gantry rail cross', (-6.2, -3.2, CEIL-.2), (-6.2, 3.2, CEIL-.2), .14, .14, m['brass'])
    cube('Hoist trolley', (-6.2, 1.4, CEIL-.38), (.5, .4, .24), m['steel'])
    pipe('Hoist chain', (-6.2, 1.4, CEIL-.5), (-6.2, 1.4, -1.2), .02, m['steel'])
    cube('Hoist hook', (-6.2, 1.4, -1.28), (.12, .06, .14), m['ochre'])
    # ---- aft bulkhead with the doorway, front wall with gauges -------------------------------------------------------------
    for s in (-1, 1):
        yz_prism('Aft bulkhead', [(s*1.5, Z_FLOOR), (s*HW0, Z_FLOOR), (s*HW1, CEIL), (s*1.5, CEIL)], XA-.1, XA+.1, m['lining'])
    yz_prism('Front wall', [(-HW0, Z_FLOOR), (HW0, Z_FLOOR), (HW1, CEIL), (-HW1, CEIL)], XF-.12, XF, m['steel'])
    for i, y in enumerate((-2.4, -1.6, -.8, 0, .8, 1.6, 2.4)):
        pipe('Gauge rim', (XF-.16, y, -1.15), (XF-.12, y, -1.15), .16, m['brass'])
        pipe('Gauge face', (XF-.16, y, -1.15), (XF-.17, y, -1.15), .13, m['cream'])
        beam('Gauge needle', (XF-.18, y, -1.15), (XF-.18, y+.08*(1 if i % 2 else -1), -1.08), .02, .012, m['steel'])
    # ---- the bulkhead arch: opening 3.8 m wide, heavy piers, a hazard lintel --------------------------------------------------
    for s in (-1, 1):
        yz_prism('Arch wall', [(s*2.0, Z_FLOOR), (s*HW0, Z_FLOOR), (s*HW1, CEIL), (s*2.0, CEIL)], ARCH_X-.1, ARCH_X+.1, m['lining'])
        cube('Arch pier', (ARCH_X, s*1.92, (Z_FLOOR+CEIL)/2), (.5, .5, CEIL-Z_FLOOR), m['teal'])
        cube('Arch pier foot', (ARCH_X, s*1.92, Z_FLOOR+.14), (.6, .6, .28), m['steel'])
        cube('Arch pier cap', (ARCH_X, s*1.92, CEIL-.1), (.6, .6, .2), m['steel'])
        for yy in (s*3.1, s*4.2):
            zc = Z_FLOOR+1.15+(abs(yy)-3.1)*.0
            pts = poly((0, 0), .22)
            for q in range(8):
                a, b = pts[q], pts[(q+1) % 8]
                beam('Porthole frame', (ARCH_X-.11, yy+a[0], zc+a[1]), (ARCH_X-.11, yy+b[0], zc+b[1]), .05, .05, m['ochre'])
            mesh_obj('Porthole glass', [(ARCH_X-.1, yy+q[0], zc+q[1]) for q in pts], [tuple(range(8))], m['glass'])
    cube('Arch lintel', (ARCH_X, 0, -.62), (.5, 4.0, .40), m['teal'])
    for k in range(10):
        cube('Lintel hazard', (ARCH_X+.26, -1.8+k*.4, -.62), (.03, .2, .30), m['ochre'] if k % 2 == 0 else m['steel'], bevel=.002)
    # ---- hall: plinth, reactor, rods ----------------------------------------------------------------------------------------------
    plin = poly(CORE, 1.15)
    prism('Plinth', plin, Z_FLOOR, Z_PLINTH, m['steel'])
    prism('Plinth tread', poly(CORE, 1.12), Z_PLINTH, Z_PLINTH+.008, m['floor'])
    for i in range(8):
        a, b = plin[i], plin[(i+1) % 8]
        beam('Plinth hazard', (a[0], a[1], Z_PLINTH+.012), (b[0], b[1], Z_PLINTH+.012), .09, .02, m['ochre'] if i % 2 == 0 else m['steel'])
    for k in (1, 2):    # two 0.2 m steps up from the -y side
        cube('Plinth step', (CORE[0], -1.15-.3*(3-k)+.15, Z_FLOOR+.2*k/2), (1.5, .3, .2*k), m['steel'])
        cube('Plinth step tread', (CORE[0], -1.15-.3*(3-k)+.15, Z_FLOOR+.2*k+.004), (1.5, .3, .008), m['ochre'], bevel=.001)
    prism('Core housing', poly(CORE, .62), Z_PLINTH, -1.7, m['steel'])
    prism('Core vessel', poly(CORE, .5), -1.7, -.55, m['steel'])
    vessel = poly(CORE, .5)
    for i in range(8):
        a, b = vessel[i], vessel[(i+1) % 8]
        mid = ((a[0]+b[0])/2, (a[1]+b[1])/2)
        nx, ny = mid[0]-CORE[0], mid[1]-CORE[1]
        d = math.hypot(nx, ny)
        o = beam('Core slit', (mid[0]+nx/d*.012, mid[1]+ny/d*.012, -1.6), (mid[0]+nx/d*.012, mid[1]+ny/d*.012, -.65), .1, .03, m['hot'])
        o.rotation_euler.z = math.atan2(ny, nx)+math.pi/2
    for z in (-1.68, -.58):
        prism('Core glow band', poly(CORE, .52), z, z+.05, m['hot'])
    for z in (-1.4, -1.0):
        prism('Core ring', poly(CORE, .6), z, z+.09, m['teal'])
    prism('Core neck', poly(CORE, .3), -.55, CEIL, m['steel'])
    prism('Core collar', poly(CORE, .42), CEIL-.25, CEIL, m['teal'])
    for i in range(8):
        t = math.radians(i*45)
        pipe('Core feed', (CORE[0]+.2*math.cos(t), CORE[1]+.2*math.sin(t), -.65), (CORE[0]+.5*math.cos(t), CORE[1]+.5*math.sin(t), CEIL), .035, m['brass'])
    for i in range(5):
        t = math.radians(-72+i*36)
        c = (CORE[0]+.85*math.cos(t+math.pi), CORE[1]+.85*math.sin(t+math.pi))
        pipe('Rod tube glass', (c[0], c[1], Z_PLINTH+.1), (c[0], c[1], -1.2), .09, m['glass'])
        pipe('Fuel rod', (c[0], c[1], Z_PLINTH+.14), (c[0], c[1], -1.55+.12*i), .055, m['hot'] if i < 3 else m['amber'])
        pipe('Rod tube cap', (c[0], c[1], -1.2), (c[0], c[1], -1.13), .11, m['brass'])
    station('Grey placeholder rod loader', (CORE[0]+.15, 1.55, Z_FLOOR+.25), (.5, .4, .5), 0, m['grey'], 'Fuel rod loader hatch (insert rods by hand)')
    # ---- hot side (-y): turbine -------------------------------------------------------------------------------------------------
    pipe('Turbine body', (-2.55, -3.0, -2.05), (-.35, -3.0, -2.05), .6, m['grey'])
    for k in range(6):
        pipe('Turbine fin', (-2.4+k*.36, -3.0, -2.05), (-2.3+k*.36, -3.0, -2.05), .68, m['steel'])
    pipe('Turbine glow', (-1.8, -3.0, -2.05), (-1.2, -3.0, -2.05), .62, m['hot'])
    pipe('Exhaust stack', (-.45, -3.0, -1.5), (-.45, -3.0, CEIL), .16, m['ochre'])
    for x in (-2.2, -.7):
        cube('Turbine saddle', (x, -3.0, Z_FLOOR+.2), (.14, 1.4, .4), m['steel'])
    station('Grey placeholder turbine lever stand', (-.7, -1.9, Z_FLOOR+.5), (.4, .8, 1.0), 0, m['grey'], 'Turbine lever stand (hand lever)')
    # ---- cold side (+y): coolant tanks --------------------------------------------------------------------------------------------
    for z, name in ((-2.45, 'low'), (-1.6, 'high')):
        pipe('Coolant tank '+name, (-2.55, 3.1, z), (-.35, 3.1, z), .4, m['teal'])
        pipe('Coolant glow '+name, (-1.8, 3.1, z), (-1.2, 3.1, z), .42, m['cold'])
    for x in (-2.3, -1.5, -.7):
        cube('Tank strap', (x, 3.1, -2.0), (.12, .9, 1.5), m['steel'])
    for x in (-2.0, -1.0):
        pipe('Sight glass', (x, 2.55, -2.75), (x, 2.55, -1.2), .04, m['cold'])
    station('Grey placeholder reactor control stand', (-1.9, 1.9, Z_FLOOR+.5), (.5, 1.6, 1.0), -.6, m['grey'], 'Reactor control stand (fission and turbine levers, dials)')
    for y in (-.5, 0, .5):
        beam('Lever stub', (-1.9, 1.9+y*.9, Z_FLOOR+1.0), (-1.75, 1.9+y*.9, Z_FLOOR+1.55), .05, .05, m['ochre'])
    # ---- cargo: crates, placeholders -----------------------------------------------------------------------------------------------
    for x, y, sz in ((-6.9, 3.2, (1.0, 1.0, 1.0)), (-5.7, 3.3, (.8, .8, .8)), (-6.8, -3.3, (1.1, .9, .9)), (-3.6, -3.4, (.9, .9, .9)), (-3.7, 3.4, (.9, .8, .8))):
        station('Grey placeholder crate', (x, y, Z_FLOOR+sz[2]/2), sz, 0, m['grey'], 'Crate placeholder')
    cube('Grey placeholder crate stacked', (-6.9, 3.2, Z_FLOOR+1.35), (.7, .7, .7), m['grey'])
    # ---- lights ------------------------------------------------------------------------------------------------------------------------
    for x in (-6.6, -4.9):
        th.area_light('Cargo light', (x, 0, CEIL-.3), (x, 0, Z_FLOOR), (.80, .90, 1.0), 520, 2.4)
    th.area_light('Wall wash port', (-5.5, -4.8, -1.2), (-5.5, -3.5, Z_FLOOR), (.80, .90, 1.0), 220, 1.6)
    th.area_light('Wall wash starboard', (-5.5, 4.8, -1.2), (-5.5, 3.5, Z_FLOOR), (.80, .90, 1.0), 220, 1.6)
    th.area_light('Core glow', (CORE[0], CORE[1], -1.1), (CORE[0], CORE[1], Z_FLOOR), (1., .38, .1), 280, 1.0)
    th.area_light('Hot side', (-1.4, -3.0, -.9), (-1.4, -3.0, Z_FLOOR), (1., .45, .12), 260, 1.6)
    th.area_light('Cold side', (-1.4, 3.0, -.9), (-1.4, 3.0, Z_FLOOR), (.35, .8, 1.), 280, 1.6)
    th.area_light('Hall fill', (-1.4, 0, CEIL-.2), (-1.4, 0, Z_FLOOR), (1., .7, .45), 340, 2.4)
    for o in th.review.objects:
        if o.type == 'LIGHT':
            o.visible_camera = False


def make_plan(m):
    s = bpy.data.scenes.new(PREFIX+' | Plan')
    c = bpy.data.collections.new(PREFIX+' plan marks')
    s.collection.children.link(c)
    for o in th.kit.objects:
        if any(t in o.name for t in ['Floor plate', 'Crate socket', 'Plinth', 'Core housing', 'Core vessel', 'Grey placeholder', 'Turbine body', 'Coolant tank', 'Arch pier', 'Arch lintel', 'Ladder foot', 'Exit chevron', 'Rod tube']):
            c.objects.link(o)
    for xs, ys, col in (((XA, XF), (HW0, HW0), m['steel']), ((XA, XF), (-HW0, -HW0), m['steel']), ((XA, XA), (-HW0, HW0), m['steel']), ((XF, XF), (-HW0, HW0), m['steel']),
                        ((XA, XF), (HW1, HW1), m['ochre']), ((XA, XF), (-HW1, -HW1), m['ochre'])):
        o = th.beam('Plan wall', (xs[0], ys[0], -1.0), (xs[1], ys[1], -1.0), .12, .2, col)
        c.objects.link(o)
        th.kit.objects.unlink(o)
    th.label('LOWER DECK  /  MACHINE DECK', (-4.0, 8.2, -1.0), .4, m['steel'], c)
    th.label('FLOOR -2.90 (+-4.3 m)   PLINTH -2.50   CEILING -0.24 (+-6.8 m): the hull walls lean outward', (-4.0, 7.5, -1.0), .2, m['steel'], c)
    th.label('LADDER FROM HUB', (LADDER[0], 1.6, -1.0), .2, m['cream'], c)
    th.label('RAMP EXIT', (XA+.3, 0, -1.0), .2, m['cream'], c, rotation=(0, 0, math.pi/2))
    th.label('HOT / TURBINE', (-1.4, -3.8, -1.0), .22, m['cream'], c)
    th.label('COLD / COOLANT', (-1.4, 3.8, -1.0), .22, m['cream'], c)
    th.label('CORE', (CORE[0], 0, -1.0), .28, m['cream'], c)
    th.label('ARCH', (ARCH_X, -2.9, -1.0), .2, m['cream'], c)
    th.label('ochre line = wall at ceiling height', (-4.0, -7.4, -1.0), .2, m['steel'], c)
    s.camera = th.camera('Plan top', (-4.0, 0, -.7), (-4.0, 0, -3), ortho=17, scene=s)
    s.camera.data.clip_start = .02
    s.render.engine = 'BLENDER_WORKBENCH'
    s.display.shading.light = 'FLAT'; s.display.shading.color_type = 'MATERIAL'
    s.display.shading.show_shadows = False; s.display.shading.show_cavity = True
    s.display.shading.background_type = 'WORLD'
    s.world = bpy.data.worlds.new(PREFIX+' plan paper'); s.world.color = (.82, .81, .74)
    s.view_settings.view_transform = 'Standard'
    s.render.resolution_x = 1600; s.render.resolution_y = 1000; s.render.resolution_percentage = 100
    return s


def check(before, source):
    problems = []
    unchanged = th.fingerprint(source) == before
    if not unchanged:
        problems.append('source ship changed')
    for o in th.kit.objects:
        pts = [o.matrix_world@Vector(v) for v in o.bound_box]
        lo = [min(p[i] for p in pts) for i in range(3)]; hi = [max(p[i] for p in pts) for i in range(3)]
        if o.name.startswith(PREFIX+' Grey placeholder'):
            if lo[0] < LADDER[0]+.55 and hi[0] > LADDER[0]-.55 and lo[1] < LADDER[1]+.55 and hi[1] > LADDER[1]-.55 and hi[2] > Z_FLOOR+.3:
                problems.append('ladder foot blocked by '+o.name)
        if not any(t in o.name for t in ('Floor slab', 'Hopper', 'Frame', 'Wall', 'D-ring', 'Aft bulkhead', 'Front wall', 'Arch wall', 'Ceiling', 'Duct', 'Rib', 'Gantry', 'Hoist', 'trunk', 'Pipe collar', 'Core', 'Exhaust', 'Arch pier', 'Porthole', 'Lamp', 'Work lamp', 'Trunk')):
            if max(abs(lo[1]), abs(hi[1])) > hw_at(lo[2])+.03 and lo[2] < CEIL-.4:
                problems.append('outside the hull: '+o.name)
        if hi[2] > CEIL+.01 and not any(t in o.name for t in ('Core', 'Exhaust', 'Arch', 'Ceiling', 'Duct', 'Rib', 'Gantry', 'Hoist', 'trunk', 'Pipe collar', 'Hopper', 'Frame', 'Wall', 'Aft', 'Front', 'Lamp')):
            problems.append('pierces the ceiling: '+o.name)
    report = {'problems': problems, 'source_unchanged': unchanged, 'floor_width_m': 2*HW0, 'ceiling_width_m': 2*HW1,
              'deck_length_m': XF-XA, 'headroom_m': round(CEIL-Z_FLOOR, 2), 'plinth_headroom_m': round(CEIL-Z_PLINTH, 2),
              'wall_lean_deg': round(math.degrees(SLOPE), 1),
              'new_objects': len(th.kit.objects), 'unique_meshes': len({o.data for o in th.kit.objects if o.type == 'MESH'}),
              'scope': 'architecture study; no game collision validation'}
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT/'geometry-check.json').write_text(json.dumps(report, indent=2)+'\n')
    for p in problems:
        print('PROBLEM: '+p)
    print(json.dumps(report))
    return report


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--render', action='store_true')
    parser.add_argument('--replace', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else [])
    OUT.mkdir(parents=True, exist_ok=True)
    existing = [s for s in bpy.data.scenes if s.name.startswith(PREFIX)]
    if existing and not args.replace:
        raise RuntimeError('Study already exists; inspect before --replace.')
    for s in existing:
        bpy.data.scenes.remove(s)
    for c in list(bpy.data.collections):
        if c.name.startswith(PREFIX):
            for o in list(c.objects):
                if o.name.startswith(PREFIX):
                    bpy.data.objects.remove(o, do_unlink=True)
            bpy.data.collections.remove(c)
    for coll in (bpy.data.meshes, bpy.data.materials, bpy.data.lights, bpy.data.cameras, bpy.data.curves):
        for d in list(coll):
            if d.name.startswith(PREFIX) and d.users == 0:
                coll.remove(d)
    th.PREFIX = PREFIX
    source = bpy.data.scenes[SOURCE]
    before = th.fingerprint(source)
    study = bpy.data.scenes.new(PREFIX+' | Architecture')
    study.unit_settings.system = 'METRIC'
    shell = bpy.data.collections.new(PREFIX+' shell (source objects)')
    study.collection.children.link(shell)
    skip = ('Labels', 'Guides', 'Sightline', 'Ground', 'Plan', 'Gear stowed')
    for c in source.collection.children:
        if any(t in c.name for t in skip):
            continue
        for o in c.all_objects:
            if o.name.startswith(DROPPED) or o.name in shell.objects:
                continue
            shell.objects.link(o)
    kit = bpy.data.collections.new(PREFIX+' modular interior'); study.collection.children.link(kit)
    review = bpy.data.collections.new(PREFIX+' lighting'); study.collection.children.link(review)
    th.kit, th.review, th.study, th.box_meshes, th.source = kit, review, study, {}, source
    bpy.context.window.scene = study
    m = dict(floor=th.material('grating dark', (.09, .11, .115), .3),
             lining=th.material('warm lining', (.43, .40, .31)),
             teal=th.material('painted petrol', (.075, .19, .175), .18),
             steel=th.material('dark iron', (.045, .065, .064), .35),
             ochre=th.material('ochre edges', (.65, .33, .065), .1),
             amber=th.material('warm diffusers', (1, .62, .25), emission=4),
             hot=th.material('hot core', (1., .14, .01), emission=1.7),
             cold=th.material('cold coolant', (.30, .75, 1.), emission=2.2),
             ceil=th.material('ceiling warm grey', (.30, .27, .22)),
             grey=th.material('plain grey placeholders', (.28, .29, .28)),
             cream=th.material('gauge face', (.83, .82, .71)),
             brass=th.material('brass', (.78, .50, .16), .85),
             glass=th.material('tube glass', (.5, .7, .8)))
    m['glass'].surface_render_method = 'BLENDED'
    m['glass'].node_tree.nodes['Principled BSDF'].inputs['Alpha'].default_value = .18
    m['glass'].node_tree.nodes['Principled BSDF'].inputs['Roughness'].default_value = .3
    study.world = bpy.data.worlds.new(PREFIX+' dark'); study.world.use_nodes = True
    study.world.node_tree.nodes['Background'].inputs[0].default_value = (.02, .03, .05, 1)
    study.world.node_tree.nodes['Background'].inputs[1].default_value = .8
    study.render.engine = 'BLENDER_EEVEE'
    study.render.resolution_x = 1600; study.render.resolution_y = 1000; study.render.resolution_percentage = 100
    study.view_settings.view_transform = 'AgX'; study.view_settings.look = 'AgX - Medium High Contrast'; study.view_settings.exposure = 1.1
    build(m)
    cams = {
        'ladder': th.camera('01 Ladder foot looking at the reactor', (LADDER[0], -.9, Z_FLOOR+1.7), (CORE[0], 0, -1.5), lens=18),
        'hall': th.camera('02 Reactor hall from the cold side', (-.5, 1.9, Z_FLOOR+1.65), (CORE[0]-.3, -1.3, -1.65), lens=15),
        'cargo': th.camera('03 Ramp door looking forward', (XA+.5, -1.2, Z_FLOOR+1.7), (-3.0, .3, -1.7), lens=15),
        'wide': th.camera('04 Wide look at the hopper walls', (XA+.7, 0.0, -1.35), (-2.0, 0.0, -1.7), lens=12),
        'section': th.camera('05 Cross section looking forward', (-7.2, 0.0, -1.57), (-2.0, 0.0, -1.57), ortho=15),
    }
    cams['section'].data.clip_start = .3
    for c_ in cams.values():     # study cameras are fixed: navigating in the viewport must not move them
        c_.lock_location = c_.lock_rotation = c_.lock_scale = (True, True, True)
    study.camera = cams['ladder']
    plan = make_plan(m)
    bpy.context.window.scene = study
    bpy.context.view_layer.update()
    report = check(before, source)
    if report['problems']:
        raise RuntimeError('Geometry checks failed')
    if args.render:
        shots = [(study, cams['ladder'], '01-ladder'), (study, cams['hall'], '02-hall'), (study, cams['cargo'], '03-cargo'),
                 (study, cams['wide'], '04-wide'), (study, cams['section'], '05-section'), (plan, plan.camera, '06-plan')]
        for s, c, name in shots:
            bpy.context.window.scene = s
            s.camera = c
            s.render.filepath = str(OUT/(name+'.png'))
            bpy.ops.render.render(write_still=True)
        bpy.context.window.scene = study
        study.camera = cams['ladder']
    for screen in bpy.data.screens:
        for a in screen.areas:
            if a.type == 'VIEW_3D':
                a.spaces.active.region_3d.view_perspective = 'CAMERA'
                a.spaces.active.overlay.show_overlays = False
                a.spaces.active.shading.type = 'RENDERED'
    print('READY: '+str(OUT))


if __name__ == '__main__':
    main()
