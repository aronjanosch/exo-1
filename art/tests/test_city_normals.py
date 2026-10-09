"""Run with blender -b --python-exit-code 1 -P art/tests/test_city_normals.py."""
from pathlib import Path
import runpy
import sys
import unittest

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "city"))
import kit


class CityNormalsTests(unittest.TestCase):
    def setUp(self):
        bpy.ops.wm.read_factory_settings(use_empty=True)

    def test_tv_rounding_cannot_exceed_half_the_depth(self):
        part = kit.Part("invalid screen")
        try:
            with self.assertRaisesRegex(ValueError, "rounded_box radius"):
                part.rounded_box((-0.33, -0.33, 0.52), (0.33, -0.29, 0.98), 0.1, (1, 1, 1))
        finally:
            part.bm.free()

    def test_all_bungalows_build_with_valid_orientation(self):
        original_run = kit.run
        kit.run = lambda models, out: None
        try:
            homes = runpy.run_path(str(Path(kit.__file__).with_name("homes.py")))
        finally:
            kit.run = original_run
        for name, (make, footprint) in homes["MODELS"].items():
            with self.subTest(model=name):
                part = make()
                obj = part.build(kit.fresh_collection(name))
                self.assertEqual(kit.check(obj, part, footprint)["problems"], [])


if __name__ == "__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(CityNormalsTests)
    if not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful():
        raise RuntimeError("city orientation tests failed")
