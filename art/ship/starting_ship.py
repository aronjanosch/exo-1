"""Starting ship, revision 4. Original geometry; metres; bow +X, up +Z.

Live: runpy.run_path(..., run_name='__main__'). Re-running replaces only this
script's own scenes (prefix 'Ship v3'); foreign references and older revisions stay.
Headless: blender -b -t 4 -P art/ship/starting_ship.py -- --render --export
The roof-off review links the SAME model objects, with the roof collection omitted.
Architecture only: hull, rooms, doors, glazing. No consoles or furniture.

Height idea: the cabin is one deck at z=0. The hull steps up toward the stern
(wings 2.8, spine 3.4, turret drum 4.6, engine block 3.8) and drops to a low nose,
so the silhouette reads from every side. Roofs are chamfered along their outer
edges; the lower hull is one continuous loft under the whole cabin.

Revision 4 follows the initiator's repainted plan: storage / gear room / airlock on the port side,
storage / crew on the starboard side, weapons (port) and lab (starboard) in front of the turret room, and a larger
reactor level (cargo bay aft, reactor hall forward) that reaches into the sloped keel face.

Footprint: the whole plan is drawn in design coordinates (revision 4) and stretched by FOOT in x and y at the end
of the build (stretch_plan); heights do not change, because people, decks (2.8 m) and doors (2.2 m high) do not
scale. FOOT = 1 rebuilds revision 4 exactly (same geometry hash). Door and window widths stretch with the plan.
Option: --footprint <factor>.
"""
from pathlib import Path
import argparse
import hashlib
import json
import math
import sys
import bpy
import bmesh
from mathutils import Vector, Matrix

ROOT =Path(__file__).resolve().parents[2]
OUT = ROOT / 'build/ship-v3'
OUT.mkdir(parents=True, exist_ok=True)
PREFIX = 'Ship v3'

RAKE = .45      # bridge glazing leans back by this much over its height
EAVE = .5       # horizontal width of a chamfered roof edge
THICK = .23     # roof slab thickness at its rim
RISE = .28      # height the chamfer climbs
PAD = .13       # keel and walls share one outer face: half the wall thickness
SILL, HEAD = .9, 2.1
DRUM_Z = 4.6
BR_SILL = .22   # bridge glazing starts almost at floor level
GROUND = -4.0   # ground plane with the landing skids extended
# Collections that exist only for review and never reach the export or the geometry checks.
REVIEW_ONLY = ('Labels','Labels lower','Guides','Sightline reference','Ground reference','Plan grid',
               'Plan ghost deck','Plan ghost reactor','Gear stowed')
FLOORS = {}
ROOM_EDGES = []
ROOM_LABELS = []
# Reactor level, revision 4: cargo bay aft (at ramp level), reactor hall forward. The hall runs into the
# sloped keel face, so its floor climbs in two risers while the hull stays as it was.
RX0, RX1, RHALF = -6.6, .8, 2.7
CARGO_X = -2.4                  # bulkhead line between cargo bay and reactor hall
REACTOR = [(RX0,-RHALF),(RX1,-RHALF),(RX1,RHALF),(RX0,RHALF)]
BAY = [(-8.2,-1.5),(-6.6,-1.5),(-6.6,1.5),(-8.2,1.5)]
FLOOR_A, FLOOR_T, FLOOR_B = -2.9, -2.725, -2.55   # cargo bay, tread, reactor hall
HALL_X = (-2.4,-2.0)                              # the tread; the hall floor starts at HALL_X[1]
PLINTH_C, PLINTH_R, PLINTH_TOP = (-.6,0), 1.0, -2.25
DOOR_BAY = 1.2                  # half width of the doorway cargo bay -> ramp bay: matches the ramp
HINGE = (-8.2,-2.9)        # foot of the aft ramp: reactor-level floor meets the hull
RAMP_TOP = (-9.8,-.7)      # head of the ramp door in the sloped aft keel
RAMP_HALF = 1.25
SKID_Y = 4.9
STRUTS = (-9.6,-3.6,4.2)        # the middle strut clears the airlock beside the gear room
AIR_X = (-2.8,-.2)              # airlock footprint along x, on the port side in front of the gear room
POD = False                     # pilot pod on the bridge nose; off: the pilot sits on the bridge, --pilot-pod brings it back
# 1.2 was picked over 1.3 by the initiator after the side-by-side with the PULSAR starter ships
# (main deck about 256 -> 363 m2, length 28.6 -> 31.2 m without the pod, same decks).
FOOT = 1.2                      # plan stretch in x and y; --footprint overrides it
PLAN_RES = (1600,1200)          # design pixels; 50 px per metre at any FOOT, see plan_res()
PLAN_SCALE = 32.0          # design metres across the image width: 50 px per metre


def material(name, color, metal=0.0, rough=.5):
    m = bpy.data.materials.new(PREFIX+' '+name)
    m.diffuse_color = (*color, 1)
    m.use_nodes = True
    p = m.node_tree.nodes.get('Principled BSDF')
    p.inputs['Base Color'].default_value = (*color, 1)
    p.inputs['Metallic'].default_value = metal
    p.inputs['Roughness'].default_value = rough
    return m


def mesh(name, verts, faces, mat, group='Structure', bevel=0, mats=None, face_mat=None, recalc=True):
    d = bpy.data.meshes.new(name)
    d.from_pydata(verts, [], faces)
    d.update()
    if recalc:
        bm = bmesh.new(); bm.from_mesh(d)
        bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
        bm.to_mesh(d); bm.free()
    o = bpy.data.objects.new(name, d); collections[group].objects.link(o)
    for m in (mats or [mat]): d.materials.append(m)
    if face_mat:
        for p, i in zip(d.polygons, face_mat): p.material_index = i
    if bevel:
        mod = o.modifiers.new('Edge radii', 'BEVEL')
        mod.width = bevel; mod.segments = 2
    return o


def box(name, loc, size, mat, group='Structure', bevel=.025):
    x,y,z = (a/2 for a in size)
    v = [(a*x,b*y,c*z) for a,b,c in [(-1,-1,-1),(1,-1,-1),(1,1,-1),(-1,1,-1),(-1,-1,1),(1,-1,1),(1,1,1),(-1,1,1)]]
    o = mesh(name,v,[(0,3,2,1),(4,5,6,7),(0,1,5,4),(1,2,6,5),(2,3,7,6),(3,0,4,7)],mat,group,bevel)
    o.location=loc
    return o


def prism(name, poly, bottom, top, mat, group='Structure', bevel=.025):
    n=len(poly)
    return mesh(name,[(x,y,z) for z in (bottom,top) for x,y in poly],
                [tuple(reversed(range(n))),tuple(range(n,2*n))]+[(i,(i+1)%n,(i+1)%n+n,i+n) for i in range(n)],mat,group,bevel)


def beam(name, a, b, width, depth, mat, group='Structure'):
    a,b=Vector(a),Vector(b)
    o=box(name,(a+b)/2,(width,depth,(b-a).length),mat,group,.015)
    o.rotation_euler=(b-a).to_track_quat('Z','Y').to_euler()
    return o


def wall_span(name,a,b,z0,z1,mat,thickness=.18,group='Structure'):
    a,b=Vector(a),Vector(b); mid=(a+b)/2
    o=box(name,(*mid,(z0+z1)/2),((b-a).length,thickness,z1-z0),mat,group)
    o.rotation_euler.z=math.atan2(b.y-a.y,b.x-a.x)
    return o


def cylinder(name,loc,radius,depth,mat,group='Hardware',axis='Z',n=12):
    poly=[(math.cos(i*2*math.pi/n)*radius,math.sin(i*2*math.pi/n)*radius) for i in range(n)]
    o=prism(name,poly,-depth/2,depth/2,mat,group,.015)
    o.location=loc
    if axis=='X': o.rotation_euler.y=math.pi/2
    return o


def text(body,loc,size,mat,group='Labels',rotation=None):
    d=bpy.data.curves.new(body,'FONT'); d.body=body; d.size=size
    d.align_x='CENTER'; d.align_y='CENTER'; d.extrude=.001
    o=bpy.data.objects.new(body,d); collections[group].objects.link(o)
    o.location=loc; d.materials.append(mat)
    if rotation: o.rotation_euler=rotation
    return o


def rect(x0,x1,y0,y1): return [(x0,y0),(x1,y0),(x1,y1),(x0,y1)]


def inward(a,b):
    d=(Vector(b)-Vector(a)).normalized()
    return Vector((-d.y,d.x))


def inset_loop(loop,dists):
    """Mitre offset of a counter-clockwise loop; dists[i] moves edge loop[i]->loop[i+1] inward."""
    n=len(loop); out=[]
    for i in range(n):
        p0,p1,p2=Vector(loop[(i-1)%n]),Vector(loop[i]),Vector(loop[(i+1)%n])
        n0,n1=inward(p0,p1),inward(p1,p2)
        d0,d1=dists[(i-1)%n],dists[i]
        det=n0.x*n1.y-n0.y*n1.x
        if abs(det)<1e-6: q=p1+n0*d0
        else:
            c0,c1=n0.dot(p1)+d0,n1.dot(p1)+d1
            q=Vector(((c0*n1.y-c1*n0.y)/det,(n0.x*c1-n1.x*c0)/det))
        out.append((q.x,q.y))
    return out


def interp(points,x):
    if x<=points[0][0]: return points[0][1]
    for (x0,v0),(x1,v1) in zip(points,points[1:]):
        if x<=x1: return v0+(v1-v0)*((x-x0)/(x1-x0) if x1>x0 else 0)
    return points[-1][1]


def area(poly):
    return abs(sum(a[0]*b[1]-b[0]*a[1] for a,b in zip(poly,poly[1:]+poly[:1])))/2


def fp(p):
    """A design point (x, y) or (x, y, z) in the stretched ship: x and y by FOOT, z as it is."""
    return (p[0]*FOOT,p[1]*FOOT,*p[2:])


def plan_res():
    """Plan image size: the plan stretches with the ship, so the picture grows and stays at 50 px per metre."""
    return (round(PLAN_RES[0]*FOOT),round(PLAN_RES[1]*FOOT))


def pilot_x():
    """Design x of the pilot's eye. The pilot sits 1.7 m behind the glass at any FOOT: that distance is human, so it
    is not stretched. The glass is the nose of the pod (13.1 for FOOT = 1) or the bridge front (10.3); fp stretches
    the returned value again."""
    return (13.1 if POD else 10.3)+1.7*(1-1/FOOT)


def pilot_eye_z():
    """Seated eye height: 1.2 m above the pod floor (-0.95) or the bridge deck (0)."""
    return .25 if POD else 1.2


def captain_x():
    """Design x of the captain's eye. The captain stands 3.3 m behind the bridge front glass at any FOOT, so the
    upward view under the header does not shrink while the roof height stays (8.7 for FOOT = 1)."""
    return 8.7+3.3*(1-1/FOOT)


def ramp_run_rise():
    """The landed aft ramp in the stretched ship: horizontal run, drop to the ground, plate length."""
    hx,hz=HINGE; tx,tz=RAMP_TOP; L=math.hypot(tx-hx,tz-hz); rise=hz-GROUND
    run=math.sqrt(L*L-rise*rise)*FOOT
    return run,rise,math.hypot(run,rise)


def between_posts(eye,posts,step=30):
    """Probe azimuths (degrees) between the posts, seen from the design point eye in the stretched ship. The forward
    half (-90 to 90) is cut at every post bearing, each gap is split into pieces of at most step degrees and gets one
    ray in the middle of each piece. The posts move with FOOT, so hand-picked angles would hit them."""
    ex,ey=eye[0]*FOOT,eye[1]*FOOT
    cuts={-90.0,90.0}|{round(math.degrees(math.atan2(py*FOOT-ey,px*FOOT-ex)),3) for px,py in posts if px*FOOT>ex}
    cuts=sorted(cuts); out=[]
    for lo,hi in zip(cuts,cuts[1:]):
        n=max(1,math.ceil((hi-lo)/step))
        out+=[round(lo+(hi-lo)*(k+.5)/n,1) for k in range(n)]
    return out


def stretch_plan():
    """Stretch every generated mesh and label by FOOT in x and y after the design-space build. The metre grid
    ('Plan grid') is not stretched; plan_overlays builds it at real size. Cameras, lights and probes take
    their positions through fp. FOOT == 1 does nothing, so revision 4 stays bit-identical."""
    if FOOT==1: return
    m=Matrix.Diagonal((FOOT,FOOT,1,1)); done=set()
    for key,c in collections.items():
        if key=='Plan grid': continue
        for o in c.objects:
            if o in done: continue
            done.add(o)
            if o.type=='MESH':
                assert o.data.users==1,o.name
                o.data.transform(m@o.matrix_basis); o.matrix_basis=Matrix.Identity(4)
            elif o.type=='FONT':
                o.location.x*=FOOT; o.location.y*=FOOT


ROOMS = {
    'ENGINE':[(-11,-2),(-10,-3),(-7,-3),(-7,3),(-10,3),(-11,2)],
    'MEDBAY':[(-10,3),(-5,3),(-5,6),(-8.8,6),(-10,4.8)],
    'CREW':[(-10,-4.8),(-8.8,-6),(-3,-6),(-3,-3),(-10,-3)],
    'TURRET':rect(-7,0,-3,3),
    # Port side, aft to forward: storage for small items and reagents, the gear room for outside work.
    'STORAGE 01':rect(-5,-3,3,6),
    'EVA GEAR':rect(-3,0,3,6),
    'STORAGE 02':rect(-3,0,-6,-3),
    # Weapons (port) and lab (starboard) share the old cargo footprint plus the wing beside it. The extra
    # vertices on y=+-3 mark where the raised central ceiling (the lantern) ends.
    'WEAPONS':[(0,0),(5,0),(5,3),(5,4.8),(3.2,6),(0,6),(0,3)],
    'LAB':[(0,-6),(3.2,-6),(5,-4.8),(5,-3),(5,0),(0,0),(0,-3)],
    'BRIDGE':[(5,-4.8),(7,-5.5),(10,-5.5),(12,-3.1),(12,3.1),(10,5.5),(7,5.5),(5,4.8)],
}
# Clear ceiling height per room; a wall rises to the tallest room it touches. Lab and weapons have
# a raised centre strip (|y|<=3, x 0..5) under the spine roof and 2.8 in the wings; see wall_ceiling.
CEIL = {'ENGINE':3.8,'MEDBAY':2.8,'CREW':2.8,'STORAGE 01':2.8,'STORAGE 02':2.8,'EVA GEAR':2.8,
        'LAB':3.4,'WEAPONS':3.4,'TURRET':3.4,'BRIDGE':2.8}
ROOM_TEXT = {'STORAGE 01':'STORAGE','STORAGE 02':'STORAGE'}      # short floor lettering where the room is narrow


def wall_ceiling(room,a,b):
    if room in ('LAB','WEAPONS'):
        mx,my=(a[0]+b[0])/2,(a[1]+b[1])/2
        return CEIL[room] if abs(my)<=3+1e-6 and -1e-6<=mx<=5+1e-6 else 2.8
    return CEIL[room]


# One shared bulkhead per edge; doors are cut into both adjacent room boundaries. The airlock door
# is the only door to the outside; its other side is the gear room.
DOORS=[('ENGINE','TURRET',(-7,0),1.5),
       ('MEDBAY','TURRET',(-6,3),1.4),('CREW','TURRET',(-5,-3),1.4),
       ('STORAGE 01','TURRET',(-4,3),1.2),('EVA GEAR','TURRET',(-1.5,3),1.4),
       ('STORAGE 02','TURRET',(-1.5,-3),1.4),
       ('WEAPONS','TURRET',(0,1.6),1.5),('LAB','TURRET',(0,-1.6),1.5),
       ('STORAGE 02','LAB',(0,-4.8),1.2),
       ('WEAPONS','BRIDGE',(5,1.6),1.5),('LAB','BRIDGE',(5,-1.6),1.5),
       ('EVA GEAR','AIRLOCK',(-1.5,6),1.4)]
# Side windows: edge (sorted endpoints) -> [(centre, width)] measured from the first endpoint.
# Medbay, crew and the storage rooms are windowless; the gear room's outer wall carries the airlock door.
WINDOWS={((0,6),(3.2,6)):[(1.6,2.0)],((3.2,6),(5,4.8)):[(1.08,1.2)],
         ((0,-6),(3.2,-6)):[(1.6,2.0)],((3.2,-6),(5,-4.8)):[(1.08,1.2)]}
# Octagonal turret drum on top of the turret room: it touches the room's four walls.
DRUM=[(-5.5,-3),(-1.5,-3),(0,-1.5),(0,1.5),(-1.5,3),(-5.5,3),(-7,1.5),(-7,-1.5)]
DRUM_WIN={0:[(1.1,1.2),(2.9,1.2)],1:[(1.06,1.3)],2:[(1.5,1.8)],3:[(1.06,1.3)],
          4:[(1.1,1.2),(2.9,1.2)],5:[(1.06,1.3)],7:[(1.06,1.3)]}
BR=ROOMS['BRIDGE']
BR_D=[RAKE]*7+[0]               # edge 7 is the aft wall: no glazing
BR_TOP=inset_loop(BR,BR_D)
BRIDGE_FINS=set()
PILOT=[(12,-1.18),(14,-1.18),(14.8,-.72),(14.8,.72),(14,1.18),(12,1.18)]


def canopy_z(x):
    """Height of the pilot canopy roof: it sinks toward the nose so the captain looks over it."""
    return 1.15-(x-12)/2.8*.25


def on_edge(p,a,b):
    p,a,b=Vector(p),Vector(a),Vector(b); d=b-a
    t=(p-a).dot(d)/d.length_squared
    return -.0001<=t<=1.0001 and (p-a-d*t).length<.001


def window_glass(A,B,sill,head,thick=.30,group='Structure'):
    A,B=Vector(A),Vector(B)
    pane=mesh('Window pane',[(*A,sill),(*B,sill),(*B,head),(*A,head)],[(0,1,2,3)],glass,'Glazing',recalc=False)
    mod=pane.modifiers.new('Laminated glazing thickness','SOLIDIFY'); mod.thickness=.03
    for z in (sill,head): wall_span('Window rail',A,B,z-.05,z+.05,frame,thick,group)
    for p in (A,B): beam('Window upright',(*p,sill),(*p,head),.09,.09,frame,group)


def wall_run(name,P,s0,s1,z0,z1,mat,thick,wins,sill=SILL,head=HEAD,group='Structure'):
    """Wall along P(s) from s0 to s1, with real openings (frame and glass) where wins say so."""
    inside=sorted((c-w/2,c+w/2) for c,w in wins if c-w/2>=s0-1e-6 and c+w/2<=s1+1e-6)
    cur=s0
    for lo,hi in inside:
        if lo>cur+1e-6: wall_span(name,P(cur),P(lo),z0,z1,mat,thick,group)
        wall_span(name,P(lo),P(hi),z0,sill,mat,thick,group)
        wall_span(name,P(lo),P(hi),head,z1,mat,thick,group)
        window_glass(P(lo),P(hi),sill,head,group=group)
        cur=hi
    if s1>cur+1e-6: wall_span(name,P(cur),P(s1),z0,z1,mat,thick,group)


def roof_zone(name,loop,z0,cham,mat,skirt,thick=THICK,rise=RISE,bottom=None,group='Roof',accent=None):
    """Closed roof slab over a counter-clockwise loop. cham[i] is the horizontal width of
    the sloped skirt on edge i (0 = the wall continues upward); bottom[i] insets the underside."""
    n=len(loop); bottom=bottom or [0]*n
    P=inset_loop(loop,bottom)
    Q=inset_loop(loop,[b+c for b,c in zip(bottom,cham)])
    v=[(x,y,z0) for x,y in P]+[(x,y,z0+thick) for x,y in P]+[(x,y,z0+thick+rise) for x,y in Q]
    f=[tuple(reversed(range(n)))]; fm=[0]
    for i in range(n):
        j=(i+1)%n
        f.append((i,j,j+n,i+n)); fm.append(0)
        f.append((i+n,j+n,j+2*n,i+2*n)); fm.append(1 if cham[i]>0 else 0)
    f.append(tuple(range(2*n,3*n))); fm.append(0)
    o=mesh(name,v,f,mat,group,.04,[mat,skirt],fm)
    if accent:
        # A thin saturated line on the upper edge of each sloped skirt marks the layers.
        for i in range(n):
            if cham[i]>0:
                a,b=Q[i],Q[(i+1)%n]; zt=z0+thick+rise+.012
                beam(name+' accent line',(*a,zt),(*b,zt),.07,.03,accent,group)
    return o


def plate(name,poly,base,mat,rise=.1,chamfer=.2,group='Roof'):
    """A layered armour plate lying on a roof: light top, dark shadow edge, chamfered corners."""
    return roof_zone(name,poly,base,[chamfer]*len(poly),mat,hull_edge,thick=rise*.6,rise=rise*.4,group=group)


def architecture():
    # Floor tints follow the colours of the layout drawing: medbay green, storage and gear purple,
    # lab teal, weapons red, crew yellow.
    colors={'ENGINE':(.24,.29,.29),'MEDBAY':(.40,.56,.40),'CREW':(.60,.52,.30),
            'TURRET':(.38,.43,.40),'LAB':(.28,.50,.42),'WEAPONS':(.58,.30,.27),
            'STORAGE 01':(.42,.36,.50),'STORAGE 02':(.42,.36,.50),'EVA GEAR':(.46,.38,.54),
            'BRIDGE':(.28,.38,.42)}
    for name,poly in ROOMS.items():
        mat=material(name+' floor',colors[name],.05,.75); FLOORS[name]=mat
        floorpoly=poly
        if name=='BRIDGE' and POD:
            floorpoly=poly[:4]+[(12,-.85),(9.6,-.85),(9.6,.85),(12,.85)]+poly[4:]
        if name=='TURRET':
            for p in [rect(-7,-4.2,-3,3),rect(-2.8,0,-3,3),rect(-4.2,-2.8,-3,-.7),rect(-4.2,-2.8,.7,3)]:
                prism('Turret deck around ladder',p,-.22,0,mat)
        else: prism(name+' deck',floorpoly,-.22,0,mat)
        cx=sum(x for x,y in poly)/len(poly); cy=sum(y for x,y in poly)/len(poly)
        if name=='TURRET': cx=-5.6;cy=-1.8
        if name=='WEAPONS': cx,cy=2.0,3.0
        if name=='LAB': cx,cy=2.0,-3.0
        text(ROOM_TEXT.get(name,name),(cx,cy,.035),.36,ink); ROOM_LABELS.append((ROOM_TEXT.get(name,name),cx,cy))
    # Split collinear edges at every room vertex, then deduplicate.
    points=set(p for poly in ROOMS.values() for p in poly)
    edges={}
    for room,poly in ROOMS.items():
        for a,b in zip(poly,poly[1:]+poly[:1]):
            pts=sorted((p for p in points if on_edge(p,a,b)),key=lambda p:(Vector(p)-Vector(a)).length)
            for p,q in zip(pts,pts[1:]): edges.setdefault(tuple(sorted((p,q))),[]).append(room)
    ROOM_EDGES[:]=list(edges)
    for (a,b),owners in edges.items():
        outer=len(owners)==1
        if outer and owners[0]=='BRIDGE':
            bridge_window(a,b)
            continue
        h=max(wall_ceiling(r,a,b) for r in owners)
        wins=WINDOWS.get((a,b),[])
        openings=[(Vector(p),w) for r1,r2,p,w in DOORS if on_edge(p,a,b)]
        A=Vector(a); d=Vector(b)-A; length=d.length; u=d/length
        P=lambda s,A=A,u=u: A+u*s
        mat=hull if outer else lining; thick=.26 if outer else .18
        label=' / '.join(owners); cursor=0
        for p,w in sorted(openings,key=lambda pair:(pair[0]-A).length):
            c=(p-A).length; lo=c-w/2; hi=c+w/2
            if lo>cursor: wall_run(label,P,cursor,lo,0,h,mat,thick,wins)
            left,right=P(lo),P(hi)
            wall_span('Door lintel',left,right,2.25,h,lining,group='Roof')
            for v in (left,right): beam('Pressure door jamb',(*v,.05),(*v,2.25),.10,.24,frame)
            wall_span('Door header',left,right,2.20,2.3,frame,.26,'Roof')
            wall_span('Door threshold',left,right,0,.015,brass,.20)
            # Door leaf parked above the opening; volume reserved, no interaction yet.
            wall_span('Retracted pressure door',left,right,2.32,2.7,frame,.07,'Roof')
            cursor=hi
        if cursor<length: wall_run(label,P,cursor,length,0,h,mat,thick,wins)
    # Turret drum: the room's raised central roof, with a ring of slit windows.
    for i,(a,b) in enumerate(zip(DRUM,DRUM[1:]+DRUM[:1])):
        A=Vector(a); d=Vector(b)-A; L=d.length; u=d/L
        wall_run('Turret drum wall',lambda s,A=A,u=u:A+u*s,0,L,CEIL['TURRET'],DRUM_Z,hull,.22,
                 DRUM_WIN.get(i,[]),3.95,4.4,'Roof')
    # Clear central access. Rails have an opening on the forward edge.
    for y in (-.78,.78):
        beam('Ladder coaming rail',(-4.28,y,.8),(-2.72,y,.8),.045,.045,brass)
        for x in (-4.28,-2.72): beam('Coaming stanchion',(x,y,0),(x,y,.8),.045,.045,frame)
    beam('Ladder aft guard',(-4.28,-.78,.8),(-4.28,.78,.8),.045,.045,brass)
    for y in (-.46,.46): beam('Ladder rail',(-4.02,y,-2.9),(-4.02,y,.95),.06,.06,brass)
    for i in range(12): beam('Ladder rung',(-4.02,-.46,-2.68+i*.28),(-4.02,.46,-2.68+i*.28),.045,.045,frame)
    # Wall-side space reservations for future turret stations. No furniture.
    for y in (-2.65,2.65):
        box('Turret station footprint',(-5.8,y,.015),(1.35,.55,.025),brass,'Guides',0)
    # Reactor level: cargo bay aft at ramp level (floor -2.9), reactor hall forward on a stepped floor that
    # climbs with the sloped keel face (tread -2.725, hall -2.55), and a short ramp bay at the aft end that
    # opens onto the aft ramp. Floors are blocks that stay inside the hull: the keel rises toward the bow.
    fa=material('CARGO floor',(.50,.44,.31),.05,.75); FLOORS['CARGO']=fa
    fl=material('REACTOR floor',(.40,.43,.44),.05,.75); FLOORS['REACTOR']=fl
    fb=material('RAMP BAY floor',(.40,.36,.30),.05,.75); FLOORS['RAMP BAY']=fb
    block=lambda name,x0,x1,top,mat,depth=.5: prism(name,rect(x0,x1,-RHALF,RHALF),top-depth,top,mat,'Lower')
    block('Cargo floor',RX0,HALL_X[0],FLOOR_A,fa,.22)
    block('Reactor step tread',HALL_X[0],HALL_X[1],FLOOR_T,fl)
    block('Reactor hall floor',HALL_X[1],RX1,FLOOR_B,fl)
    prism('Ramp bay floor',BAY,FLOOR_A-.22,FLOOR_A,fb,'Lower')
    for x,top in ((HALL_X[0],FLOOR_T),(HALL_X[1],FLOOR_B)):
        wall_span('Step nosing',(x+.06,-RHALF+.1),(x+.06,RHALF-.1),top,top+.015,brass,.10,'Lower')
    cx,cy=PLINTH_C
    plinth=[(cx+PLINTH_R*math.cos(math.radians(22.5+45*i)),cy+PLINTH_R*math.sin(math.radians(22.5+45*i))) for i in range(8)]
    prism('Reactor plinth',plinth,FLOOR_B-.05,PLINTH_TOP,fl,'Lower',.04)
    for a,b in zip(plinth,plinth[1:]+plinth[:1]): wall_span('Plinth nosing',a,b,PLINTH_TOP,PLINTH_TOP+.015,brass,.07,'Lower')
    for a,b in zip(REACTOR,REACTOR[1:]+REACTOR[:1]):
        if a[0]==b[0]==RX0:
            # Aft wall of the cargo bay: a doorway as wide as the ramp (2.4 m) into the ramp bay.
            wall_span('Cargo bay liner',(RX0,-RHALF),(RX0,-DOOR_BAY),FLOOR_A,-.23,lining,.16,'Lower')
            wall_span('Cargo bay liner',(RX0,DOOR_BAY),(RX0,RHALF),FLOOR_A,-.23,lining,.16,'Lower')
            wall_span('Cargo doorway header',(RX0,-DOOR_BAY),(RX0,DOOR_BAY),-.6,-.23,lining,.16,'Lower')
            for y in (-DOOR_BAY,DOOR_BAY): beam('Cargo doorway jamb',(RX0,y,FLOOR_A),(RX0,y,-.6),.10,.20,frame,'Lower')
        else: wall_span('Reactor protective liner',a,b,FLOOR_A,-.23,lining,.16,'Lower')
    for y in (-1.5,1.5): wall_span('Ramp bay liner',(-8.2,y),(RX0,y),FLOOR_A,-.23,lining,.16,'Lower')
    # Review-only envelope reserves machine and maintenance space.
    cylinder('Reactor reserved volume',(cx,cy,(PLINTH_TOP-.7)/2),.7,-.7-PLINTH_TOP,brass,'Guides')
    text('CARGO',(-5.0,1.9,FLOOR_A+.03),.4,ink,'Labels lower')
    text('REACTOR',(-.2,1.95,FLOOR_B+.03),.4,ink,'Labels lower')
    text('RAMP BAY',(-7.4,0,FLOOR_A+.03),.3,ink,'Labels lower')
    # Cockpit floor, its short bridge ramp and protective side cheeks.
    if not POD:
        text('PILOT',(pilot_x(),0,.035),.3,ink)       # the pilot station is the front centre of the bridge
        return
    prism('Pilot lowered deck',PILOT,-1.12,-.95,dark)
    mesh('Pilot access ramp',[(9.6,-.85,0),(12,-.85,-.95),(12,.85,-.95),(9.6,.85,0)],[(0,1,2,3)],dark)
    for y in (-.91,.91):
        beam('Ramp handrail',(9.65,y,.8),(12,y,-.15),.045,.045,brass)
        mesh('Ramp cheek',[(9.6,y,-.17),(12,y,-1.12),(12,y,-.95),(9.6,y,0)],[(0,1,2,3)],lining)
    for a,b in zip(PILOT,PILOT[1:]+PILOT[:1]):
        if a[0]==b[0]==12: continue
        wall_span('Pilot floor rim',a,b,-.95,-.80,frame,.14)
        o=mesh('Pilot clear pane',[(*a,-.80),(*b,-.80),(*b,canopy_z(b[0])),(*a,canopy_z(a[0]))],[(0,1,2,3)],glass,'Glazing',recalc=False)
        mod=o.modifiers.new('Laminated glazing thickness','SOLIDIFY'); mod.thickness=.025
        beam('Pilot glazing mullion',(*a,-.80),(*a,canopy_z(a[0])+.03),.075,.075,frame)
        beam('Pilot canopy rim',(*a,canopy_z(a[0])+.03),(*b,canopy_z(b[0])+.03),.09,.09,frame)
    o=mesh('Pilot overhead glazing',[(x,y,canopy_z(x)) for x,y in PILOT],[tuple(range(len(PILOT)))],glass,'Glazing',recalc=False)
    mod=o.modifiers.new('Laminated glazing thickness','SOLIDIFY'); mod.thickness=.025
    beam('Canopy spine',(12,0,canopy_z(12)+.05),(14.8,0,canopy_z(14.8)+.05),.065,.065,frame)
    text('PILOT',(13.25,0,-.92),.3,white)


def bridge_top(p):
    """Where the glazing above bridge-outline point p ends: p moved inward by RAKE, at the roof line."""
    for i,(a,b) in enumerate(zip(BR,BR[1:]+BR[:1])):
        if BR_D[i]==0 or not on_edge(p,a,b): continue
        if math.dist(p,a)<1e-6: return (*BR_TOP[i],2.8)
        if math.dist(p,b)<1e-6: return (*BR_TOP[(i+1)%len(BR)],2.8)
        n=inward(a,b); return (p[0]+n.x*RAKE,p[1]+n.y*RAKE,2.8)
    raise ValueError(p)


def bridge_window(a,b):
    def glaze(p,q,bottom):
        """Raked panes between p and q, a mullion about every 2.3 m."""
        n=max(1,math.ceil(math.dist(p,q)/2.3))
        pts=[(p[0]+(q[0]-p[0])*k/n,p[1]+(q[1]-p[1])*k/n) for k in range(n+1)]
        for s,t in zip(pts,pts[1:]):
            o=mesh('Panoramic bridge pane',[(*s,bottom),(*t,bottom),bridge_top(t),bridge_top(s)],[(0,1,2,3)],glass,'Glazing',recalc=False)
            mod=o.modifiers.new('Laminated glazing thickness','SOLIDIFY'); mod.thickness=.02
        for s in pts[1:-1]: beam('Bridge glazing mullion',(*s,bottom),bridge_top(s),.07,.07,frame)
    sill=BR_SILL
    if POD and a[0]==b[0]==12:
        # The forward centre has a low doorway to the recessed pilot alcove.
        for p,q in [(a,(12,-1.18)),((12,1.18),b)]:
            wall_span('Bridge front sill',p,q,0,sill,hull,.25)
            glaze(p,q,sill)
            beam('Bridge rub rail',(*p,sill),(*q,sill),.13,.13,frame)   # not across the portal
        o=mesh('Bridge pane over cockpit',[(12,-1.18,1.22),(12,1.18,1.22),bridge_top((12,1.18)),bridge_top((12,-1.18))],[(0,1,2,3)],glass,'Glazing',recalc=False)
        mod=o.modifiers.new('Laminated glazing thickness','SOLIDIFY'); mod.thickness=.02
        beam('Pilot portal header',(12,-1.18,1.16),(12,1.18,1.16),.10,.10,frame)
        for y in (-1.18,1.18):
            beam('Portal side frame',(12,y,-.95),(12,y,1.16),.09,.09,frame)
            beam('Portal upper frame',(12,y,1.16),bridge_top((12,y)),.09,.09,frame)
    else:
        wall_span('Bridge window sill',a,b,0,sill,hull,.25)
        glaze(a,b,sill)
    for v in (a,b):
        if v in BRIDGE_FINS: continue
        BRIDGE_FINS.add(v); t=bridge_top(v); out=(Vector(v)-Vector(t[:2])).normalized()
        beam('Bridge cage post',(*v,sill),t,.20,.20,frame)
        beam('Bridge cage fin',(v[0]+out.x*.14,v[1]+out.y*.14,sill+.05),(t[0]+out.x*.10,t[1]+out.y*.10,2.78),.05,.26,frame)
    beam('Bridge roof rim',bridge_top(a),bridge_top(b),.22,.22,frame,'Roof')
    if not (POD and a[0]==b[0]==12): beam('Bridge rub rail',(*a,sill),(*b,sill),.13,.13,frame)


def roofs():
    E=EAVE
    PORT=[(-10,3),(5,3),(5,4.8),(3.2,6),(-8.8,6),(-10,4.8)]
    STBD=[(-10,-3),(-10,-4.8),(-8.8,-6),(3.2,-6),(5,-4.8),(5,-3)]
    roof_zone('Wing roof port',PORT,2.8,[0,0,E,E,E,E],hull,teal)
    roof_zone('Wing roof starboard',STBD,2.8,[E,E,E,E,0,0],hull,teal)
    roof_zone('Spine roof',[(-1.5,-3),(5,-3),(5,3),(-1.5,3),(0,1.5),(0,-1.5)],3.4,[E,E,E,0,0,0],hull_lt,teal)
    # Lab and weapons rooms share the raised centre strip under the spine roof; its side walls stand on the wing roofs.
    for y in (-3,3): wall_span('Lantern wall',(0,y),(5,y),2.8,3.4,hull,.18,'Roof')
    roof_zone('Turret corner roof port',[(-7,1.5),(-5.5,3),(-7,3)],3.4,[0,E,0],hull,teal)
    roof_zone('Turret corner roof starboard',[(-7,-3),(-5.5,-3),(-7,-1.5)],3.4,[E,0,0],hull,teal)
    roof_zone('Turret drum roof',DRUM,DRUM_Z,[E]*8,hull_lt,teal,accent=brass)
    roof_zone('Engine roof',ROOMS['ENGINE'],3.8,[E]*6,hull_dk,teal)
    roof_zone('Bridge roof',BR,2.8,[E]*7+[0],hull,teal,bottom=BR_D,accent=brass)


def loft(name,rings,mat,group='Hull',cap=True):
    # Eight-point cross-section, with deliberate upper and lower chines.
    verts=[]
    for x,w,z0,z1 in rings:
        h=z1-z0
        verts += [(x,-w*.70,z0),(x,w*.70,z0),(x,w,z0+h*.27),(x,w,z1-h*.20),
                  (x,w*.73,z1),(x,-w*.73,z1),(x,-w,z1-h*.20),(x,-w,z0+h*.27)]
    faces=[]
    if cap: faces=[tuple(reversed(range(8))),tuple(range((len(rings)-1)*8,len(rings)*8))]
    for r in range(len(rings)-1):
        for i in range(8): faces.append((r*8+i,r*8+(i+1)%8,(r+1)*8+(i+1)%8,(r+1)*8+i))
    return mesh(name,verts,faces,mat,group,.05)


def hull_loft(name,rings,thickness,seg_mat,front_notch=None,hole=()):
    """Open-top shell over rings that run left top, left belt, left bottom ... right top, thickened
    inward. seg_mat gives the material index per ring segment (0 hull, 1 armour, 2 dark).
    front_notch=(half_width,depth) cuts the front cap open for a passage at the top centre;
    hole is a set of (station, segment) faces that are left out, e.g. the ramp opening."""
    n=len(rings[0]); verts=[p for ring in rings for p in ring]; faces=[]; fm=[]
    for k in range(len(rings)-1):
        for j in range(n-1):
            if (k,j) in hole: continue
            faces.append((k*n+j,k*n+j+1,(k+1)*n+j+1,(k+1)*n+j)); fm.append(seg_mat[j])
    faces.append(tuple(reversed(range(n)))); fm.append(1)
    base=(len(rings)-1)*n; cap=tuple(range(base,base+n))
    if front_notch:
        nw,nd=front_notch; xe=rings[-1][0][0]; a=len(verts)
        verts+=[(xe,nw,0),(xe,nw,-nd),(xe,-nw,-nd),(xe,-nw,0)]
        cap+=(a,a+1,a+2,a+3)
    faces.append(cap); fm.append(1)
    o=mesh(name,verts,faces,hull,'Keel',0,[hull,teal,dark],fm,recalc=False)
    mod=o.modifiers.new('Hull plating','SOLIDIFY'); mod.thickness=thickness; mod.offset=-1; mod.use_even_offset=True
    return o


# Keel depth and cabin half-width along the ship (piecewise linear, so the lower hull is faceted).
# The sloped face between x=-9.8 and x=-8.2 carries the aft ramp (HINGE to RAMP_TOP).
KEEL_Z=[(-11,-.7),(-9.8,-.7),(-8.2,-2.9),(-8.1,-3.55),(-.4,-3.55),(4.4,-1.7),(7,-1.25),(12,-1.25)]
KEEL_W=[(-11,2),(-10,3),(-9.97,4.8),(-8.8,6),(3.2,6),(5,4.8),(7,5.5),(10,5.5),(12,3.1)]


def lower_hull():
    """One continuous loft under the whole cabin: the same outline as the walls, carried down
    through an armour belt and a chamfered keel. The reactor level sits in its deepest stretch;
    the sloped aft face has an opening for the ramp."""
    xs=sorted(set(p[0] for p in KEEL_Z+KEEL_W))
    rings=[]; h=RAMP_HALF
    for x in xs:
        W=interp(KEEL_W,x)+PAD; zb=interp(KEEL_Z,x); D=-zb
        belt=min(.7,.45*D); z1=-belt; i1=.12; i2=i1+.86*(D-belt)
        xe=x+(PAD if x==xs[-1] else 0)-(PAD if x==xs[0] else 0)
        rings.append([(xe,-W,0),(xe,-(W-i1),z1),(xe,-(W-i2),zb),(xe,-h,zb),(xe,h,zb),(xe,W-i2,zb),(xe,W-i1,z1),(xe,W,0)])
    hole={(k,3) for k,x in enumerate(xs[:-1]) if RAMP_TOP[0]-1e-6<=x<HINGE[0]-1e-6}
    hull_loft('Continuous lower hull',rings,.32,(0,1,2,2,2,1,0),front_notch=(1.245,.95),hole=hole)
    if not POD: return
    # Chin fairing under the pilot alcove, from the same family of shapes.
    cw=[(12,1.18),(14,1.18),(14.8,.72)]; cz=[(12,-1.35),(14,-1.35),(14.8,-1.25)]
    chin=[]
    for x in (12,14,14.8):
        W=interp(cw,x)+.065; zb=interp(cz,x); zt=-.95
        chin.append([(x,-W,zt),(x,-(W-.1),zt-.3),(x,-(W-.38),zb),(x,W-.38,zb),(x,W-.1,zt-.3),(x,W,zt)])
    hull_loft('Pilot chin fairing',chin,.2,(0,1,2,1,0))


def slab(name,p0,p1,w,h,mat,group,bevel=.03):
    """Box from p0 to p1 with cross-section w x h; its local Z stays as upright as possible."""
    p0,p1=Vector(p0),Vector(p1)
    o=box(name,(p0+p1)/2,((p1-p0).length,w,h),mat,group,bevel)
    o.rotation_euler=(p1-p0).to_track_quat('X','Z').to_euler()
    return o


def ramp(pose):
    """Aft ramp: a door in the sloped keel face, hinged at the reactor-level floor. 'down' lies on the
    ground (landed), 'up' fills the opening (flight)."""
    group='Gear' if pose=='down' else 'Gear stowed'
    hx,hz=HINGE; tx,tz=RAMP_TOP; L=math.hypot(tx-hx,tz-hz)
    if pose=='up': d=Vector(((tx-hx)/L,(tz-hz)/L))
    else:
        rise=hz-GROUND; d=Vector((-math.sqrt(L*L-rise*rise)/L,-rise/L))
    n=Vector((d.y,-d.x))                       # the walking side of the plate
    P=lambda s,y=0,off=0: (hx+d.x*s+n.x*off,y,hz+d.y*s+n.y*off)
    slab('Aft ramp plate',P(0),P(L),2.4,.12,hull_dk,group)
    for s in (.35,.75,1.15,1.55,1.95,2.35):
        slab('Ramp cleat',P(s,-1.05,.08),P(s,1.05,.08),.07,.05,brass,group,.01)
    for y in (-1.2,1.2): slab('Ramp edge rail',P(.05,y,.2),P(L-.05,y,.2),.06,.32,frame,group,.01)
    c=cylinder('Ramp hinge',(hx,0,hz),.1,2.6,frame,group); c.rotation_euler.x=math.pi/2
    if pose=='down':
        # Opening frame in the keel face; the same frame is visible in both poses, so build it once.
        h=RAMP_HALF
        for y in (-h,h): beam('Ramp opening frame',(hx,y,hz),(tx,y,tz),.14,.14,frame,'Keel')
        beam('Ramp opening head',(tx,-h,tz),(tx,h,tz),.14,.14,frame,'Keel')


def gear(pose):
    """Landing skids on three struts per side. 'down' stands the ship on GROUND (landed), 'up' folds
    the runners against the hull flank (flight). Greybox: the recesses in the hull come with the animation."""
    group='Gear' if pose=='down' else 'Gear stowed'
    ry,rz=(SKID_Y,-3.85) if pose=='down' else (5.5,-1.45)
    for s in (-1,1):
        y=s*ry
        slab('Landing skid',(-10.0,y,rz),(5.4,y,rz),.56,.30,dark,group,.05)
        slab('Landing skid tip front',(5.4,y,rz),(6.5,y,rz+.45),.56,.30,dark,group,.05)
        slab('Landing skid tip rear',(-10.0,y,rz),(-10.8,y,rz+.35),.56,.30,dark,group,.05)
        for x in STRUTS:
            W=interp(KEEL_W,x)+PAD
            A=Vector((x,s*(W+.15),-.4)); T=Vector((x,y,rz+.15)); M=A+(T-A)*.55
            slab('Gear sleeve',A,M,.58,.58,hull_dk,group,.08)
            slab('Gear piston',M,T,.32,.32,frame,group,.04)
            box('Gear hinge block',tuple(A),(.95,.85,.65),frame,group,.06)
            box('Gear foot block',tuple(T),(.85,.7,.26),frame,group,.05)


NY=8.4          # nacelle centre line: a gap of about 0.65 m separates pod and hull


def nacelles():
    for sign in (-1,1):
        # Pods hang on visible pylons with an X brace between them: exposed structure, negative space.
        for z0,z1,name in [(1.25,2.35,'Upper engine pylon'),(-.7,.2,'Lower engine pylon')]:
            box(name,(-6.9,sign*6.45,(z0+z1)/2),(2.8,1.0,z1-z0),hull_dk,'Hull',.10)
        for (xa,za),(xb,zb) in [((-8.2,.2),(-5.6,1.25)),((-8.2,1.25),(-5.6,.2))]:
            beam('Pylon brace',(xa,sign*6.45,za),(xb,sign*6.45,zb),.12,.12,frame,'Hull')
        nac=loft('Engine nacelle',[(-13.5,1.28,-.7,2.8),(-12.8,1.62,-.95,3.15),
            (-7,1.62,-.95,3.15),(-5.0,1.32,-.55,2.85),(-4.3,.78,.0,2.2)],hull)
        nac.location.y=sign*NY
        for x in (-12.5,-7.4):
            collar=loft('Nacelle steel collar',[(x-.16,1.66,-.99,3.19),(x+.16,1.66,-.99,3.19)],frame)
            collar.location.y=sign*NY
        for x,r,dep,mat in [(-13.55,1.05,.16,frame),(-13.65,.78,.06,dark),(-13.7,.55,.03,glow)]:
            cylinder('Recessed engine exhaust',(x,sign*NY,1.05),r,dep,mat,'Hull','X')
        for i in range(6): box('Engine cooling vane',(-10.9+i*.52,sign*NY,3.16),(.22,1.7,.075),dark,'Hull',.025)
        fy=sign*(NY+1.61)
        box('Nacelle ochre flank',(-9.7,fy,1.15),(3.25,.06,1.30),brass,'Hull',.07)
        box('Nacelle light slit',(-9.7,fy,2.25),(3.25,.04,.12),glow,'Hull',.02)
        box('Nacelle lower rail',(-10,sign*(NY+1.25),-.50),(4.9,.16,.22),frame,'Hull',.04)
        if sign==1:
            # Asymmetry: a lashed-on spare tank, the kind of fix a working crew adds.
            cylinder('Auxiliary tank',(-9.4,NY+2.2,-.2),.55,4.0,teal,'Hull','X')
            for x in (-10.8,-8.0): box('Tank strap',(x,NY+2.2,-.2),(.22,1.2,1.2),frame,'Hull',.03)
            for x in (-10.8,-8.0): box('Tank bracket',(x,NY+1.65,-.2),(.22,.5,.3),frame,'Hull',.03)
        # Ribs and an identification stripe on the wing walls, clear of the windows.
        for x in (-4.45,3.0): box('Hull rib',(x,sign*6.13,1.2),(.16,.19,2.4),frame,'Hull',.02)
        for xa,xb in ([(-4.1,1.3)] if sign<0 else [(-4.1,AIR_X[0]-.1),(AIR_X[1]+.1,1.3)]):
            box('Hull identification stripe',((xa+xb)/2,sign*6.17,.45),(xb-xa,.03,.14),brass,'Hull',.02)


def turret(x,y,z,scale):
    cylinder('Turret mounting ring',(x,y,z+.09),.8*scale,.18,frame,'Roof')
    box('Turret housing',(x,y,z+.18+.25*scale),(1.4*scale,1.05*scale,.5*scale),hull,'Roof',.14)
    for dy in (-.24,.24):
        beam('Turret barrel',(x+.4,y+dy*scale,z+.18+.25*scale),(x+1.95*scale,y+dy*scale,z+.18+.25*scale),.10*scale,.10*scale,frame,'Roof')


def exterior():
    lower_hull(); nacelles()
    top=lambda r: CEIL[r]+THICK+RISE
    wing=2.8+THICK+RISE
    # Layered armour plates in two grey tones, as on a patched-up working hull.
    plate('Wing plate port',[(-8.6,3.5),(-3.4,3.5),(-3.4,5.3),(-8.0,5.3),(-8.6,4.8)],wing,hull_lt)
    plate('Wing plate port forward',[(-2.4,3.3),(4.0,3.3),(4.0,5.0),(2.9,5.2),(-2.4,5.2)],wing,hull_dk)
    plate('Wing plate starboard',[(-8.4,-5.3),(-1.0,-5.3),(-1.0,-3.4),(-8.4,-3.4)],wing,hull_dk)
    plate('Wing plate starboard forward',[(.2,-5.1),(3.9,-5.1),(3.9,-3.4),(.2,-3.4)],wing,hull_lt)
    plate('Bridge roof plate',[(6.4,-2.8),(9.6,-2.8),(10.6,-1.8),(10.6,1.8),(9.6,2.8),(6.4,2.8)],wing,hull_lt,.12,.3)
    plate('Engine roof plate',[(-9.7,-2.2),(-7.5,-2.2),(-7.5,2.2),(-9.7,2.2)],top('ENGINE'),hull_lt)
    # Spine roof: a visual 2x2 hatch field with ochre latches. Crates come in over the aft ramp, so these stay decoration.
    cz=top('LAB')
    for x0 in (.8,2.55):
        for y0 in (-2.1,.1):
            plate('Roof hatch',rect(x0,x0+1.6,y0,y0+2.0),cz,hull_dk,.08,.08)
            box('Hatch latch',(x0+.8,y0+(1.9 if y0>0 else .1),cz+.07),(.5,.1,.04),brass,'Roof',.01)
    # Restrained hardware: a main turret on the drum, a smaller one on the engine block.
    turret(-3.5,0,DRUM_Z+THICK+RISE,1.0)
    turret(-8.6,0,top('ENGINE')+.1,.7)
    # Radiator fins, a dish and a sensor mast: worked-on, slightly crooked.
    for i in range(3): box('Radiator fin',(-10.4+i*.42,.0,top('ENGINE')+.2),(.1,2.0,.4),dark,'Roof',.02)
    d=cylinder('Comms dish',(-8.3,-3.0,top('ENGINE')+.9),.55,.07,white,'Roof'); d.rotation_euler=(.5,.6,0)
    beam('Dish mast',(-8.3,-3.0,top('ENGINE')),(-8.3,-3.0,top('ENGINE')+.85),.08,.08,frame,'Roof')
    beam('Sensor mast',(7.0,3.6,2.8+THICK+RISE),(7.0,3.6,4.5),.08,.08,frame,'Roof')
    box('Sensor head',(7.0,3.6,4.55),(.5,.3,.2),brass,'Roof',.04)
    for x,y,sx,sy in [(-5.9,4.4,1.2,1.0)]:
        box('Roof service hatch',(x,y,wing+.2),(sx,sy,.1),hull_dk,'Roof',.03)
    # Landing skids and the aft ramp, both poses; the stowed pose lives in its own collection.
    gear('down'); gear('up'); ramp('down'); ramp('up')
    box('Ground reference',(0,0,GROUND-.1),(70,70,.2),material('ground',(.34,.32,.26),0,1),'Ground reference',0)
    # Airlock on the port flank in front of the gear room (revision 4: moved from the lab side per the
    # layout drawing). Clear of the engine pods; the middle landing strut moved aft to make room.
    x0,x1=AIR_X; xc=(x0+x1)/2
    p=rect(x0,x1,6,8.25)
    prism('Airlock deck',p,-.22,0,material('AIRLOCK floor',(.30,.35,.36),.05,.75))
    roof_zone('Airlock roof',p,2.8,[0,.35,.35,.35],hull,teal,thick=.2,rise=.15)
    for x in (x0,x1): wall_span('Airlock side',(x,6),(x,8.25),0,2.8,hull,.24)
    for a,b in [((x0,8.25),(x0+.6,8.25)),((x1-.6,8.25),(x1,8.25))]: wall_span('Airlock outer jamb',a,b,0,2.8,hull,.24)
    wall_span('Airlock outer header',(x0+.6,8.25),(x1-.6,8.25),2.25,2.8,frame,.24)
    wall_span('Airlock outer door',(x0+.6,8.25),(x1-.6,8.25),0,2.25,frame,.10,'Doors')
    text('AIRLOCK',(xc,7.1,.04),.25,white)
    # Small docking collar, no invented full cargo ramp.
    for x in (xc-1.05,xc+1.05): box('Dock collar side',(x,8.45,1.2),(.18,.4,2.65),brass,'Hull',.04)
    box('Dock collar header',(xc,8.45,2.5),(2.25,.4,.18),brass,'Hull',.04)


def camera(scene,name,loc,target,lens=35,ortho=None):
    loc,target=fp(loc),fp(target)               # given in design coordinates
    d=bpy.data.cameras.new(name); o=bpy.data.objects.new(name,d)
    scene.collection.objects.link(o); o.location=loc
    o.rotation_euler=(Vector(target)-Vector(loc)).to_track_quat('-Z','Y').to_euler()
    d.lens=lens; d.clip_start=.05; d.clip_end=500
    if ortho: d.type='ORTHO';d.ortho_scale=ortho*FOOT
    return o


def lighting(scene):
    scene.render.engine='CYCLES'; scene.cycles.samples=32
    scene.cycles.use_denoising=True
    scene.render.resolution_x=1600;scene.render.resolution_y=1100;scene.render.resolution_percentage=100
    scene.world=bpy.data.worlds.new(PREFIX+' studio');scene.world.use_nodes=True
    bg=scene.world.node_tree.nodes['Background']; bg.inputs[0].default_value=(.23,.29,.36,1);bg.inputs[1].default_value=.65
    scene.view_settings.view_transform='AgX'
    for pos,power,size in [((5,-14,22),5500,14),((-8,15,16),4500,12),((17,6,10),2000,10)]:
        d=bpy.data.lights.new('Softbox','AREA');d.energy=power;d.shape='DISK';d.size=size
        o=bpy.data.objects.new('Softbox',d);scene.collection.objects.link(o);o.location=fp(pos)
        o.rotation_euler=(-o.location).to_track_quat('-Z','Y').to_euler()
    # Architectural lighting, embedded in the ceiling rather than floating props.
    for pos in [(8,0,2.65),(-3,0,3.2),(-9,0,3.4),(2,1.5,3.2),(2,-1.5,3.2),(2.2,4.5,2.5),(2.2,-4.5,2.5),(-1.5,4.5,2.5),(-1.5,-4.5,2.5),
                (-4.5,4.5,2.5),(-6.5,-4.5,2.5),(-6.5,4.5,2.5),(-4.5,0,-.35),(-.6,0,-.35),(-7.4,0,-.35)]:
        d=bpy.data.lights.new('Cabin ceiling light','AREA');d.energy=65;d.shape='DISK';d.size=2
        o=bpy.data.objects.new('Cabin ceiling light',d);scene.collection.objects.link(o);o.location=fp(pos)


def sightline_backdrop():
    """Review-only bearing posts make transparency and the horizon observable."""
    mat=material('sightline ground',(.28,.32,.31),0,1)
    box('Review ground',(8,0,-7),(160,160,.2),mat,'Sightline reference',0)
    cx=captain_x()*FOOT     # the eye in real metres: the circle is round there, and stretch_plan scales design points
    for angle in range(-120,121,15):
        r=math.radians(angle);x=cx+28*math.cos(r);y=28*math.sin(r)
        box('Distant bearing post',(x/FOOT,y/FOOT,-1.4),(.35,.35,11),brass if angle==0 else white,'Sightline reference',0)
        t=text(f'{abs(angle):03d}',(x/FOOT,y/FOOT,4.7),1.0,white,'Sightline reference')
        t.rotation_euler=Vector((cx-x,-y,0)).to_track_quat('Z','Y').to_euler()
    for a in range(-120,120,5):
        p=[]
        for angle in (a,a+5):
            r=math.radians(angle);p.append(((cx+28*math.cos(r))/FOOT,28*math.sin(r)/FOOT,1.65))
        beam('Captain horizon reference',p[0],p[1],.035,.035,brass,'Sightline reference')


def check_geometry(scene):
    """Geometry checks only. This does not claim game collision or ladder support."""
    from mathutils.bvhtree import BVHTree
    bpy.context.window.scene=scene;bpy.context.view_layer.update()
    dg=bpy.context.evaluated_depsgraph_get();verts=[];faces=[];triangles=0
    for k,c in collections.items():
        if k=='Glazing' or k in REVIEW_ONLY:continue
        for obj in c.objects:
            if obj.type!='MESH':continue
            ev=obj.evaluated_get(dg);m=ev.to_mesh();offset=len(verts)
            verts.extend(obj.matrix_world@v.co for v in m.vertices)
            # Triangles, not n-gons: BVH fans concave polygons and would invent surfaces.
            m.calc_loop_triangles()
            faces.extend(tuple(offset+i for i in t.vertices) for t in m.loop_triangles)
            triangles+=len(m.loop_triangles);ev.to_mesh_clear()
    tree=BVHTree.FromPolygons(verts,faces)
    problems=[];rays=[]
    PILOT_X=pilot_x(); PILOT_Z=pilot_eye_z()
    probes=[
        # Captain: between the corner posts (about +-43 degrees at FOOT 1), not through them, and up under the header.
        ('captain',(captain_x(),0,1.65),[(a,0) for a in between_posts((captain_x(),0),BR)]+[(-20,15),(0,15),(20,15)]),
        # Pilot: forward between the posts (pod nose or bridge cage), down to the floor rim or sill, up through the roof.
        ('pilot',(PILOT_X,0,PILOT_Z),[(a,0) for a in [0]+between_posts((PILOT_X,0),PILOT if POD else BR)]+([(-25,45),(25,45)] if POD else [(-25,30),(25,30)])+
                 ([(0,-28),(-40,-22),(40,-22)] if POD else [(0,-14),(-45,-12),(45,-12)])),
        # Standing eye behind the side windows: they must be real openings. Oblique rays toward
        # the nacelle and the airlock are blocked by design, so only the outward fan is probed.
        ('weapons window',(1.6,4.6,1.5),[(90,0),(80,0),(100,0)]),
        ('weapons corner window',(3.3,4.2,1.5),[(56,0),(46,0),(66,0)]),
        ('lab window',(1.6,-4.6,1.5),[(-90,0),(-80,0),(-100,0)]),
        ('lab corner window',(3.3,-4.2,1.5),[(-56,0),(-46,0),(-66,0)])]
    # Walking out aft: three body widths and heights along the reactor level, through the doorway,
    # the ramp bay and the opening in the keel (the open ramp lies below).
    if POD:
        # Back through the portal into the bridge: 5.5 m, nothing may cross the doorway.
        probes+=[('pilot back',(PILOT_X,0,.25),[(180,0)],5.5),('pilot back high',(PILOT_X,0,.9),[(180,0)],5.5)]
    for yy in (-1.0,-.4,0,.4,1.0):
        for hh in (.3,.9,1.7):
            probes.append((f'ramp path y{yy} h{hh}',(-6.0,yy,-2.9+hh),[(180,0)]))
    # Cargo bay at body height along both sides of the ladder, from the doorway to the front bulkhead.
    for yy in (-1.8,-1.2,1.2,1.8):
        for z in (-2.0,-1.0):
            probes.append((f'cargo walk y{yy} z{z}',(RX0+.3,yy,z),[(0,0)],RX1-RX0-.4))
    for label,eye,angles,*rest in probes:
        reach=(rest[0] if rest else 60)*FOOT; eye=fp(eye)       # probes are written in design space
        for az,el in angles:
            # A ray exactly through a thin mullion is not an obstruction: two of three neighbours must be clear.
            clears=[];first=None
            for da in (-1.5,0,1.5):
                a,e=math.radians(az+da),math.radians(el)
                d=Vector((math.cos(a)*math.cos(e),math.sin(a)*math.cos(e),math.sin(e)))
                hit=tree.ray_cast(Vector(eye),d,reach)[0]
                clears.append(hit is None)
                if hit is not None and first is None:first=hit
            clear=sum(clears)>=2;rays.append({'eye':label,'azimuth':az,'elevation':el,'clear':clear})
            if not clear:problems.append(f'{label} ray {az}/{el} blocked at {tuple(round(x,2) for x in first)}')
    for r1,r2,p,w in DOORS:
        shared=[]
        for a,b in zip(ROOMS[r1],ROOMS[r1][1:]+ROOMS[r1][:1]):
            if on_edge(p,a,b):shared.append((a,b))
        a,b=shared[0];d=(Vector(b)-Vector(a)).normalized();normal=Vector((-d.y,d.x))
        for offset in (-.36,0,.36):
            c=Vector(fp(p))+d*offset
            for z in (.2,.9,1.79):
                origin=Vector((c.x-normal.x*.5,c.y-normal.y*.5,z))
                if tree.ray_cast(origin,Vector((*normal,0)),1.0)[0] is not None:
                    problems.append(f'Door clearance {r1}/{r2} at {offset}/{z}')
    ramp_clearances=[]
    for i in range(13 if POD else 0):
        x=9.65+i*3.9/12;floor=-min(.95,max(0,(x-9.6)*.95/2.4))
        dist=tree.ray_cast(Vector((x*FOOT,0,floor+.03)),Vector((0,0,1)),5)[3]
        clearance=None if dist is None else dist+.03;ramp_clearances.append(clearance)
        if clearance is not None and clearance<1.8:problems.append(f'Pilot ramp headroom {clearance:.2f} at x={x:.2f}')
    # Glass is deliberately excluded from visibility, so headroom checks separately
    # include the canopy's measured height above its floor.
    assert not POD or canopy_z(14)-(-.95)>=1.8
    xs=[v.x for v in verts];ys=[v.y for v in verts];zs=[v.z for v in verts]
    report={'status':'PASS' if not problems else 'PROBLEM','problems':problems,'opaque_triangles':triangles,
      'opaque_geometry_sha256':hashlib.sha256(json.dumps(sorted(tuple(round(c,5) for c in v) for v in verts)).encode()).hexdigest(),
      'extent_m':{'length':round(max(xs)-min(xs),2),'width':round(max(ys)-min(ys),2),'height':round(max(zs)-min(zs),2),
                  'x':[round(min(xs),2),round(max(xs),2)],'z':[round(min(zs),2),round(max(zs),2)]},
      'footprint_factor':FOOT,
      'room_floor_area_m2':{k:round(area(v)*FOOT**2,1) for k,v in ROOMS.items()},'ceiling_m':CEIL,
      'door_count':len(DOORS),'door_widths_m':sorted(set(round(d[3]*FOOT,2) for d in DOORS)),
      'door_clear_height_m':2.20,'pilot_pod':POD,
      **({'pilot_canopy_clear_height_m':round(canopy_z(14)+.95,2),'pilot_ramp_angle_degrees':math.degrees(math.atan(.95/(2.4*FOOT))),
          'captain_eye_above_canopy_m':round(1.65-(canopy_z(12)+.05),2)} if POD else {}),'cargo_bay_m':[round((CARGO_X-RX0)*FOOT,2),round(2*RHALF*FOOT,2),round(-.23-FLOOR_A,2)],
      'reactor_hall_m':[round((RX1-CARGO_X)*FOOT,2),round(2*RHALF*FOOT,2),round(-.23-FLOOR_B,2)],'reactor_level_floor_area_m2':round((RX1-RX0)*2*RHALF*FOOT**2,1),
      'reactor_floor_levels_z':[FLOOR_A,FLOOR_T,FLOOR_B,PLINTH_TOP],
      'reactor_min_side_maintenance_m':round((RHALF-(PLINTH_C[1]+PLINTH_R))*FOOT,2),
      'landing':{'ground_z':GROUND,'deck_above_ground_m':-GROUND,'keel_clearance_m':round(min(z for _,z in KEEL_Z)-GROUND,2),
                 'skid_centres_y':[-SKID_Y*FOOT,SKID_Y*FOOT],'skid_x':[-10.8*FOOT,6.5*FOOT],'strut_x':[x*FOOT for x in STRUTS],
                 'ramp_length_m':round(ramp_run_rise()[2],2),
                 'ramp_slope_degrees':round(math.degrees(math.asin(ramp_run_rise()[1]/ramp_run_rise()[2])),1),
                 'ramp_width_m':2.4*FOOT,'doorway_cargo_to_bay_m':2*DOOR_BAY*FOOT,'doorway_clear_height_m':round(-.6-FLOOR_A,2)},
      'sightlines':rays,'ramp_headroom_samples_m':ramp_clearances,
      'limits':'Geometry probes, not a game walk test. Ladders and door interaction are not implemented here.'}
    (OUT/'geometry-check.json').write_text(json.dumps(report,indent=2)+'\n')
    print('SHIP_V3 CHECK:',report['status'],json.dumps(problems),'opaque triangles',triangles,json.dumps(report['extent_m']))
    return report


def section_scene(source):
    """A derived longitudinal cut; never exported as a second set of ship meshes."""
    section=bpy.data.scenes.new(PREFIX+' | Section')
    bpy.context.window.scene=source;bpy.context.view_layer.update()
    dg=bpy.context.evaluated_depsgraph_get()
    group=bpy.data.collections.new(PREFIX+' Section cut');section.collection.children.link(group)
    for key,c in collections.items():
        if key=='Glazing' or key in ('Labels','Labels lower','Sightline reference','Ground reference','Plan grid','Plan ghost deck','Plan ghost reactor','Gear stowed'):continue
        for obj in c.objects:
            if obj.type!='MESH':continue
            ev=obj.evaluated_get(dg);d=bpy.data.meshes.new_from_object(ev)
            # All script objects are unparented. matrix_basis also works for guides
            # excluded from the source view layer (whose matrix_world is stale).
            bm=bmesh.new();bm.from_mesh(d);bm.transform(obj.matrix_basis)
            bmesh.ops.bisect_plane(bm,geom=list(bm.verts)+list(bm.edges)+list(bm.faces),dist=.00001,
                                  plane_co=(0,0,0),plane_no=(0,1,0),clear_inner=True,clear_outer=False)
            bm.to_mesh(d);bm.free()
            if not len(d.polygons):bpy.data.meshes.remove(d);continue
            o=bpy.data.objects.new('Section '+obj.name,d);group.objects.link(o)
    lighting(section)
    # The cut opens the keel toward the camera: one soft light keeps its inside readable.
    d=bpy.data.lights.new('Keel interior light','AREA');d.energy=1500;d.shape='DISK';d.size=8
    o=bpy.data.objects.new('Keel interior light',d);section.collection.objects.link(o);o.location=fp((-3,-14,-1.8))
    o.rotation_euler=(Vector(fp((-3,0,-1.8)))-o.location).to_track_quat('-Z','Y').to_euler()
    section.camera=camera(section,'11 Longitudinal section',(1,-48,11),(1,0,-.5),ortho=32)
    return section


def plan_overlays():
    """Metre grid and ghost outlines for the plan scenes. The plan camera sits over the ship origin and
    sees PLAN_SCALE metres across the image, so 1600 px are 32 m (50 px per metre)."""
    gl=material('grid line',(.80,.85,.88)); gb=material('grid line bold',(.52,.63,.71))
    gm=material('ghost line',(.40,.46,.51))
    # The grid is real metres, so it is not stretched with the ship: it covers the stretched plan area.
    R,H=math.ceil(16*FOOT),math.ceil(12*FOOT)
    for x in range(-R,R+1):
        b=x%5==0; box(f'Grid x {x}',(x,0,4.0),(.05 if b else .02,2*H,.01),gb if b else gl,'Plan grid',0)
    for y in range(-H,H+1):
        b=y%5==0; box(f'Grid y {y}',(0,y,4.0),(2*R,.05 if b else .02,.01),gb if b else gl,'Plan grid',0)
    for x in range(-(R//5)*5,R+1,5): text(f'{x}',(x+.12,H-.5,4.02),.5,ink,'Plan grid')
    for y in range(-(H//5)*5,H-1,5): text(f'{y}',(-R+.6,y+.15,4.02),.5,ink,'Plan grid')
    text('x [m]   bow ->',(R-5,-(H-.5),4.02),.5,ink,'Plan grid'); text('y [m]',(-R+1.8,-(H-.5),4.02),.5,ink,'Plan grid')
    hz=HINGE[1]-GROUND; L=math.hypot(RAMP_TOP[0]-HINGE[0],RAMP_TOP[1]-HINGE[1])
    rx=HINGE[0]-math.sqrt(L*L-hz*hz)

    def outline(poly,mat,group,name,z=3.9):
        for a,b in zip(poly,poly[1:]+poly[:1]): wall_span(name,a,b,z-.01,z+.01,mat,.07,group)

    for group in ('Plan ghost deck','Plan ghost reactor'):
        for sgn in (-1,1):
            y0,y1=sorted((sgn*NY-1.62,sgn*NY+1.62))
            outline(rect(-13.5,-4.3,y0,y1),gm,group,'Nacelle outline')
            outline(rect(-10.8,6.5,sgn*SKID_Y-.28,sgn*SKID_Y+.28),gm,group,'Skid outline')
        outline(rect(rx,HINGE[0],-1.2,1.2),brass if group.endswith('deck') else gm,group,'Ramp outline')
    outline(REACTOR,brass,'Plan ghost deck','Reactor level outline'); outline(BAY,brass,'Plan ghost deck','Ramp bay outline')
    for x in (CARGO_X,HALL_X[1]): wall_span('Cargo bay and hall steps',(x,-RHALF),(x,RHALF),3.89,3.91,brass,.05,'Plan ghost deck')
    text('reactor level below',(-3.4,-1.4,3.95),.34,ink,'Plan ghost deck')
    for a,b in ROOM_EDGES: wall_span('Deck room outline',a,b,3.89,3.91,gm,.10,'Plan ghost reactor')
    for name,cx,cy in ROOM_LABELS: text(name,(cx,cy,3.95),.34,gb,'Plan ghost reactor')


def wb_render(scene,cam,path,visible,flat=True,res=None):
    """Workbench render: fast, flat colours, no noise. visible lists the collections that stay on."""
    r=scene.render; r.engine='BLENDER_WORKBENCH'; r.resolution_x,r.resolution_y=res or plan_res(); r.resolution_percentage=100
    r.image_settings.file_format='PNG'; r.filepath=str(path); r.film_transparent=False
    sh=scene.display.shading; sh.light='FLAT' if flat else 'STUDIO'; sh.color_type='MATERIAL'
    sh.show_shadows=False; sh.show_cavity=not flat; sh.show_object_outline=False
    scene.display.render_aa='8'; scene.view_settings.view_transform='Standard'
    if not scene.get('paper'):
        scene.world=bpy.data.worlds.new(PREFIX+' paper'); scene.world.color=(.98,.98,.96); scene['paper']=True
    scene.camera=cam; bpy.context.window.scene=scene
    inv={c.name:k for k,c in collections.items()}
    lc=scene.view_layers[0].layer_collection.children; saved={}
    for name,layer in lc.items():
        saved[name]=layer.exclude; layer.exclude=inv.get(name) not in visible
    bpy.ops.render.render(write_still=True)
    for name,layer in lc.items(): layer.exclude=saved[name]


def render_plans(S):
    out=OUT/'plans'; out.mkdir(exist_ok=True)
    paper=(.96,.96,.94,1); wall=(.17,.20,.22,1)
    for scene,tag,ground in ((S['deck'],'deck','Plan ghost deck'),(S['reactor'],'reactor','Plan ghost reactor')):
        cam=S[tag+'_top']
        base=['Structure','Doors'] if tag=='deck' else ['Lower']
        label='Labels' if tag=='deck' else 'Labels lower'
        for clean in (False,True):
            mats=dict(FLOORS); mats['hull']=hull; mats['lining']=lining
            saved={k:(m.diffuse_color[:],m.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value[:]) for k,m in mats.items()}
            for m in (hull,lining): m.diffuse_color=wall; m.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value=wall
            if clean:
                for m in FLOORS.values():
                    m.diffuse_color=paper; m.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value=paper
            vis=base+['Plan grid',ground]+([] if clean else [label])
            wb_render(scene,cam,out/f'plan_{tag}{"_clean" if clean else ""}.png',vis)
            for k,m in mats.items():
                m.diffuse_color=saved[k][0]; m.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value=saved[k][1]
    # Three-dimensional snapshots without a roof, one per level.
    wb_render(S['deck'],S['deck_3d'],out/'deck_3d.png',['Structure','Doors','Labels','Hull','Keel','Gear'],flat=False)
    wb_render(S['reactor'],S['reactor_3d'],out/'reactor_3d.png',['Lower','Labels lower','Keel','Gear','Guides'],flat=False)
    (out/'README.txt').write_text(plan_readme())


def plan_readme():
    w,h=plan_res()
    return f"""Raumplan EXO-1 Startschiff (Revision 4, Grundriss mal {FOOT:g})

Bilder (alle {w} x {h} px, Draufsicht, Bug nach rechts):
  plan_deck.png            Deck-Ebene (z = 0), Dach ab, mit Raumnamen
  plan_deck_clean.png      dasselbe ohne Raumnamen und ohne Bodenfarben, zum Hineinmalen
  plan_reactor.png         Reaktor-Ebene (Cargo-Bereich -2,9, Reaktorhalle -2,55, Rampenraum), Deck-Raeume als graue Umrisse
  plan_reactor_clean.png   dasselbe ohne Namen und Farben
  deck_3d.png, reactor_3d.png   dachlose 3D-Ansichten zur Orientierung

Massstab: 50 Pixel = 1 Meter. Der Schiffsursprung (x = 0, y = 0) liegt in der Bildmitte.
  x_m = (px - {w//2}) / 50      Bug (+x) ist rechts
  y_m = (({h//2}) - py) / 50      +y ist oben (Backbord)
Das Raster hat Linien alle 1 m, kraeftig alle 5 m, die Zahlen am Rand sind Meter.
Graue Umrisse zeigen Gondeln, Landekufen und Rampe; Ocker auf der Deck-Ebene: Reaktor-Ebene darunter.
"""


def build():
    global collections,hull,hull_lt,hull_dk,hull_edge,lining,teal,dark,frame,brass,glass,ink,white,glow
    # Only generated objects; foreign references and earlier revisions are untouched.
    owned={o for s in bpy.data.scenes if s.name.startswith(PREFIX) for o in s.objects}
    for obj in owned:bpy.data.objects.remove(obj,do_unlink=True)
    for s in list(bpy.data.scenes):
        if s.name.startswith(PREFIX): bpy.data.scenes.remove(s)
    for c in list(bpy.data.collections):
        if c.name.startswith(PREFIX):
            for o in list(c.objects): bpy.data.objects.remove(o,do_unlink=True)
            bpy.data.collections.remove(c)
    FLOORS.clear(); BRIDGE_FINS.clear(); ROOM_EDGES.clear(); ROOM_LABELS.clear()
    scene=bpy.data.scenes.new(PREFIX+' | Exterior')
    bpy.context.window.scene=scene;scene.unit_settings.system='METRIC'
    collections={}
    for name in ['Structure','Hull','Keel','Roof','Glazing','Lower','Doors','Hardware','Gear','Gear stowed',
                 'Labels','Labels lower','Guides','Sightline reference','Ground reference','Plan grid',
                 'Plan ghost deck','Plan ghost reactor']:
        c=bpy.data.collections.new(PREFIX+' '+name);scene.collection.children.link(c);collections[name]=c
    hull=material('warm ceramic hull',(.53,.56,.50),.25)
    hull_lt=material('light plate',(.59,.62,.56),.25)
    hull_dk=material('worn plate',(.49,.52,.47),.3)
    hull_edge=material('plate edge',(.34,.39,.37),.3)
    lining=material('cabin lining',(.52,.56,.53),.1)
    teal=material('petrol armour',(.105,.235,.23),.45)
    dark=material('dark tread',(.075,.10,.105),.3,.7)
    frame=material('structural steel',(.065,.095,.10),.6,.32)
    brass=material('ochre identification',(.64,.34,.105),.25)
    ink=material('floor lettering',(.07,.10,.1))
    white=material('light lettering',(.7,.76,.70))
    glass=material('clear laminated glazing',(.76,.91,.92),0,.065)
    p=glass.node_tree.nodes.get('Principled BSDF');p.inputs['Transmission Weight'].default_value=1;p.inputs['IOR'].default_value=1.12
    glow=material('engine idle',(.7,.3,.06))
    p=glow.node_tree.nodes.get('Principled BSDF');p.inputs['Emission Color'].default_value=(1,.4,.06,1);p.inputs['Emission Strength'].default_value=2
    architecture();roofs();exterior();lighting(scene);sightline_backdrop();plan_overlays()
    stretch_plan()       # everything above is design space; the factor is applied once, here
    scene['status']='Design proposal for #266. No game integration or approved dimensions.'
    scene['floor_z']=0.;scene['reactor_floor_z']=-2.9;scene['ground_z']=GROUND
    if POD: scene['pilot_floor_z']=-.95
    scene['review']=('Scenes: Exterior, Plan deck, Plan reactor, Landed (gear down), Flight (gear up), Section. '
                     'All share one set of objects. Toggle collections in the Outliner (Roof, Gear, Gear stowed).')
    cameras={
      'exterior':camera(scene,'01 Exterior',(27,-35,23),(0,0,.1),ortho=35),
      'front':camera(scene,'01b Front low',(30,-19,5.5),(5,0,1.2),lens=30),
      'aft':camera(scene,'02 Aft',(-28,32,18),(-1,0,0),ortho=34),
      'side':camera(scene,'03 Side',(1,-50,3),(1,0,0),ortho=34),
      'captain':camera(scene,'04 Captain eye 1.65m',(captain_x(),0,1.65),(20,0,1.65),lens=19),
      'captain_side':camera(scene,'05 Captain lateral',(captain_x(),0,1.65),(12,12,1.65),lens=23),
      'pilot':camera(scene,'06 Pilot seated eye 1.2m above floor',(pilot_x(),0,pilot_eye_z()),(22,0,pilot_eye_z()+.4),lens=18),
      'bridge_back':camera(scene,'07 Bridge looking aft',(11,3.6,1.65),(5,-1,1.3),lens=20),
    }
    scene.camera=cameras['exterior']
    # Hide review marks in exterior; use per-view-layer exclusion, not global flags.
    for k in REVIEW_ONLY:
        scene.view_layers[0].layer_collection.children[collections[k].name].exclude=True
    S={'exterior':scene,'cameras':cameras}
    # Plan scenes: no roof. 'Plan deck' shows deck level, 'Plan reactor' the level below.
    deck=bpy.data.scenes.new(PREFIX+' | Plan deck')
    for k in ('Structure','Doors','Labels','Hull','Keel','Gear','Plan grid','Plan ghost deck'): deck.collection.children.link(collections[k])
    S['deck']=deck; S['deck_top']=camera(deck,'Plan top (50 px per m)',(0,0,60),(0,0,0),ortho=PLAN_SCALE)
    S['deck_3d']=camera(deck,'Open roof 3D',(23,-28,29),(0,0,0),ortho=34); deck.camera=S['deck_top']
    reactor=bpy.data.scenes.new(PREFIX+' | Plan reactor')
    for k in ('Lower','Labels lower','Keel','Gear','Guides','Plan grid','Plan ghost reactor'): reactor.collection.children.link(collections[k])
    # Ladder linked as actual geometry into the lower level.
    for o in collections['Structure'].objects:
        if 'Ladder' in o.name or 'Coaming' in o.name: reactor.collection.objects.link(o)
    S['reactor']=reactor; S['reactor_top']=camera(reactor,'Plan top (50 px per m)',(0,0,60),(0,0,0),ortho=PLAN_SCALE)
    S['reactor_3d']=camera(reactor,'Lower level 3D',(-17,-15,9),(-4.5,0,-2.5),ortho=18); reactor.camera=S['reactor_top']
    for sc in (deck,reactor):
        bpy.context.window.scene=sc     # a new scene gets its layer collections only once it is active
        for k in ('Plan grid','Plan ghost deck','Plan ghost reactor'):
            lc=sc.view_layers[0].layer_collection.children
            if collections[k].name in lc: lc[collections[k].name].exclude=True
    # Landed and in flight.
    landed=bpy.data.scenes.new(PREFIX+' | Landed (gear down)')
    for k in ('Structure','Hull','Keel','Roof','Glazing','Lower','Doors','Hardware','Gear','Ground reference'): landed.collection.children.link(collections[k])
    lighting(landed)
    S['landed']=landed; S['landed_aft']=camera(landed,'Landed aft',(-26,-19,-1.0),(-8,0,-2.6),lens=30)
    S['landed_front']=camera(landed,'Landed front',(25,-19,0.5),(3,0,-2.2),lens=30)
    flight=bpy.data.scenes.new(PREFIX+' | Flight (gear up)')
    for k in ('Structure','Hull','Keel','Roof','Glazing','Lower','Doors','Hardware','Gear stowed'): flight.collection.children.link(collections[k])
    lighting(flight)
    S['flight']=flight; S['flight_front']=camera(flight,'Flight front',(25,-19,0.5),(3,0,-2.2),lens=30)
    bpy.context.window.scene=scene
    for a in bpy.context.screen.areas if bpy.context.screen else []:
        if a.type=='VIEW_3D':
            s=a.spaces.active;s.shading.type='SOLID';s.shading.color_type='MATERIAL';s.shading.show_cavity=True
            s.overlay.show_overlays=False;s.region_3d.view_location=Vector((0,0,0));s.region_3d.view_distance=32
            s.region_3d.view_rotation=Vector((27,-35,23)).to_track_quat('Z','Y');s.region_3d.view_perspective='ORTHO'
    S['section']=section_scene(scene)
    bpy.context.window.scene=scene
    return S


def main():
    global FOOT,POD
    parser=argparse.ArgumentParser();parser.add_argument('--render',action='store_true');parser.add_argument('--export',action='store_true')
    parser.add_argument('--views',help='Comma-separated review views; omitted means all. "plans" renders only the plan images.')
    parser.add_argument('--pilot-pod',action='store_true',help='Build the lowered pilot pod on the bridge nose (default: the pilot sits on the bridge).')
    parser.add_argument('--footprint',type=float,help=f'Plan stretch in x and y, heights unchanged (default {FOOT:g}; 1 is revision 4).')
    args=parser.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else [])
    if args.footprint: FOOT=args.footprint
    if args.pilot_pod: POD=True
    S=build();scene=S['exterior']
    check_geometry(scene)
    # Headless rebuild starts without reference assets. Saving linked scenes through
    # libraries.write crashes Blender 5.2; use the normal file-save path instead.
    if bpy.app.background:
        bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'starting-ship.blend'))
    print('SHIP_V3: built original architectural study, bow +X, main deck 0, '+('pilot pod -0.95' if POD else 'pilot on the bridge')+', reactor -2.9, ground',GROUND,'footprint',FOOT)
    if args.export:
        bpy.context.window.scene=scene
        bpy.ops.object.select_all(action='DESELECT')
        for k,c in collections.items():
            if k not in REVIEW_ONLY:
                for o in c.objects:
                    if o.type=='MESH':o.select_set(True)
        dest=ROOT/'content/ship';dest.mkdir(exist_ok=True)
        bpy.ops.export_scene.gltf(filepath=str(dest/'starting-ship-study.glb'),use_selection=True,use_active_scene=True,export_apply=True,export_yup=True)
    if args.render:
        views=set(args.views.split(',')) if args.views else None
        want=lambda n: not views or n in views
        cameras=S['cameras']
        for name in ['exterior','front','aft','side','captain','captain_side','pilot','bridge_back']:
            if not want(name):continue
            bpy.context.window.scene=scene;scene.camera=cameras[name]
            scene.view_layers[0].layer_collection.children[collections['Sightline reference'].name].exclude=name in ('exterior','front','aft','side')
            scene.render.filepath=str(OUT/(name+'.png'));bpy.ops.render.render(write_still=True)
        scene.view_layers[0].layer_collection.children[collections['Sightline reference'].name].exclude=True
        for name,sc,cam in [('landed_aft',S['landed'],S['landed_aft']),('landed_front',S['landed'],S['landed_front']),('flight_front',S['flight'],S['flight_front'])]:
            if not want(name):continue
            bpy.context.window.scene=sc;sc.camera=cam
            sc.render.filepath=str(OUT/(name+'.png'));bpy.ops.render.render(write_still=True)
        if want('section'):
            bpy.context.window.scene=S['section'];S['section'].render.filepath=str(OUT/'section.png');bpy.ops.render.render(write_still=True)
        if want('plans'): render_plans(S)
        bpy.context.window.scene=scene;scene.camera=cameras['exterior']
    return scene


if __name__=='__main__':main()
