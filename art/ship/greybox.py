"""First ship study from the initiator's sketch; dimensions are proposals.

Run live through MCP, or: blender -b -P art/ship/greybox.py
Writes only build/ship-greybox; does not export a game asset.
"""
from pathlib import Path
import math
import bpy
from mathutils import Vector, Quaternion

OUT = Path(__file__).resolve().parents[2] / 'build/ship-greybox'
OUT.mkdir(parents=True, exist_ok=True)
PREFIX = 'EXO ship draft'
if any(s.name.startswith(PREFIX) for s in bpy.data.scenes):
    raise RuntimeError('Draft scenes already exist. Inspect them before replacing; use a fresh Blender for rebuilds.')

def material(name, color, metallic=0):
    mat = bpy.data.materials.new('ship draft '+name)
    mat.diffuse_color = (*color,1)
    mat.use_nodes = True
    node = mat.node_tree.nodes.get('Principled BSDF')
    node.inputs['Base Color'].default_value = (*color,1)
    node.inputs['Metallic'].default_value = metallic
    node.inputs['Roughness'].default_value = .6
    return mat

hull = material('hull',(.28,.36,.33),.3)
wall = material('walls',(.52,.58,.54))
dark = material('devices',(.08,.12,.13),.4)
glass = material('glazing',(.08,.35,.45),.5)
yellow = material('ladder',(.85,.55,.12),.3)
colors = {'BRIDGE':(.25,.46,.58),'CARGO':(.47,.40,.24),'SCIENCE':(.24,.50,.43),'MED':(.48,.58,.5),'CREW':(.48,.40,.52),'WEAPONS':(.53,.31,.25),'ENGINE':(.32,.38,.43),'TURRET':(.42,.46,.4)}

def new_scene(name):
    s = bpy.data.scenes.new(PREFIX+' - '+name)
    bpy.context.window.scene = s
    s.unit_settings.system = 'METRIC'
    s['status'] = 'Proposal only: dimensions, access and station clearance require initiator review.'
    return s

def cube(name, loc, size, mat, bevel=.04):
    bpy.ops.mesh.primitive_cube_add(size=1, location=loc)
    obj=bpy.context.object
    obj.name=name
    obj.dimensions=size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    if bevel:
        mod=obj.modifiers.new('small edge bevel','BEVEL'); mod.width=bevel; mod.segments=2
    return obj

def text(body, loc, size=.45):
    data=bpy.data.curves.new(body,'FONT'); data.body=body; data.size=size
    obj=bpy.data.objects.new(body,data); bpy.context.scene.collection.objects.link(obj)
    obj.location=loc; obj.data.materials.append(dark)
    return obj

def cylinder(name, loc, radius, depth, mat, vertices=16):
    bpy.ops.mesh.primitive_cylinder_add(vertices=vertices, radius=radius, depth=depth, location=loc)
    obj=bpy.context.object; obj.name=name; obj.data.materials.append(mat)
    return obj

def segment(name, start, end, openings):
    a,b=Vector(start),Vector(end); delta=b-a; length=delta.length
    spans=[]; cursor=0
    for centre,width in sorted(openings):
        lo=max(0,centre-width/2); hi=min(length,centre+width/2)
        if lo>cursor: spans.append((cursor,lo))
        cursor=max(cursor,hi)
    if cursor<length: spans.append((cursor,length))
    for lo,hi in spans:
        mid=a+delta*((lo+hi)/2/length)
        obj=cube(name,(mid.x,mid.y,1.3),(hi-lo,.14,2.6),wall)
        obj.rotation_euler.z=math.atan2(delta.y,delta.x)

# Each doorway is measured along its room edge. Main floor z=0.
rooms=[
 ('ENGINE',(-12,-6,-4,4),{'E':[(4,1.4)],'N':[(5,1.4)],'S':[(5,1.4)]}),
 ('MED',(-9,-4,4,7),{'E':[(1.5,1.4)],'S':[(2,1.4),(4,1.4)]}),
 ('CREW',(-9,-4,-7,-4),{'E':[(1.5,1.4)],'N':[(2,1.4),(4,1.4)]}),
 ('TURRET',(-6,2,-4,4),{'W':[(4,1.4)],'E':[(2,1.4),(6,1.4)],'N':[(1,1.4),(5,1.4)],'S':[(1,1.4),(5,1.4)]}),
 ('SCIENCE',(-4,8,4,7),{'W':[(1.5,1.4)],'E':[(1,1.4)],'S':[(3,1.4)]}),
 ('WEAPONS',(-4,8,-7,-4),{'W':[(1.5,1.4)],'E':[(2,1.4)],'N':[(3,1.4)]}),
 ('CARGO A',(2,8,0,4),{'W':[(2,1.4)],'E':[(2,1.4)]}),
 ('CARGO B',(2,8,-4,0),{'W':[(2,1.4)],'E':[(2,1.4)]}),
 ('BRIDGE',(8,16,-6,6),{'W':[(1,1.4),(4,1.4),(8,1.4),(11,1.4)],'E':[(6,2)]}),
]
interior=new_scene('rooms')
for name,(x0,x1,y0,y1),doors in rooms:
    mat=material(name,colors[name.split()[0]])
    if name=='TURRET':
        # An actual opening for the ladder, 1.4 m square.
        for loc,size in [((-4.35,0,-.12),(3.3,8,.24)),((.35,0,-.12),(3.3,8,.24)),((-2,2.35,-.12),(1.4,3.3,.24)),((-2,-2.35,-.12),(1.4,3.3,.24))]:
            cube('Turret floor around ladder',loc,size,mat)
    else:
        cube(name+' floor',((x0+x1)/2,(y0+y1)/2,-.12),(x1-x0,y1-y0,.24),mat)
    for edge,a,b in [('S',(x0,y0),(x1,y0)),('N',(x0,y1),(x1,y1)),('W',(x0,y0),(x0,y1)),('E',(x1,y0),(x1,y1))]:
        if name=='BRIDGE' and edge in {'N','S','E'}:
            # Window band with low sill, maintains sight over lowered pilot.
            delta=Vector(b)-Vector(a); mid=(Vector(a)+Vector(b))/2
            spans=[(0,delta.length)] if edge!='E' else [(0,5),(7,12)]
            for lo,hi in spans:
                centre=Vector(a)+delta*((lo+hi)/2/delta.length)
                sill=cube('Bridge window sill',(centre.x,centre.y,.35),(hi-lo,.16,.7),wall); sill.rotation_euler.z=math.atan2(delta.y,delta.x)
                pane=cube('Bridge panoramic window',(centre.x,centre.y,1.55),(hi-lo,.08,1.7),glass); pane.rotation_euler.z=sill.rotation_euler.z
        else: segment(name+' bulkhead',a,b,doors.get(edge,[]))
    text(name,(x0+.35,y0+.35,.04))

# Small pilot alcove, half a metre lower: stepped greybox access is provisional.
cube('Pilot alcove floor',(17.1,0,-.62),(2.2,2.6,.24),hull)
cube('Pilot seat',(17.1,0,.1),(.65,.65,.7),dark)
cube('Pilot console',(17.85,0,.2),(.4,1.6,.8),dark)
for y in (-1.3,1.3): cube('Pilot side glazing',(17.1,y,.6),(2.2,.08,1.5),glass)
cube('Pilot front glazing',(18.2,0,.6),(.08,2.6,1.5),glass)
cube('Pilot access ramp',(15.9,0,-.25),(1.4,1.8,.15),wall).rotation_euler.y=math.atan(.5/1.4)
cube('Captain table',(11,0,.8),(1.4,1.2,1.6),dark)
for x,y in [(-3.7,3.65),(.2,-3.65)]:
    cube('Wall turret console',(x,y,.85),(1.5,.55,1.3),dark)
    cube('Wall turret display',(x,y,1.7),(1.3,.12,.6),glass)
for x,y in [(4,1),(6,3),(4,-3),(6,-1)]: cube('Cargo crate',(x,y,.5),(1,1,1),dark)
cube('Engine controls',(-11,0,1),(.6,2,2),dark)
cube('Science bench',(2,6.5,.6),(3,.8,1.2),dark)
cube('Med bed',(-7,6,.4),(2,.8,.8),wall)
cube('Crew bunk',(-7,-6,.4),(2,.8,.8),wall)
cube('Weapons bench',(2,-6.5,.6),(3,.8,1.2),dark)
cube('Protected reactor chamber floor',(-2,0,-3.12),(7,7,.24),hull)
cylinder('Reactor placeholder',(-3.7,1,-1.8),.8,2.4,dark)
for y in (-.48,.48): cube('Ladder rail',(-2,y,-1.1),(.08,.08,3.8),yellow)
for i in range(12): cube('Ladder rung',(-2,0,-2.8+i*.28),(.08,1,.07),yellow)
text('REACTOR BELOW / LADDER',(-4,-2,.06),.3)
text('PROVISIONAL DIMENSIONS / BOW +X',(-12,-9,0),.5)
cylinder('Walker clearance 0.35m radius 1.8m height',(12,3,.9),.35,1.8,yellow)

# Closed silhouette study: one continuous belly, tapering away from reactor.
exterior=new_scene('hull')
def ring_hull(name, rings, mat):
    verts=[]
    for x,w,bottom,top in rings:
        verts.extend([(x,-w*.8,bottom),(x,w*.8,bottom),(x,w,bottom+.6),(x,w,top-.5),(x,w*.8,top),(x,-w*.8,top),(x,-w,top-.5),(x,-w,bottom+.6)])
    faces=[tuple(reversed(range(8))),tuple(range((len(rings)-1)*8,len(rings)*8))]
    for r in range(len(rings)-1):
        for j in range(8): faces.append((r*8+j,r*8+(j+1)%8,(r+1)*8+(j+1)%8,(r+1)*8+j))
    mesh=bpy.data.meshes.new(name); mesh.from_pydata(verts,[],faces); mesh.update()
    obj=bpy.data.objects.new(name,mesh); bpy.context.scene.collection.objects.link(obj); obj.data.materials.append(mat)
    bevel=obj.modifiers.new('hull edges','BEVEL'); bevel.width=.12; bevel.segments=2
    return obj
ring_hull('Continuous protected hull',[(-12,4.5,-.6,2.8),(-8,7.3,-1.2,2.8),(-5,7.3,-3.4,2.8),(1,7.3,-3.4,2.8),(5,7.3,-1.6,2.8),(8,6.3,-.7,2.8),(14,6.3,-.7,2.8),(16,4,-.7,2.8)],hull)
ring_hull('Small lowered pilot nose',[(16,1.6,-.9,1.45),(18.3,1.4,-.9,1.45),(19,1,-.7,1)],hull)
for y in (-1.42,1.42): cube('Pilot glazing',(17.25,y,.75),(2.1,.08,1),glass)
cube('Pilot forward glass',(18.65,0,.55),(.12,1.8,.8),glass)
for y in (-6.32,6.32): cube('Bridge side window band',(12,y,1.55),(5.7,.1,1.5),glass)
for y in (-3,3): cube('Bridge forward window',(16.02,y,1.75),(.12,2.6,1.4),glass)
for y in (-8.5,8.5):
    ring_hull_obj=ring_hull('Aft engine nacelle',[(-14,1.3,-.3,2.4),(-7,1.3,-.3,2.4),(-6,1,-.2,2)],hull)
    ring_hull_obj.location.y=y
    cube('Engine exhaust',(-14.04,y,1),(.1,1.8,1.6),dark)
cylinder('Turret base',(-2,0,3),1.5,.4,dark)
cube('Turret body',(-2,0,3.5),(2.2,1.8,.8),hull)
for y in (-.4,.4): cube('Turret barrel',(-.1,y,3.5),(2.6,.15,.15),dark)

def setup_render(s, loc, target, ortho):
    bpy.context.window.scene=s
    s.render.engine='BLENDER_EEVEE'
    s.render.resolution_x=1600; s.render.resolution_y=1100; s.render.resolution_percentage=100
    s.world=bpy.data.worlds.new(s.name+' world'); s.world.use_nodes=True
    s.world.node_tree.nodes['Background'].inputs[0].default_value=(.07,.09,.1,1)
    s.world.node_tree.nodes['Background'].inputs[1].default_value=.6
    for position,energy,size in [((0,-12,25),3500,18),((10,15,18),2500,15)]:
        data=bpy.data.lights.new('Study softbox','AREA'); data.energy=energy; data.shape='DISK'; data.size=size
        obj=bpy.data.objects.new('Study softbox',data); s.collection.objects.link(obj); obj.location=position
        obj.rotation_euler=(Vector(target)-obj.location).to_track_quat('-Z','Y').to_euler()
    data=bpy.data.cameras.new('Study camera'); obj=bpy.data.objects.new('Study camera',data); s.collection.objects.link(obj)
    obj.location=loc; obj.rotation_euler=(Vector(target)-obj.location).to_track_quat('-Z','Y').to_euler()
    data.type='ORTHO'; data.ortho_scale=ortho; s.camera=obj

setup_render(interior,(21,-27,32),(2,0,0),43)
setup_render(exterior,(29,-36,24),(2,0,0),43)
bpy.context.window.scene=interior
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=='VIEW_3D':
            space=area.spaces.active; space.shading.type='SOLID'; space.shading.color_type='MATERIAL'; space.shading.show_cavity=True
            space.shading.show_backface_culling=False
            space.overlay.show_extras=False; space.overlay.show_floor=False
            region=space.region_3d; region.view_location=Vector((3,0,0)); region.view_distance=32
            region.view_rotation=Quaternion((1,0,0,0)); region.view_perspective='ORTHO'
bpy.ops.wm.save_as_mainfile(filepath=str(OUT/('ship-greybox-rebuilt.blend' if bpy.app.background else 'ship-greybox.blend')))
print('DRAFT: approximately 31 m long, main hull 14.6 m wide; 2.6 m room height, 1.4 m doors; dimensions not approved.')
print('DRAFT: ladder needs walker support; pilot ramp and door clearances not collision-tested; airlock not placed.')
if bpy.app.background:
    for s,name in [(interior,'rooms'),(exterior,'hull')]:
        bpy.context.window.scene=s; s.render.filepath=str(OUT/(name+'.png')); bpy.ops.render.render(write_still=True)
    exterior.camera.location=(24,-34,4)
    exterior.camera.rotation_euler=(Vector((2,0,0))-exterior.camera.location).to_track_quat('-Z','Y').to_euler()
    exterior.render.filepath=str(OUT/'hull-side.png'); bpy.ops.render.render(write_still=True)
