"""Run with blender -b --python-exit-code 1 -P art/tests/test_mesh_checks.py."""

from pathlib import Path
import sys
import unittest

import bmesh
import bpy
from mathutils import Matrix

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mesh_checks import orientation_problems, require_orientation, orient_mesh, require_mesh_orientation


class OrientationTests(unittest.TestCase):
    def setUp(self):
        self.bm = bmesh.new()

    def tearDown(self):
        self.bm.free()

    def cube(self, size=1, offset=(0, 0, 0)):
        result = bmesh.ops.create_cube(self.bm, size=size, matrix=Matrix.Translation(offset))
        return {f for v in result["verts"] for f in v.link_faces}

    def test_disconnected_inverted_part_is_not_hidden_by_large_body(self):
        self.cube(size=10)
        faces = self.cube(size=0.04, offset=(20, 0, 0))
        bmesh.ops.reverse_faces(self.bm, faces=list(faces))
        with self.assertRaisesRegex(ValueError, "inverted closed island"):
            require_orientation(self.bm, "small part")

    def test_one_reversed_face_is_detected(self):
        faces = self.cube()
        bmesh.ops.reverse_faces(self.bm, faces=[next(iter(faces))])
        self.assertTrue(any("inconsistent face winding" in p for p in orientation_problems(self.bm)))

    def test_outward_cubes_at_different_scales_and_offsets(self):
        self.cube(size=1)
        self.cube(size=0.001, offset=(100, 100, 100))
        self.assertEqual(orientation_problems(self.bm), [])

    def test_open_shell_is_allowed_but_mixed_winding_is_not(self):
        faces = self.cube()
        bmesh.ops.delete(self.bm, geom=[next(iter(faces))], context="FACES")
        self.assertEqual(orientation_problems(self.bm), [])
        bmesh.ops.reverse_faces(self.bm, faces=[next(iter(self.bm.faces))])
        self.assertTrue(orientation_problems(self.bm))

    def test_closed_zero_volume_is_rejected(self):
        self.cube()
        for v in self.bm.verts:
            v.co.z = 0
        self.assertTrue(any("zero-volume" in p for p in orientation_problems(self.bm)))

    def test_repair_preserves_geometry_and_orients_each_island(self):
        self.cube(size=10)
        faces = self.cube(size=0.04, offset=(20, 0, 0))
        bmesh.ops.reverse_faces(self.bm, faces=list(faces))
        mesh = bpy.data.meshes.new("orientation repair")
        try:
            self.bm.to_mesh(mesh)
            before = [tuple(v.co) for v in mesh.vertices]
            orient_mesh(mesh)
            require_mesh_orientation(mesh)
            self.assertEqual(before, [tuple(v.co) for v in mesh.vertices])
        finally:
            bpy.data.meshes.remove(mesh)

    def test_generated_body_cannot_silently_detach_a_limb(self):
        self.cube()
        self.cube(size=0.04, offset=(2, 0, 0))
        mesh = bpy.data.meshes.new("detached hand")
        try:
            self.bm.to_mesh(mesh)
            with self.assertRaisesRegex(ValueError, "one connected surface"):
                orient_mesh(mesh, connected=True)
        finally:
            bpy.data.meshes.remove(mesh)

    def test_back_to_back_faces_do_not_count_as_a_solid(self):
        mesh = bpy.data.meshes.new("flat face pair")
        try:
            mesh.from_pydata([(0, 0, 0), (1, 0, 0), (0, 1, 0)], [], [(0, 1, 2), (2, 1, 0)])
            with self.assertRaisesRegex(ValueError, "zero-volume"):
                require_mesh_orientation(mesh)
        finally:
            bpy.data.meshes.remove(mesh)


if __name__ == "__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(OrientationTests)
    if not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful():
        raise RuntimeError("face orientation tests failed")
