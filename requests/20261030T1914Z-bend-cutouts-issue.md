## What happens

`sheet.convert` on a solid whose bends are modelled with their radius (an inner and an outer cylinder) silently drops any cut-out that lies in a bend cylinder: a slit along the bend for folding it by hand, a relief slot at the end of a bend, a hole through the bend's round. The conversion succeeds, but the converted sheet comes back without the cut-out, and the flat pattern and the DXF have no cut-out there. A part made from that DXF has no slit.

## Repro (CLI, `solidcraft-cli run slot.json --results`)

An L section, outside 60 × 40, 2 thick, 100 long, its corner bent R 2 inside about the axis (4, 4). A slot is revolved out of the bend's round about that axis: z 40…60, between 200° and 220° round the axis (the bend runs from the leg at 180° to the base at 270°), so its sides are square to the sheet. Rule: T 2, R 2, K 0.44. Converted with the base fixed and the outer cylinder picked as the bend.

| | expected | main (97a24941) |
|---|---|---|
| volume after convert | 19300.5 mm³ (the same solid, as without the slot) | 19342.5 mm³ (41.9 more: the slot, 20° × (R + T/2) × T × 20) |
| flat pattern cut-outs | 1, in the bend zone | 0 |

<details><summary>slot.json</summary>

```json
{"commands": [
  {"command": "sheet.manage_rules", "params": {"name": "Test", "thickness": "2 mm", "k_factor": "0.44", "bend_radius": "2 mm", "gap": "0.5 mm", "active": true}},
  {"command": "sketch.create", "params": {"plane": "XY", "name": "Section"}},
  {"command": "sketch.line", "params": {"points": [[0, 40], [0, 4]]}},
  {"command": "sketch.arc.three_point", "params": {"start": [0, 4], "end": [4, 0], "through": [1.1716, 1.1716]}},
  {"command": "sketch.line", "params": {"points": [[4, 0], [60, 0], [60, 2], [4, 2]]}},
  {"command": "sketch.arc.three_point", "params": {"start": [4, 2], "end": [2, 4], "through": [2.5858, 2.5858]}},
  {"command": "sketch.line", "params": {"points": [[2, 4], [2, 40], [0, 40]]}},
  {"command": "sketch.finish", "params": {}},
  {"command": "solid.extrude", "params": {"sketch": "Section", "distance": 100, "body_name": "Section"}},
  {"command": "sketch.create", "params": {"plane": {"origin": [4, 4, 0], "x_dir": [-0.866, -0.5, 0], "y_dir": [0, 0, 1]}, "name": "Slot"}},
  {"command": "sketch.rectangle.two_point", "params": {"p0": [1, 40], "p1": [5, 60]}},
  {"command": "sketch.finish", "params": {}},
  {"command": "solid.revolve", "params": {"sketch": "Slot", "axis": {"origin": [4, 4, 0], "dir": [0, 0, 1]}, "angle": "20 deg", "direction": "symmetric", "operation": "cut"}},
  {"command": "inspect.measure", "params": {}},
  {"command": "sheet.convert", "params": {"body": "Section", "face": [30, 0, 50], "bends": [[1.1716, 1.1716, 50]]}},
  {"command": "inspect.measure", "params": {}},
  {"command": "sheet.flat_pattern", "params": {}}
]}
```

</details>

Bend zone in the flat: 100 × 4.524 (the allowance π/2 × (R + K·T)). The slot should be a cut-out 20 along the bend line and 2/9 of the allowance across it (1.005), 5/9…7/9 of the way from the base's side.

## A second symptom, also on main without any conversion

The same slot made the other way, on a sheet: convert the L without the slot, `sheet.unfold`, cut the 20 × 1.005 rectangle through the flat inside the bend zone (x 0.481…1.486, z 40…60 in the unfolded L, so the zone is whole round it), `sheet.refold`. The flat pattern is right (one cut-out), but the folded sheet is wrong: `inspect.measure` gives 19149.9 mm³ instead of 19300.5, and sampling the exported STL on the mid radius at z = 50 every 2° finds the slot over 201°…229° with material left at 219°, instead of 200°…220°. The same cut moved to reach the zone's edge (x 0.481…4.5) folds right (19175.85 mm³, 19175.9 by hand), so it is a cut-out wholly inside a bend zone that folds wrong: the zone's revolved face is built with the cut-out as a loop inside it.

## Why (first symptom)

`sheet_convert.rs` builds panels from the planar faces only; the bend cylinders between them are replaced by the rule's bend, and nothing reads the cylinder's own inner loops.

## Proposed fix

1. Carry each inner loop of a bend cylinder into the flat as a cut-out of its bend zone: along the bend as it lies along the axis, across it as far as it is round the bend (the bend's material unrolls with it). A loop whose walls do not reach the bend's other skin is a pocket in the bend: refuse it with a clear message instead of dropping it.
2. When the folded sheet is built, split a bend zone's material across the bend through the middle of each cut-out wholly inside it, so every piece meets its cut-outs at its edge, as a notch (the case that already folds right).

PR to follow.

Changes a default: no
