"""Bridge interior study against the approved, 1.2-scaled live ship (v3: stepped bridge, no neck).

The bridge stays where it is, directly behind the hub doors. The rear part is flat (instruments,
captain's seat); forward of that the floor falls down a ramp to the pilot's nest in the glazed nose,
and the belly under the nose is glass. A faceted dome with a glazed front half closes the room;
the ship's octagon motif returns in the seat tiers, the dome rings and the floor glow.
Landmark: a brass-and-glass compass globe hangs over the captain's island. Consoles stand as two half
rings of segments around the island; the aft wall is an analogue instrument wall.
Consoles and displays are plain grey placeholders: the display work lives on its own branches.
Reuses the helpers of turret_hub.py. Never changes the source ship.
Live: runpy.run_path(..., run_name='__main__') in the open Blender (no glTF export there, it crashes 5.2.1).
Headless: blender -b -t 4 -P art/ship/bridge.py -- --render
"""
from pathlib import Path
import argparse
import json
import math
import sys

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.modules.pop('turret_hub', None)
import turret_hub as th

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'build/bridge'
PREFIX = 'Bridge study'
SOURCE = th.SOURCE

HUB_X = 6.11                  # door plane of the main hull (the old bridge doors)
SHIFT = 0.0                   # the pod stays directly behind the hub doors
AFT = HUB_X+SHIFT             # world x of the pod's aft wall
DOORS = [1.92, -1.92]         # y of the two corridors, doors 1.8 m wide
CORR_W = 2.0                  # clear corridor width
LEVEL = .0                    # rear (instrument) level: the door level of the hub
NEST = -.45                   # pilot's nest
RIM = 2.80                    # wall top (the existing glazing cage); the dome starts here
DOME_TOP = 4.15
STEP = .15
# Floor profile along the pod, u = metres forward of the aft wall: flat, ramp down, nest.
ZF = [(0.0, LEVEL), (4.6, LEVEL), (6.8, NEST), (8.18, NEST)]
BP = [0.0, 2.28, 4.6, 5.88, 6.8, 8.18]          # breakpoints of outline and floor
GLASS_FLOOR_U = 5.88          # the belly is glass forward of here, |y| <= 2.8
GLASS_FLOOR_Y = 2.8
GLASS_EDGES = (2, 3, 4)       # dome facets over the forward glazing
SEAT_U = 3.4                  # the island sits in the middle of the flat zone
RAMP_U0, RAMP_U1 = 4.6, 6.8


def zf(u):
    for (a, za), (b, zb) in zip(ZF, ZF[1:]):
        if a <= u <= b:
            return za if b == a else za+(zb-za)*(u-a)/(b-a)
    return ZF[-1][1]


def hw(u):
    """Half width of the pod at u (inside the glazing)."""
    if u <= 2.28:
        return 5.70+.80*u/2.28
    if u <= 5.88:
        return 6.50
    return 6.50-2.80*(u-5.88)/2.30


def X(u):
    return AFT+u


def P(u, y, z=None):
    return (X(u), y, zf(u) if z is None else z)


OUTLINE = [(u, -hw(u)) for u in BP]+[(u, hw(u)) for u in reversed(BP)]     # the aft wall is the closing edge
ROOM8 = [(0, -5.70), (2.28, -6.5), (5.88, -6.5), (8.18, -3.7), (8.18, 3.7), (5.88, 6.5), (2.28, 6.5), (0, 5.70)]


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


def oct_pts(c, apothem, z=0.):
    r = apothem/math.cos(math.pi/8)
    return [(c[0]+r*math.cos(math.radians(22.5+45*i)), c[1]+r*math.sin(math.radians(22.5+45*i)), z) for i in range(8)]


def station(name, loc, size, yaw, mat, purpose, pitch=0.):
    o = th.cube(name, loc, size, mat)
    o.rotation_euler = (0, pitch, yaw)
    o['purpose'] = purpose
    return o


def sloped(name, x0, x1, y, size_y, z0, z1, height, mat):
    """A box that follows the corridor slope; z0 and z1 are the centre heights at x0 and x1."""
    L = math.hypot(x1-x0, z1-z0)
    o = th.cube(name, ((x0+x1)/2, y, (z0+z1)/2), (L, size_y, height), mat)
    o.rotation_euler.y = -math.atan2(z1-z0, x1-x0)
    return o


def compass_globe(m, C):
    """Glass globe with an amber core and three brass rings, hung from the dome top by a rod."""
    cz = 2.85
    th.pipe('Globe rod', (C[0], 0, DOME_TOP-.15), (C[0], 0, cz+.5), .035, m['brass'])
    for r_, mat, name in ((.46, m['glass'], 'Globe glass'), (.12, m['amber'], 'Globe core')):
        bm = bmesh.new()
        bmesh.ops.create_uvsphere(bm, u_segments=14, v_segments=8, radius=r_)
        me = bpy.data.meshes.new(PREFIX+' '+name)
        bm.to_mesh(me)
        bm.free()
        me.materials.append(mat)
        o = bpy.data.objects.new(PREFIX+' '+name, me)
        th.kit.objects.link(o)
        o.location = (C[0], 0, cz)
    for i in range(24):      # tick marks on the equator
        f = i*math.tau/24
        th.cube('Globe tick', (C[0]+.60*math.cos(f), .60*math.sin(f), cz), (.05, .05, .09 if i % 3 == 0 else .05), m['ochre'])
    th.cube('Globe moon', (C[0]+.68*math.cos(1.1), .68*math.sin(1.1), cz), (.11, .11, .11), m['amber'])
    for tilt, spin, rad in ((0, 0, .56), (60, 20, .62), (120, -15, .68)):
        tt, ss = math.radians(tilt), math.radians(spin)
        pts = []
        for i in range(16):
            f = i*math.tau/16
            x, y, z = rad*math.cos(f), rad*math.sin(f)*math.cos(tt), rad*math.sin(f)*math.sin(tt)
            pts.append((C[0]+x*math.cos(ss)-y*math.sin(ss), x*math.sin(ss)+y*math.cos(ss), cz+z))
        for i in range(16):
            th.beam('Globe ring', pts[i], pts[(i+1) % 16], .06, .06, m['brass'])
    th.beam('Globe gimbal', (C[0], 0, cz+.7), (C[0], 0, cz+.5), .08, .08, m['brass'])


def captain_displays(m, C):
    """Two big screens on brass arms from the dome, left and right of the seat and turned toward it."""
    for s_ in (-1, 1):
        p = (C[0]+.95, s_*1.6)
        d = Vector((C[0]-p[0], -p[1]))
        d.normalize()
        psi = math.atan2(-d.x, d.y)
        nx, ny = -math.sin(psi), math.cos(psi)       # the screen face
        tx, ty = math.cos(psi), math.sin(psi)        # along the screen
        zc = 1.78
        frame = th.cube('Captain screen frame', (p[0], p[1], zc), (1.58, .09, .94), m['teal'])
        frame.rotation_euler = (-.12, 0, psi)
        face = th.cube('Captain screen', (p[0]+nx*.05, p[1]+ny*.05, zc+.005), (1.38, .02, .76), m['steel'])
        face.rotation_euler = (-.12, 0, psi)
        face['purpose'] = 'Captain display (one of two); the display branches decide the content'
        for k in (-1, 1):
            top = (p[0]-nx*.06+tx*k*.5, p[1]-ny*.06+ty*k*.5, zc+.47)
            mount = (p[0]+tx*k*.35+(.15), p[1]*1.12+ty*k*.35, 4.02)
            th.beam('Screen arm', mount, top, .06, .06, m['brass'])
            th.cube('Screen arm joint', top, (.12, .12, .12), m['ochre'])
        th.cube('Screen arm mount', (p[0]+.15, p[1]*1.12, 4.06), (.9, .3, .1), m['teal'])


def build(m):
    cube, beam, pipe = th.cube, th.beam, th.pipe
    # --- floor surface: strips with a linear height per strip, glass belly in the nose ---------------
    for a, b in zip(BP, BP[1:]):
        if a >= GLASS_FLOOR_U-.001:
            g = GLASS_FLOOR_Y
            mesh_obj('Glass floor', [P(a, -g), P(b, -g), P(b, g), P(a, g)], [(0, 1, 2, 3)], m['glass'])
            for sgn in (-1, 1):
                pts = [P(a, sgn*g), P(b, sgn*g), P(b, sgn*hw(b)), P(a, sgn*hw(a))]
                mesh_obj('Floor strip', pts, [(0, 1, 2, 3)], m['floor'])
                mesh_obj('Belly plate', [(p[0], p[1], p[2]-.35) for p in pts], [(0, 1, 2, 3)], m['steel'])
        else:
            pts = [P(a, -hw(a)), P(b, -hw(b)), P(b, hw(b)), P(a, hw(a))]
            mesh_obj('Floor strip', pts, [(0, 1, 2, 3)], m['floor'])
            mesh_obj('Belly plate', [(p[0], p[1], p[2]-.35) for p in pts], [(0, 1, 2, 3)], m['steel'])
    # glass floor frame: a few ribs under the glass, a flush ochre border on top
    for u in (5.88, 7.0, 8.1):
        beam('Glass floor rib', P(u, -GLASS_FLOOR_Y, zf(u)-.10), P(u, GLASS_FLOOR_Y, zf(u)-.10), .10, .05, m['steel'])
    for y in (-GLASS_FLOOR_Y, 0, GLASS_FLOOR_Y):
        beam('Glass floor rib', P(5.88, y, zf(5.88)-.10), P(8.1, y, zf(8.1)-.10), .10, .05, m['steel'])
    for y in (-GLASS_FLOOR_Y, GLASS_FLOOR_Y):
        beam('Glass floor border', P(GLASS_FLOOR_U, y, zf(GLASS_FLOOR_U)+.012), P(8.15, y, zf(8.15)+.012), .08, .02, m['ochre'])
    for u in (GLASS_FLOOR_U, 8.1):
        beam('Glass floor border', P(u, -GLASS_FLOOR_Y, zf(u)+.012), P(u, GLASS_FLOOR_Y, zf(u)+.012), .08, .02, m['ochre'])
    # plate seams: across every 1.2 m and along every 1.2 m, following the floor profile
    for k in range(1, 7):
        u = k*1.2
        if hw(u) > .3:
            beam('Plate seam', P(u, -hw(u)+.15, zf(u)+.004), P(u, hw(u)-.15, zf(u)+.004), .02, .006, m['edge'])
    ubreaks = sorted(set([0.0, RAMP_U0, RAMP_U1, 8.18]))
    for yk in (-4.8, -3.6, -2.4, -1.2, 1.2, 2.4, 3.6, 4.8):
        for ua, ub in zip(ubreaks, ubreaks[1:]):
            if abs(yk) < hw(ub)-.2 and abs(yk) < hw(ua)-.2:
                beam('Plate seam', P(ua+.15, yk, zf(ua+.15)+.004), P(ub-.15, yk, zf(ub-.15)+.004), .02, .006, m['edge'])
    # seams where the slope changes, chevron stripes down the ramp
    for u in (RAMP_U0, RAMP_U1):
        beam('Slope seam', P(u, -hw(u)+.2, zf(u)+.012), P(u, hw(u)-.2, zf(u)+.012), .09, .02, m['ochre'])
    for k in range(1, int((RAMP_U1-RAMP_U0)/.6)+1):
        u = RAMP_U0+k*.6
        beam('Ramp stripe', P(u, -hw(u)+.4, zf(u)+.012), P(u, hw(u)-.4, zf(u)+.012), .06, .02, m['amber'] if k % 2 else m['ochre'])
    # skirt along the outline under the floor, glazing from floor to rim
    for i in range(len(OUTLINE)-1):
        (ua, ya), (ub, yb) = OUTLINE[i], OUTLINE[i+1]
        a, b = P(ua, ya), P(ub, yb)
        mesh_obj('Skirt', [a, b, (b[0], b[1], b[2]-.35), (a[0], a[1], a[2]-.35)], [(0, 1, 2, 3)], m['steel'])
        mesh_obj('Pod glass', [a, b, (b[0], b[1], RIM), (a[0], a[1], RIM)], [(0, 1, 2, 3)], m['glass'])
        beam('Pod rim', (a[0], a[1], RIM), (b[0], b[1], RIM), .16, .22, m['teal'])
        beam('Pod sill', (a[0], a[1], a[2]+.02), (b[0], b[1], b[2]+.02), .12, .10, m['teal'])
        n = max(1, int(round(math.hypot(b[0]-a[0], b[1]-a[1])/1.9)))
        for k in range(n+1):          # mullions: every vertex plus evenly spaced ones on long runs
            if k == n and i < len(OUTLINE)-2:
                continue
            t = k/n
            p = (a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t)
            corner = k in (0, n)
            if corner:
                beam('Cage post', (p[0], p[1], zf(p[0]-AFT)), (p[0], p[1], RIM), .24, .22, m['teal'])
                beam('Cage post cap', (p[0], p[1], RIM-.02), (p[0], p[1], RIM+.22), .28, .26, m['ochre'])
            else:
                beam('Pod mullion', (p[0], p[1], zf(p[0]-AFT)), (p[0], p[1], RIM), .10, .09, m['steel'])
    # --- aft wall of the pod: lining between the corridor openings -------------------------------------
    for a, b in [(-5.55, -2.94), (-.9, .9), (2.94, 5.55)]:
        k = max(1, math.ceil((b-a)/.9))
        w = (b-a)/k
        for i in range(k):
            y = a+(i+.5)*w
            cube('Lining panel', (AFT+.06, y, LEVEL+1.4), (.055, w-.035, RIM-LEVEL-.1), m['lining'])
            cube('Kick panel', (AFT+.09, y, LEVEL+.32), (.085, w-.055, .64), m['teal'])
            cube('Panel cap', (AFT+.11, y, RIM-.1), (.13, w-.05, .12), m['steel'])
        for y in (a+.055, b-.055):
            cube('Wall rib', (AFT+.12, y, LEVEL+1.45), (.20, .16, RIM-LEVEL+.1), m['teal'])
        cube('Hand-height rail', (AFT+.14, (a+b)/2, LEVEL+1.04), (.12, b-a-.06, .065), m['ochre'])
    for y in DOORS:
        cube('Door lamp hood', (AFT+.14, y, LEVEL+2.45), (.24, 1.92, .28), m['teal'])
        cube('Door lamp', (AFT+.17, y, LEVEL+2.33), (.27, 1.04, .045), m['amber'])
    # --- captain's seat: two low octagonal tiers, no rail ---------------------------------------------
    C = (X(SEAT_U), 0.0)
    for k, a_ in enumerate((1.05, .78), start=1):
        z0 = LEVEL+STEP*(k-1)
        pts = oct_pts(C, a_)
        prism('Captain tier %d' % k, pts, z0, z0+STEP, m['steel'])
        prism('Captain tier tread %d' % k, [(q[0], q[1]) for q in oct_pts(C, a_-.03)], z0+STEP, z0+STEP+.008, m['floor'])
        for i in range(8):
            q, w = pts[i], pts[(i+1) % 8]
            beam('Tier seam', (q[0], q[1], z0+STEP+.012), (w[0], w[1], z0+STEP+.012), .05, .025, m['ochre'])
    SEAT = LEVEL+2*STEP
    glow = oct_pts(C, 1.05+.4)
    for i in range(8):
        q, w = glow[i], glow[(i+1) % 8]
        beam('Floor glow ring', (q[0], q[1], LEVEL+.02), (w[0], w[1], LEVEL+.02), .09, .02, m['amber'])
    station('Grey placeholder captain chair', (C[0]-.1, 0, SEAT+.28), (.75, .75, .56), 0, m['grey'], 'Captain chair; design pending')
    cube('Grey placeholder chair back', (C[0]-.43, 0, SEAT+.8), (.12, .75, .7), m['grey'])
    for y in (-.55, .55):
        station('Grey placeholder arm console', (C[0], y, SEAT+.4), (.5, .24, .8), 0, m['grey'], 'Captain arm console; display branches decide the content')
    # --- landmark, the captain's two screens, two crew stations right beside the island --------------------
    compass_globe(m, C)
    captain_displays(m, C)
    for s_ in (-1, 1):
        cube('Grey placeholder crew seat', (X(3.0), s_*2.5, LEVEL+.26), (.6, .6, .52), m['grey'])
        cube('Grey placeholder crew seat back', (X(2.74), s_*2.5, LEVEL+.7), (.1, .6, .6), m['grey'])
        station('Grey placeholder crew desk', (X(3.7), s_*2.5, LEVEL+.35), (.55, 1.3, .7), 0, m['grey'], 'Crew desk (navigation / science)')
        top = cube('Grey placeholder crew desk top', (X(3.7), s_*2.5, LEVEL+.73), (.62, 1.36, .06), m['grey'])
        top.rotation_euler = (0, -.3, 0)
        dz = cube('Display zone', (X(3.7)+.01, s_*2.5, LEVEL+.77), (.4, .9, .02), m['steel'])
        dz.rotation_euler = (0, -.3, 0)
        dz['purpose'] = 'Reserved for the in-world display from the display branches'
    # pilot: two low wing consoles and a central yoke, so the view down through the glass floor stays open
    for s_ in (-1, 1):
        station('Grey placeholder pilot wing', (X(7.65), s_*1.05, NEST+.3), (.55, .85, .6), -s_*.45, m['grey'], 'Pilot wing console (engines / power)')
        top = th.cube('Grey placeholder pilot wing top', (X(7.65), s_*1.05, NEST+.63), (.6, .9, .06), m['grey'])
        top.rotation_euler = (0, .25, -s_*.45)
        dz = th.cube('Display zone', (X(7.65)-.02, s_*1.05, NEST+.665), (.4, .55, .02), m['steel'])
        dz.rotation_euler = (0, .25, -s_*.45)
        dz['purpose'] = 'Reserved for the in-world display from the display branches'
    station('Grey placeholder pilot yoke column', (X(7.75), 0, NEST+.25), (.2, .3, .5), 0, m['grey'], 'Pilot yoke stand')
    cube('Grey placeholder pilot yoke', (X(7.55), 0, NEST+.78), (.1, .7, .08), m['grey'])
    cube('Grey placeholder pilot seat', (X(6.85), 0, NEST+.26), (.62, .62, .52), m['grey'])
    cube('Grey placeholder pilot seat back', (X(6.58), 0, NEST+.7), (.1, .62, .6), m['grey'])
    # --- dome: four faceted rings, the forward facets are glass ---------------------------------------
    RC = (SEAT_U, 0.0)
    prof = [(1.0, RIM), (.80, 3.4), (.55, 3.85), (.28, DOME_TOP)]
    rings = [[(X(RC[0]+(p[0]-RC[0])*sc), RC[1]+(p[1]-RC[1])*sc, z) for p in ROOM8] for sc, z in prof]
    n = 8
    for k in range(3):
        for i in range(n):
            a, b = rings[k][i], rings[k][(i+1) % n]
            c, d = rings[k+1][(i+1) % n], rings[k+1][i]
            glass = i in GLASS_EDGES
            mesh_obj('Dome glass' if glass else 'Dome panel', [a, b, c, d], [(0, 1, 2, 3)], m['glass'] if glass else m['ceil'])
            if not glass:   # outer plating in the hull's own plate colour, a hair outside the ceiling
                out = lambda q: (q[0]+(q[0]-X(RC[0]))*.012, q[1]+q[1]*.012, q[2]+.045)
                mesh_obj('Dome plate', [out(a), out(b), out(c), out(d)], [(0, 1, 2, 3)], m['hull'])
    mesh_obj('Dome cap', rings[3], [tuple(range(n))], m['ceil'])
    mesh_obj('Dome cap plate', [(q[0], q[1], q[2]+.05) for q in rings[3]], [tuple(range(n))], m['hull'])
    th.cube('Dome cap lamp', (X(RC[0]), 0, DOME_TOP+.12), (.5, .5, .12), m['amber'])
    for k in range(4):
        for i in range(n):
            a, b = rings[k][i], rings[k][(i+1) % n]
            beam('Dome ring', a, b, .14 if k else .2, .12 if k else .22, m['teal'])
    for i in range(n):
        for k in range(3):
            beam('Dome rib', rings[k][i], rings[k+1][i], .14, .12, m['teal'])
    for i in range(n):
        a, b = rings[2][i], rings[2][(i+1) % n]
        beam('Dome cove light', (a[0], a[1], a[2]-.08), (b[0], b[1], b[2]-.08), .07, .04, m['amber'])
    # --- lights ----------------------------------------------------------------------------------------------
    th.area_light('Dome light', (C[0], 0, 3.6), (C[0], 0, LEVEL), (1., .70, .38), 700, 1.2)
    th.area_light('Globe glow', (C[0], 0, 2.85), (C[0], 0, LEVEL), (1., .62, .26), 160, .4)
    th.area_light('Rear fill', (X(.8), 0, 2.6), (X(3), 0, LEVEL), (1., .72, .42), 500, 2.0)
    th.area_light('Nest glow', (X(6.8), 0, 1.4), (X(7.5), 0, NEST), (1., .66, .34), 260, 1.2)
    sun = bpy.data.lights.new(PREFIX+' Starfield sun', 'SUN')
    sun.energy = 2.2; sun.color = (.45, .62, 1.); sun.angle = math.radians(1)
    so = bpy.data.objects.new(sun.name, sun)
    th.review.objects.link(so)
    so.rotation_euler = (math.radians(68), 0, math.radians(90))
    th.area_light('Flank spill A', (X(3.5), 5.6, 3.1), (X(3.5), 0, LEVEL), (.45, .62, 1.), 420, 2.4)
    th.area_light('Flank spill B', (X(3.5), -5.6, 3.1), (X(3.5), 0, LEVEL), (.45, .62, 1.), 420, 2.4)
    for o in th.review.objects:
        if o.type == 'LIGHT':
            o.visible_camera = False


def stars(m, review):
    d = bpy.data.meshes.new(PREFIX+' star')
    d.from_pydata([(-.5, 0, -.5), (.5, 0, -.5), (.5, 0, .5), (-.5, 0, .5)], [], [(0, 1, 2, 3)])
    d.materials.append(m['star'])
    seed = 7
    for i in range(220):
        seed = (seed*1103515245+12345) & 0x7fffffff
        az = math.radians(-85+170*(seed % 1000)/1000)
        seed = (seed*1103515245+12345) & 0x7fffffff
        el = math.radians(-50+120*(seed % 1000)/1000)
        r = 70
        pos = (AFT+4+r*math.cos(el)*math.cos(az), r*math.cos(el)*math.sin(az), 1.2+r*math.sin(el))
        o = bpy.data.objects.new(PREFIX+' star', d)
        review.objects.link(o)
        o.location = pos
        o.rotation_euler = (Vector((AFT+4, 0, 1.2))-Vector(pos)).to_track_quat('-Y', 'Z').to_euler()
        o.scale = (.30+.55*(seed % 7)/7,)*3


def planets(m, review):
    for name, loc, r, mat in (('Gas giant', (AFT+270, -95, 22), 24, m['moon']), ('Moon', (AFT+150, 60, 30), 5, m['moon2'])):
        bm = bmesh.new()
        bmesh.ops.create_uvsphere(bm, u_segments=28, v_segments=16, radius=r)
        me = bpy.data.meshes.new(PREFIX+' '+name)
        bm.to_mesh(me)
        bm.free()
        me.materials.append(mat)
        o = bpy.data.objects.new(PREFIX+' '+name, me)
        review.objects.link(o)
        o.location = loc


def make_plan(m):
    s = bpy.data.scenes.new(PREFIX+' | Plan')
    c = bpy.data.collections.new(PREFIX+' plan marks')
    s.collection.children.link(c)
    for o in th.kit.objects:
        if any(t in o.name for t in ['Floor strip', 'Glass floor', 'Captain tier', 'Grey placeholder', 'Floor glow', 'Ramp stripe', 'Slope seam']):
            c.objects.link(o)
    for i in range(len(OUTLINE)):
        a, b = OUTLINE[i], OUTLINE[(i+1) % len(OUTLINE)]
        o = th.beam('Plan wall', (X(a[0]), a[1], .9), (X(b[0]), b[1], .9), .16, .2, m['steel'])
        c.objects.link(o)
        th.kit.objects.unlink(o)
    th.label('BRIDGE  /  LIVE 8.2 x 13 m', (X(4.1), 8.4, 1.5), .40, m['steel'], c)
    th.label('REAR LEVEL +0.00  /  RAMP DOWN TO NEST -0.45 (11 deg)  /  GLASS BELLY  /  DOME TO 4.15', (X(4.1), 7.7, 1.5), .22, m['steel'], c)
    th.label('CAPTAIN +0.30', (X(SEAT_U), 0, 1.6), .26, m['cream'], c)
    th.label('COMPASS GLOBE', (X(SEAT_U), 1.3, 3.2), .22, m['cream'], c)
    th.label('RAMP', (X(5.0), 2.6, 1.6), .26, m['cream'], c)
    th.label('PILOT', (X(7.75), 0, 1.6), .22, m['cream'], c)
    th.label('GLASS FLOOR', (X(7.0), -2.0, 1.6), .2, m['cream'], c)
    th.label('DOOR', (HUB_X-.5, DOORS[0], 1.6), .2, m['cream'], c)
    th.label('DOOR', (HUB_X-.5, -DOORS[0], 1.6), .2, m['cream'], c)
    for i in range(5):
        th.cube('Scale bar', (HUB_X+i, -8.4, .1), (.99, .10, .02), m['steel'] if i % 2 == 0 else m['ochre'], c)
    s.camera = th.camera('03 Top plan', (X(1.5), 0, 28), (X(1.5), 0, 0), ortho=24, scene=s)
    s.render.engine = 'BLENDER_WORKBENCH'
    s.display.shading.light = 'FLAT'; s.display.shading.color_type = 'MATERIAL'
    s.display.shading.show_shadows = False; s.display.shading.show_cavity = True
    s.display.shading.background_type = 'WORLD'
    s.world = bpy.data.worlds.new(PREFIX+' plan paper'); s.world.color = (.82, .81, .74)
    s.view_settings.view_transform = 'Standard'
    s.render.resolution_x = 1600; s.render.resolution_y = 1200; s.render.resolution_percentage = 100
    return s


def clipped_hull(source, name, x_keep):
    """Copy of an evaluated source mesh, cut at x = x_keep and capped. The source object stays untouched."""
    o = source.objects[name]
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(o.evaluated_get(dg))
    me.transform(o.matrix_world)
    bm = bmesh.new()
    bm.from_mesh(me)
    geom = bm.verts[:]+bm.edges[:]+bm.faces[:]
    bmesh.ops.bisect_plane(bm, geom=geom, plane_co=(x_keep, 0, 0), plane_no=(1, 0, 0), clear_outer=True)
    border = [e for e in bm.edges if e.is_boundary]
    if border:
        bmesh.ops.holes_fill(bm, edges=border, sides=64)
    bm.to_mesh(me)
    bm.free()
    me.name = PREFIX+' '+name+' clipped'
    return bpy.data.objects.new(PREFIX+' '+name+' clipped', me)


def check(before, source):
    problems = []
    unchanged = th.fingerprint(source) == before
    if not unchanged:
        problems.append('source ship changed')
    for y in DOORS:   # the first metre inside the pod behind each door stays free
        box = (AFT, AFT+1.0, y-.9, y+.9)
        for o in th.kit.objects:
            pts = [o.matrix_world@Vector(v) for v in o.bound_box]
            lo = [min(p[i] for p in pts) for i in range(3)]; hi = [max(p[i] for p in pts) for i in range(3)]
            if hi[2] <= LEVEL+.04 or lo[2] >= LEVEL+2.2 or not o.name.startswith(PREFIX+' Grey placeholder'):
                continue
            if lo[0] < box[1]-.005 and hi[0] > box[0]+.2 and lo[1] < box[3]-.005 and hi[1] > box[2]+.005:
                problems.append('door approach y=%.2f: %s' % (y, o.name))
    ramp_slope = math.degrees(math.atan((LEVEL-NEST)/(RAMP_U1-RAMP_U0)))
    if ramp_slope > 12:
        problems.append('slope above 12 degrees')
    report = {'problems': problems, 'source_unchanged': unchanged, 'rear_level': LEVEL, 'nest_level': NEST,
              'ramp_slope_deg': round(ramp_slope, 1), 'headroom_rear_level': RIM-LEVEL, 'dome_top': DOME_TOP,
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
    drop = ('Panoramic bridge pane', 'Bridge ', 'BRIDGE deck', 'Sensor ', 'Continuous lower hull')
    for c in source.collection.children:
        if any(t in c.name for t in skip):
            continue
        for o in c.all_objects:
            # The old bridge, its roof and the keel under it make room for the pod; the study draws its own.
            if o.name.startswith(drop) or o.name in shell.objects:
                continue
            shell.objects.link(o)
    hull = clipped_hull(source, 'Continuous lower hull', X(GLASS_FLOOR_U))
    shell.objects.link(hull)
    kit = bpy.data.collections.new(PREFIX+' modular interior'); study.collection.children.link(kit)
    review = bpy.data.collections.new(PREFIX+' lighting'); study.collection.children.link(review)
    th.kit, th.review, th.study, th.box_meshes, th.source = kit, review, study, {}, source
    bpy.context.window.scene = study
    m = dict(floor=th.material('bridge deck blue-grey', (.20, .28, .32), .2),
             hull=th.material('light plate', (.52, .55, .50), .15),
             edge=th.material('plate edge', (.30, .35, .33), .2),
             moon=th.material('planet', (.55, .30, .14), emission=.55),
             moon2=th.material('moon', (.18, .38, .40), emission=.7),
             lining=th.material('warm lining', (.43, .40, .31)),
             teal=th.material('painted petrol', (.075, .19, .175), .18),
             steel=th.material('dark iron', (.045, .065, .064), .35),
             ochre=th.material('ochre edges', (.65, .33, .065), .1),
             amber=th.material('warm diffusers', (1, .62, .25), emission=4),
             grey=th.material('plain grey placeholders', (.28, .29, .28)),
             cream=th.material('plan light ink', (.83, .82, .71)),
             ceil=th.material('ceiling warm grey', (.36, .33, .27)),
             star=th.material('star', (.8, .9, 1.), emission=40),
             brass=th.material('brass', (.78, .50, .16), .85),
             glass=th.material('pod glass', (.35, .55, .65)))
    m['glass'].surface_render_method = 'BLENDED'
    m['glass'].node_tree.nodes['Principled BSDF'].inputs['Alpha'].default_value = .14
    m['glass'].node_tree.nodes['Principled BSDF'].inputs['Roughness'].default_value = .30
    m['glass'].node_tree.nodes['Principled BSDF'].inputs['Specular IOR Level'].default_value = .15
    study.world = bpy.data.worlds.new(PREFIX+' space'); study.world.use_nodes = True
    study.world.node_tree.nodes['Background'].inputs[0].default_value = (.025, .045, .12, 1)
    study.world.node_tree.nodes['Background'].inputs[1].default_value = .6
    study.render.engine = 'BLENDER_EEVEE'
    study.render.resolution_x = 1600; study.render.resolution_y = 1000; study.render.resolution_percentage = 100
    study.view_settings.view_transform = 'AgX'; study.view_settings.look = 'AgX - Medium High Contrast'
    build(m)
    stars(m, review)
    planets(m, review)
    cams = {
        'entry': th.camera('01 Door eye 1.70m', (AFT+.5, DOORS[0], 1.7), (X(5.5), 0.0, .3), lens=20),
        'hero': th.camera('02 Overview from the nest looking aft', (X(7.3), 2.2, NEST+1.7), (X(1.5), -.2, 1.9), lens=18),
        'captain': th.camera('03 Captain seat eye', (X(SEAT_U-.25), 0.0, LEVEL+.3+1.25), (X(8.0), 0.0, NEST+.9), lens=22),
        'pilot': th.camera('04 Pilot seat eye', (X(6.7), 0.0, NEST+1.3), (X(11.0), 0.0, NEST-.8), lens=18),
        'outside': th.camera('05 Outside three-quarter', (X(14), -17, 7.5), (X(1), 0.5, 1.0), lens=26),
        'section': th.camera('06 Section y=0', (X(4.1), -.02, 1.0), (X(4.1), 6.0, 1.0), ortho=16),
    }
    cams['section'].data.clip_start = .05
    cams['pilot'].data.clip_start = .03
    for c_ in cams.values():     # study cameras are fixed: navigating in the viewport must not move them
        c_.lock_location = c_.lock_rotation = c_.lock_scale = (True, True, True)
    study.camera = cams['entry']
    plan = make_plan(m)
    bpy.context.window.scene = study
    bpy.context.view_layer.update()
    report = check(before, source)
    if report['problems']:
        raise RuntimeError('Geometry checks failed')
    if args.render:
        shots = [(study, cams['entry'], '01-door'), (study, cams['hero'], '02-overview'), (study, cams['captain'], '03-captain'), (study, cams['pilot'], '04-pilot'),
                 (study, cams['outside'], '05-outside'), (study, cams['section'], '06-section'), (plan, plan.camera, '07-plan')]
        for s, c, name in shots:
            bpy.context.window.scene = s
            s.camera = c
            s.render.filepath = str(OUT/(name+'.png'))
            bpy.ops.render.render(write_still=True)
        bpy.context.window.scene = study
        study.camera = cams['entry']
    for screen in bpy.data.screens:
        for a in screen.areas:
            if a.type == 'VIEW_3D':
                a.spaces.active.region_3d.view_perspective = 'CAMERA'
                a.spaces.active.overlay.show_overlays = False
                a.spaces.active.shading.type = 'RENDERED'
    print('READY: '+str(OUT))


if __name__ == '__main__':
    main()
