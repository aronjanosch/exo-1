# Reactor and antimatter canister

Built by `reactor.py` (run line in its docstring) into `content/ship/reactor.glb` and `content/ship/canister_antimatter.glb`. Design source: engine room, WALKTHROUGH section 4 and the player journal `transcripts/2026-10-17-reaktor.md` in the concept repo, then the initiator's redesign of 2026-10-17. The engineer's main work (injection, field converter, AUTO/MANUAL, heat, output, demand) runs on one screen. Only the emergency and start-up parts are physical.

The look is a "magnetic bottle":
- in the middle, a glass column with a glowing spindle; its waist at 2.0 m is the annihilation point;
- two confinement rings above and below the waist;
- four pylons carrying a crown, with conduits to the ceiling;
- a base drum with vents and a blue trim;
- the console with the screen at the front (+Z in the game);
- the start-up stand on the left flank (−X);
- the canister feed bay on the right flank (+X);
- two coolant lines at the back into the floor.

## Frame and size

- Units are metres. The origin is the floor under the column's axis.
- In the game (glTF, Y up), the console faces **+Z**. Blender (x, y, z) is glTF (x, z, −y).
- Height to the ceiling flanges is 3.60 m; the column alone ends at 2.96 m. The base is Ø 3.0 m.
- With the console, stand and bay, the footprint is about 3.4 m (x) × 3.8 m (z). All measures are constants at the top of the script, so the reactor can be fitted to the ship.
- Triangles: reactor 5,112 (guide 12,000), canister 264 (guide 600).

Travel is given **relative to the node's rest rotation, about the node's own local axis**, in glTF terms. Positive angles follow the right-hand rule. Positions are the nodes' glTF translations at rest.

## Moving parts

| Node | Pivot (glTF position) | Axis (local) | Travel |
|---|---|---|---|
| `lever_start` | lever hub on the stand's slope, (−1.49, 1.05, 0) | X | Rest pose is OFF. Each detent is +35° (PRIME +35°, RUN +70°); positive tips the grip toward the player. The arm points along local +Z. |
| `toggle_boot_1..3` | bat foot on the stand's slope, (−1.66, 1.01, −0.15 / 0 / +0.15) | X | Rest pose is OFF, tipped 20° toward the player. ON is −40° from rest. Numbered from the player's left. |
| `cover_scram` | hinge along the cover's up-slope edge, (0.5, 1.01, 1.62) | X | Rest pose is closed. Open is −110°. |
| `button_scram` | button face centre, (0.5, 0.98, 1.71) | — | Pressed is 0.015 m along local −Z (into the desk). |
| `cover_eject` | hinge along the niche's top edge, in the front-left pylon, (−0.86, 1.48, 0.86) | X | Rest pose is closed. Open is −110°. |
| `lever_eject` | lever foot in the niche, (−0.81, 1.09, 0.81) | X | Rest pose is upright in the niche. Pulled is +60° (grip out toward the player). |
| `ring_1`, `ring_2` | on the column axis, y 1.62 and 2.38 | Y | Free spin; the four emitter blocks per ring make the turn visible. |
| `core` | empty at the column's foot, (0, 1.22, 0) | — | Children `core_glass` and `core_glow`. On core eject the game moves `core` down (−Y) through the base and out of the ship. |
| `slot_1`, `slot_2` | socket plate at the bottom of each hole, (1.36, 1.12, +0.25 / −0.25) | local +Y points out of the bay | A seated canister takes the slot's world transform as it is. `slot_1` is nearer the console. |

`canister_antimatter`:
- origin at the end that enters the slot first, long axis along local +Y;
- Ø 0.15 m (body), 0.42 m long, plus a 0.08 m bail on the outer end;
- 0.26 m of it sits inside the bay, 0.16 m plus the bail stands out.

## Lights (emissive by the game)

Each has its own material named like the node. Unlit, they are dark (`lamp_off`).

| Node / material | Where | Meaning |
|---|---|---|
| `lamp_slot_1`, `lamp_slot_2` | ring round each slot's mouth | Turns green when a canister is seated. Fill state goes on the screen. |
| `lamp_prime_1..5` | segment column on the stand's backboard, 1 at the bottom | During PRIME they light one after another (~10 s, trap charging). |

Always-glowing materials the game may drive (brightness, flicker, colour):
- `core_glow`: the spindle in the column;
- `ring_glow`: emitters on both rings;
- `trim_glow`: the strip round the base;
- `canister_glow`: the canister's window band, which could show its fill.

`core_glass` is alpha-blended (alpha 0.18).

## Texture quads

All are flat quads with UVs 0..1 upright. They face out of their surface (glTF local +Z). Each has its own material named like the node, plain for now; the designs follow.

| Node | Size (w × h, m) | Spot |
|---|---|---|
| `screen_main` | 1.10 × 0.62 | Console screen in a 4 cm recess, tilted back 10°, centre at 1.40 m. It shows the reactor status and switches to the implosion countdown. Panel 1280 × 720 px (tier L). |
| `screen_supply` | 1.10 × 0.12 | Slim supply screen (canisters, battery, coolant; read-only) on the console's top edge above the visor, tilted back 30°, centre at 1.86 m. Panel 1280 × 140 px, the same density as `screen_main`. |
| `label_reactor` | 0.50 × 0.07 | Nameplate on the console front, at 0.55 m. |
| `label_scram` | 0.20 × 0.05 | Under the SCRAM cover, on the main desk. |
| `label_startup` | 0.30 × 0.07 | Top of the stand's backboard, above the prime column. |
| `label_start` | 0.10 × 0.30 | Beside the START gate: OFF (up-slope), PRIME, RUN (toward the player). |
| `label_boot` | 0.46 × 0.05 | In front of the three boot toggles. |
| `label_eject` | 0.26 × 0.07 | Above the eject niche on the pylon. |
| `label_slot_1`, `label_slot_2` | 0.22 × 0.06 | Above each slot. |

## Static nodes

- `reactor_body`: everything that does not move, in one mesh.
- `scram_base`: the yellow guard plate.
- `lever_start_gate`: the gate with the three detent marks.

## Review

`build/reactor-review/` (gitignored) holds `reactor_three_quarter`, `reactor_front_eye`, `reactor_side`, `reactor_top` and `reactor_startup_close`, with a 1.80 m stand-in figure and two preview canisters that are not exported.
