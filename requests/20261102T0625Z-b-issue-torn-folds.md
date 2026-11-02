## What happens

Many CAD programs draw a box's or tray's flat with each corner relief as a thin V. The V's sides run straight across
both bend zones, from the bend's start line to its far edge, so each wall's end sits inside the neighbouring wall's
bend zone. Folded, each bend's end is torn along the V, which is what the rule's own `tear` corner relief makes.

`sheet.fold` refuses the first fold of such a flat:

> the bend zone (3.534 mm wide) reaches a corner of the sheet's outline at (201.767, -1.237, 1.500): move the fold line
> at least 3.004 mm toward the side that folds or 0.530 mm toward the side that stays

Moving the line doesn't help: the V crosses every zone of that width. The zone isn't a rectangle, and the walls' ends
make it one piece with the neighbouring walls.

## Repro

A 200 x 100 tray with 20 mm walls, 1.5 mm sheet, bend radius 1.5, k 0.5 (bend allowance 3.534 mm, half of it
`h = 1.767146`). The reliefs are V's from the corner of the base to the walls' ends at the zones' far edges. Fold
lines start 1.4 mm in from each corner, inside the V, as a CAD program places them.

```json
{"commands": [
  {"command": "sheet.manage_rules", "params": {"name": "Sheet 1.5 mm", "thickness": "1.5", "k_factor": "0.5", "bend_radius": "1.5", "active": true}},
  {"command": "sketch.create", "params": {"plane": "XY", "name": "Blank"}},
  {"command": "sketch.line", "params": {"points": [[0.6, -21.767146], [199.4, -21.767146], [199.4, -1.767146], [198.232854, 1.767146], [201.767146, -1.237002], [221.767146, -1.237002], [221.767146, 101.237002], [201.767146, 101.237002], [198.232854, 98.232854], [199.4, 101.767146], [199.4, 121.767146], [0.6, 121.767146], [0.6, 101.767146], [1.767146, 98.232854], [-1.767146, 101.237002], [-21.767146, 101.237002], [-21.767146, -1.237002], [-1.767146, -1.237002], [1.767146, 1.767146], [0.6, -1.767146]], "closed": true}},
  {"command": "sketch.finish", "params": {}},
  {"command": "sheet.flange", "params": {"sketch": "Blank"}},
  {"command": "sheet.fold", "params": {"points": [[1.4, 0, 1.5], [198.6, 0, 1.5]], "fixed": [100.0, 50.0, 1.5]}},
  {"command": "sheet.fold", "params": {"points": [[1.4, 100.0, 1.5], [198.6, 100.0, 1.5]], "fixed": [100.0, 50.0, 1.5]}},
  {"command": "sheet.fold", "params": {"points": [[0, 1.4, 1.5], [0, 98.6, 1.5]], "fixed": [100.0, 50.0, 1.5]}},
  {"command": "sheet.fold", "params": {"points": [[200.0, 1.4, 1.5], [200.0, 98.6, 1.5]], "fixed": [100.0, 50.0, 1.5]}}
]}
```

Replayed on `main` `a93d7b20`, step 6 (the first fold) fails with the message above.

## Expected

Each fold tears its zone's ends along the V and the walls stand: the same body as a tray made with flanges and `tear`
corner reliefs. Unfold gives back the flat as drawn.

Three variants of real flats need the same thing:

- **The V's sides drawn as arcs a few microns off their line.** Imported tears often come as chains of arcs. A tolerance
  of about 0.01 mm on "straight" is enough.
- **A slightly tighter radius than the reliefs were drawn for.** Then the V runs on across the zone, and the zone falls
  apart into the wall's piece and the neighbours' ends (a `fold_tabs` case). Only the piece the line runs through
  should fold.
- **Flanges next to a torn fold.** A lip already folded on a side wall, whose bend runs on past the corner into the
  next zone, should stay with its wall. A flange on another wall's end that runs on beyond the zone should stay where
  it is and not go round with the fold.

A relief that stops well short of the zone's far edge should still be refused, and the message could say that a
corner relief must run across the whole zone. A torn end only ever runs the moving side on, so a zone whose far edge
stops short of its start line (the blank narrowing towards the side that moves) should stay refused rather than lose
the slivers outside a square end. Nor should a notch or slit that stops a hair inside the zone be taken as part of a
torn zone and filled; the fold should name its corner. And very narrow zones (a small angle or radius) must keep
folding as they do today.

## Related

- #6957 / #7025: the same rectangle check, for blanks with slanted or curved outlines (an ellipse, a trapezoid). #7025
  holds the base in its bounding rectangle. That doesn't cover this case, because here the zone holds real corners of
  the outline (the neighbouring walls' ends), and the tray's other walls have to stay put.
- #6842: square relief notches on a tray.
- #723: the rule's `tear` corner relief, whose bend ends these folds reproduce.

I have a fix with tests (doc and engine only, no kernel change) and can open a PR.
