Fixes #10991.

Changes a default: no

## Change

Many CAD programs draw a tray's flat with each corner relief as a thin V. The V's sides run straight across both bend
zones, from the start line to the far edge, so a wall's end sits inside the neighbouring wall's zone. `sheet.fold`
refused every such fold: the zone wasn't a rectangle, and the walls' ends made it one piece with the neighbours ("the
bend zone reaches a corner of the sheet's outline").

In `fold_side` and `fold_tabs` (`crates/doc/src/sheet.rs`):

- **Slanted zone ends.** `torn_zone` accepts a zone that ends on a straight line from its start line to its far edge,
  whose ends may stray from that line by 0.01 mm (imported tears come as chains of arcs a few microns off their line), or
  by a quarter of a narrower zone. The start line and far edge must hold exactly, so a notch that stops a hair inside
  the zone is not filled. The new flange gets `tear` and
  `ext` for those ends, as the rule's `tear` corner relief does. The zone's slivers beside a straightened end are
  dropped.
- **A zone that is one piece with the neighbours' ends.** Where the V only meets the zone's edge, or stops a hair short
  of it, `torn_tab` folds the zone piece the line runs through and keeps the other walls' ends.
- **A zone that falls apart into tabs** (a slightly tighter radius than the reliefs were drawn for). A tab counts where
  it meets the line itself (`run_at`), and material beyond the zone hangs from the tab it shares the far edge with.
- **Flanges next to a torn fold.** A flange on a wall's end beside the zone stays with that wall: a torn bend's corner
  may reach 0.01 mm into the next zone (`torn_reach`). A flange beyond the zone that sits on material that stays (another
  wall's end running on past it) no longer goes round with the fold (`edge_on`).
- **Spikes.** The pieces a fold keeps and moves drop repeated points and zero-width spikes (`without_repeats`). A V
  side ending on a corner of the outline on the far line left such a spike, and the sheet failed to sew ("sew:
  unsupported edge contact").

Still refused:

- a relief that stops well inside the zone (the corner message now adds "a corner relief must run across the whole
  zone");
- a zone whose far edge stops short of its start line (a blank narrowing towards the side that moves), by any amount
  over 1e-6 mm. A torn end only runs the moving side on, so taking that zone would drop the slivers outside the end's
  square;
- a notch or slit that stops inside the zone: the fold names its corner, as before.

A zone narrower than the tolerance needs (a small angle or radius) reads the line at its middle and is never taken for a
torn tab, so small folds behave as before.

No kernel change.

## Tests

In `crates/engine/src/cmd/sheet_tests.rs`, on a 200 x 100 tray flat with 20 mm walls, 1.5 mm sheet, R 1.5, k 0.5:

- `a_flat_with_slanted_corner_reliefs_folds_into_a_tray`: the four walls in either order. The walls stand, the volume
  stays within 1e-4 folded and 1e-9 unfolded, the flat's box is unchanged, and refold works. At R 2.5 the fold is refused
  and names the relief.
- `a_flat_with_corner_reliefs_as_drawn_elsewhere_folds_into_a_tray`: lines starting inside the V, R 1.45 (the zones fall
  apart into tabs), and a V bowed a few microns.
- `a_lip_beside_a_torn_zone_stays_with_its_wall`: a lip folded on a side wall before the walls.
- `a_fold_beside_a_relief_ending_on_an_outline_corner_sews`: both orders.
- `a_flange_on_a_walls_end_beyond_a_torn_zone_stays`: an ear on a wall's end that runs on past the bottom zone, at
  R 1.5 and 1.45.
- `a_torn_fold_never_drops_material_where_the_blank_narrows`: a trapezoid blank. The widening side folds and unfolds
  exactly. The narrowing side is refused, or else unfolds exactly.
- `a_torn_fold_never_drops_material_where_the_blank_narrows_slightly`: slopes of 0.002 and 0.0028, within the 0.01 mm
  tolerance. The fold is refused, or else unfolds exactly.
- `a_slit_ending_a_hair_inside_the_zone_names_its_corner`: a 0.5 mm slit stopping 0.005 mm inside the zone, from either
  side. The fold names the corner.
- `a_bend_zone_narrower_than_a_torn_ends_tolerance_never_panics`: two collinear tabs at 0.5°, R 0.1 fold and keep their
  material; at 2° and 1°, R 0, they don't panic; the tray's wall at 2°, R 0 is refused or unfolds exactly.
- `the_slanted_corner_relief_script_folds_its_walls`: replays `examples/regressions/sheet-fold-slanted-corner-reliefs.json`.

On `main` the first seven fail. Six fail at their first fold across a slanted relief ("the bend zone (3.534 mm wide) reaches a
corner of the sheet's outline at (201.767, -1.237, 1.500) ..."). The trapezoid's widening side fails with "not
supported yet: folds whose bend zone crosses a slanted or curved edge of the sheet". The last three are guards that pass
on `main`. Without those guards: the slightly narrowing blank unfolds 0.037 mm³ short, the slit is filled and refused
only by sewing, and the tabs at 0.5° panic in `f64::clamp`. With the change, all pass.

## Checks

- `cargo fmt --all --check`
- `cargo clippy -p solidcraft-doc -p solidcraft-engine --all-targets -- -D warnings`. With clippy 0.1.97 the only finding
  is in `crates/doc/src/project.rs:368` on `main` (`manual_range_contains`), which this PR doesn't touch.
- `cargo test -p solidcraft-doc`
- `cargo test -p solidcraft-engine --lib -- sheet`

## Related

#7025 (#6957) changes the same rectangle check for ellipse and trapezoid blanks. The two fit together: here a zone
narrowing towards the moving side stays refused, so #7025's path still handles #6957's trapezoid. They conflict
textually, and whichever merges second rebases.
