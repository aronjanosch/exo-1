"""Smuggle kit: the city kit, reused, plus the materials the smuggling models need.

The city kit lives in `art/city/kit.py` and has the same name as this file, so it is loaded
by path under the module name `city_kit`. Everything it exports is re-exported here; the
lab's extra materials are added to its shared `MATERIALS` before any `Part` is created
(`Part.__init__` copies that dict), and `material(key)` names the Blender material `key`,
so the glTF material name is exactly the key.
"""

import importlib.util
import os
import sys

_CITY_KIT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "city", "kit.py")
_spec = importlib.util.spec_from_file_location("city_kit", _CITY_KIT)
_city = importlib.util.module_from_spec(_spec)
sys.modules["city_kit"] = _city
_spec.loader.exec_module(_city)

from city_kit import *  # noqa: E402,F401,F403
from city_kit import MATERIALS, material, colour, Part, run  # noqa: E402,F401

# Lab materials. The game recolours `brew` (the brew surface) and tints `band`, so both sit
# near white/neutral in vertex colour. Emission 0.6 matches `glow` (higher burns white).
MATERIALS.update({
    "brew": {"rgb": (1, 1, 1), "roughness": 0.5, "emit": 0.6},
    "band": {"rgb": (1, 1, 1), "roughness": 0.5, "emit": 0.6},
    "light_red": {"rgb": (1, 1, 1), "roughness": 0.5, "emit": 0.6},
    "light_blue": {"rgb": (1, 1, 1), "roughness": 0.5, "emit": 0.6},
})
