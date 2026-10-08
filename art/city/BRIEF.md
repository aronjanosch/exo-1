# City brief

The first city: hand-built, not procedural, placed onto the planet as a whole. Direction from the initiator (2026-10-08): "Eine futuristische höher gebaute Version von schedule 1 vll." All numbers below are starting values (guide values, not rules); change them here when a review shows better ones.

## Feel

- Schedule I's small dense town, grown upwards: shops at street level, 2–7 floors of flats above, junk on the roofs (domes, dishes, tanks, antennas with glowing tips).
- Goofy retro-future in a strange galaxy: portholes, chunky floor bands, fat awnings, glowing sign faces and strips. Shapes say "future", colours stay friendly.
- Read from the street first: a walker at eye height (1.7 m) on the opposite sidewalk sees the shop floor, the sign and the first two floors. Upper floors read as silhouettes and lit windows.

## Measures

| What | Value | Note |
|---|---|---|
| Road tile | 10 × 10 m | 6 m carriageway, 2 m sidewalks, kerb 0.15 m; Schedule I uses the same split |
| Facade bay | 4 m | buildings are 1–3 bays wide |
| Facade grid | 0.5 m | windows, doors and signs snap to it |
| Shop floor | 4.5 m | upper floors 3.5 m |
| Door | 1.5 × 2.5 m | wider than a human door, aliens come in all sizes |
| Lot | building plus its awning | the model stays inside the lot it declares; the check enforces it |

## Look

- One mesh per building, vertex colours for the paint, three materials: `paint`, `glass`, `glow` (emissive).
- Smooth shading with hard edges (edge split at 40°), chunky boxes, no texture yet. Lettering and grime come later as decals or a small atlas.
- Palette per building: one body colour, one accent, one glow colour; trims share `TRIM` and `METAL` across the city so streets hang together.

## What the game reads

- Empties parented to the building, with custom properties (glTF extras): `kind = "door"` plus `use` (`shop`, `flat`). The anchor sits 1 m in front of the door, on the ground, where a walker stands.
- Where a building sits in the city and where the city sits on the planet: a separate placement file, not in the model (`exo-1-concept/docs/research/place-authoring.md`).

## Open (initiator)

- Names and fiction of shops; which shops are enterable (interiors in the same model, like Schedule I).
- Textures: stay with flat vertex colours, or a small shared atlas as Schedule I uses.
- Vehicles: does the carriageway carry ground traffic, hover traffic, or is it a promenade?
