# Junction box

Built by `junction_box.py` (run line in its docstring) into `content/ship/junction_box.glb`. Design source: decision "Junction boxes: repair objects, routing central" (2026-10-18) in the concept repo, epic #322. The box is a repair object with no screen: two group toggles with lamps, a fault lamp, and a door over the bus bars. One model serves all three boxes (JB-1 engines + jump drive, JB-2 life support + lights, JB-3 shields + weapons); the labels say which.

## Frame and size

- Units are metres. The origin is on the wall surface, at floor height, under the box's centre line.
- In the game (glTF, Y up) the front faces **+Z**. Blender (x, y, z) is glTF (x, z, −y).
- The cabinet is 0.56 wide, 0.84 high and 0.17 deep, from 0.92 to 1.76 m. Three conduits run up 0.5 m from the top, one feed runs down to the floor.
- Triangles: 970 (guide 2,500).

Travel is given relative to the node's rest pose, about the node's own local axis, in glTF terms. Positive angles follow the right-hand rule.

## Moving parts

| Node | Pivot (glTF position) | Axis (local) | Travel |
|---|---|---|---|
| `door` | hinge on the door's left edge, (−0.24, 1.32, 0.17) | Y | Rest pose is closed. Open is −105° (the door swings out to the player's left). |
| `toggle_1`, `toggle_2` | bat foot on the lower front, (∓0.13, 1.06, 0.182) | X | Rest pose is OFF, bat tipped 25° down. ON is −50° from rest, bat up. `toggle_1` is on the player's left. |

No load gauge: the load per group is on the central power screen (initiator, 2026-10-18).

## Lights (emissive by the game)

Each has its own material named like the node. Unlit, they are dark (`lamp_off`).

| Node / material | Where | Meaning |
|---|---|---|
| `lamp_group_1`, `lamp_group_2` | above each toggle | The group gets power. |
| `lamp_fault` | in the middle between door and toggles | The box is damaged or broken; may blink. |

## Texture quads

Flat quads with UVs 0..1 upright, facing out of the front (glTF local +Z), plain for now.

| Node | Size (w × h, m) | Spot |
|---|---|---|
| `label_box` | 0.30 × 0.05 | Top of the front, above the door: the box's name (JB-1 …). |
| `label_group_1`, `label_group_2` | 0.12 × 0.035 | Under each toggle: the group's name. |

## Other nodes

- `junction_box_body`: everything that does not move, in one mesh, including the bus bars and breaker modules behind the door.
- `spark_point`: an empty 3 cm in front of the door's centre, where the game spawns sparks and fire.

## Review

`build/junction-box-review/` (gitignored) holds `junction_box_three_quarter`, `junction_box_front_eye` and `junction_box_side`, with a stand-in wall and a 1.80 m figure that are not exported.
