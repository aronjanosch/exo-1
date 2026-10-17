"""Camera menu for the interior studies: sidebar tab "EXO" in the 3D viewport (press N).

One button per study camera. A click switches the window to that study's scene, makes the camera the
active one and shows its view in rendered shading. Run it with Text > Run Script, or let it load with
the .blend (the text is registered as a module). Reading and viewing only, it changes no geometry.
"""
import re

import bpy


class EXO_OT_look(bpy.types.Operator):
    bl_idname = 'exo.look'
    bl_label = 'Look through this camera'
    scene: bpy.props.StringProperty()
    camera: bpy.props.StringProperty()

    def execute(self, context):
        sc = bpy.data.scenes.get(self.scene)
        ob = sc.objects.get(self.camera) if sc else None
        if not ob:
            self.report({'WARNING'}, 'Scene or camera not found')
            return {'CANCELLED'}
        context.window.scene = sc
        sc.camera = ob
        for area in context.window.screen.areas:
            if area.type == 'VIEW_3D':
                sp = area.spaces.active
                sp.region_3d.view_perspective = 'CAMERA'
                sp.shading.type = 'RENDERED'
                sp.overlay.show_overlays = False
        return {'FINISHED'}


class EXO_PT_cameras(bpy.types.Panel):
    bl_label = 'EXO studies: cameras'
    bl_space_type = 'VIEW_3D'
    bl_region_type = 'UI'
    bl_category = 'EXO'

    def draw(self, context):
        col = self.layout.column()
        col.label(text='Now: %s' % context.scene.name)
        for sc in bpy.data.scenes:
            if not sc.name.endswith('| Architecture'):
                continue
            box = col.box()
            box.label(text=sc.name.split(' | ')[0])
            for ob in sorted((o for o in sc.objects if o.type == 'CAMERA'), key=lambda o: o.name):
                active = sc.camera is ob and context.scene is sc
                row = box.row()
                label = re.sub(r'\.\d+$', '', ob.name.split(' ', 2)[2] if ob.name.count(' ') >= 2 else ob.name)
                op = row.operator('exo.look', text=label, depress=active)
                op.scene = sc.name
                op.camera = ob.name
        col.separator()
        col.label(text='Mouse wheel zooms, Shift+middle mouse pans.')
        col.label(text='Numpad 0 = camera view, Numpad . = frame selection.')


classes = (EXO_OT_look, EXO_PT_cameras)


def register():
    for c in classes:
        try:
            bpy.utils.unregister_class(c)
        except Exception:
            pass
        bpy.utils.register_class(c)


if __name__ == '__main__' or True:
    register()
