"""Run with blender -b --python-exit-code 1 -P art/tests/test_walker_normals.py."""
from pathlib import Path
import runpy
import sys
import unittest

import bmesh
import bpy

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mesh_checks import face_islands, require_mesh_orientation


class WalkerNormalsTests(unittest.TestCase):
    def test_norb_skin_body_keeps_hands_connected_and_faces_outward(self):
        bpy.ops.wm.read_factory_settings(use_empty=True)
        ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / "walker/walker.py"))
        figure = ns["Figure"]("NorbCheck", skin=(0.9, 0.7, 0.6), suit=(0.2, 0.4, 0.6))
        ns["norb_body"](figure)
        require_mesh_orientation(figure.body.data)
        bm = bmesh.new()
        try:
            bm.from_mesh(figure.body.data)
            self.assertEqual(sum(1 for _ in face_islands(bm)), 1)
        finally:
            bm.free()
        self.assertEqual(len(figure.parts), 3)
        for part in figure.parts:
            require_mesh_orientation(part.data)

    def test_spiked_hair_has_no_collapsed_closed_fragments(self):
        bpy.ops.wm.read_factory_settings(use_empty=True)
        ns = runpy.run_path(str(Path(__file__).resolve().parents[1] / "walker/walker.py"))
        figure = ns["norb_spikes"]().build()
        require_mesh_orientation(figure.data)


if __name__ == "__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(WalkerNormalsTests)
    if not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful():
        raise RuntimeError("walker orientation tests failed")
