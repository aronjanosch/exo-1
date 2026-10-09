# Walker models

`walker.py` is the source for all meshes, materials and painted textures. Units
are metres; figures face Blender −Y, with an origin centred at their feet.
Generated `.blend` files and review images belong in the ignored `build/`
directory, not in version control.

For model design and visual refinement, follow the [live MCP workflow](../README.md).
The commands below rebuild the script for review.

## Generate the review scene

From the repository root, using the installed Blender:

```sh
mkdir -p build/walker-review
blender -b --python-exit-code 1 -P art/walker/walker.py -- \
  --renders build/walker-review \
  --blend "$PWD/build/walker-review/walkers.blend"
```

The default lineup contains NorbPainted, NorbMullet, NorbSidePart, NorbSpikes,
Glibbo, Zorp and Wobbel. `--only NorbPainted,Glibbo` selects a smaller lineup;
`--samples 24` reduces review render time. `--only Norb` builds the earlier
geometric face study for comparison. The script builds meshes without rendering
when `--renders` is omitted.

Review images include front, three-quarter and back views, a face view for every
figure, a view at 60 metres in daylight, and a view at 30 metres with night
lighting. The script prints triangle counts, height and half width for each model.

Every built figure is checked for consistent face winding and positive signed
volume on each closed mesh island. Intentional open shells such as eyelids are
allowed. The Skin body must also be one connected surface: acute wrist/thumb
branches can fold and disconnect its hands, so thumbs use separate rounded
primitives at the skeleton's positions. Body normals are repaired before
decimation and paint; the finished figure is checked again without repair.
Use `--python-exit-code 1`
so a failed check also fails a headless command.

Run the orientation regression tests in Blender:

```sh
blender -b --python-exit-code 1 -P art/tests/test_mesh_checks.py
blender -b --python-exit-code 1 -P art/tests/test_walker_normals.py
```

## Shared visual language

- Smooth broad shapes with hard edges at genuine corners.
- White eyes, small dark pupils and heavy lids; intentional facial asymmetry.
- Painted mouths and restrained cheek colour; eyes, brows and hair remain 3D.
- Everyday clothes with broad painted seams and pockets. Shirt/garment textures
  use greys so the colour can be multiplied over them.
- Alien bodies and clothing merge overlapping primitives into continuous
  surfaces. Lids, garment borders and accessories stay separate.
- Four human hairstyles share the same base body and facial construction.
  The final human profile follows the live MCP study: a shorter lower face and
  a clearer jaw. Mop and side part get gentle hair smoothing; the mop crown
  stays close to the scalp.

Body and hair simplification are separate budgets. The human body is simplified
before material boundaries are cut and before UV unwrapping, so hems and cuffs
keep clean colour transitions. Budgets are guide values, with additional geometry
for eyes and accessories where it contributes to the silhouette.

This script creates a Blender review scene. It does not export or install a game
asset; the glTF integration contract is in the concept repo's walker brief and
issue #32.
