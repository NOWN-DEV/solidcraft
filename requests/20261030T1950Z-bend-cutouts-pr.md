<!-- claude-projects-attribution: {"github_login":"NOWN-DEV"} -->
_Requested by **Flip** · [project thread](https://claude.ai/code/project/chan_0177ZsyeNekJSJqYFCskBFvv?thread=cmsg_0177ZsyeNekJSJqYFCskBFvv3DZnLtjtXwHbQNFFkTyDyj)_

Fixes #9644

## What changes

**Convert to Sheet Metal keeps cut-outs that go through a modelled bend.** Each inner loop of a bend cylinder (a slit along the bend for folding it by hand, a relief slot at the end of a bend, a hole through its round) whose walls reach the bend's other skin becomes a cut-out of that bend's zone in the flat: along the bend where it lies along the cylinder's axis, across it as far as it lies round the bend, so the bend's material unrolls with it. A loop that stops inside the sheet is a pocket in a bend; it is refused with its position (`not supported yet: the pocket at … in a bend (cut it after converting)`) instead of being dropped. `crates/doc/src/sheet_convert.rs`: `bend_cutouts`, called in `convert_body` once the flanges are placed; `face_loops` and `loop_walls` are split out of `outline` unchanged so both use them.

**A cut-out wholly inside a bend zone folds right.** This was wrong on main before any conversion (unfold, cut inside the zone, refold): the zone's revolved face held the cut-out as an inner loop and the folded slot came out wider, with material left inside it. `SheetBody::cells_with_work` now splits a bend zone's material across the bend through the middle of each such cut-out (`zone_pieces_without_holes`), so every piece meets its cut-outs at its edge as a notch, which already folded right. Panels are untouched; a zone with no cut-out inside it is as before.

The module docs list the case and keep "cut-outs that run from a bend into a panel" and "pockets in a bend" under not supported yet.

Changes a default: no

## Tested

The repro from #9644 (L section 60 × 40 × 100, T 2, inner R 2, K 0.44; a slot 20 long revolved out of the bend between 200° and 220°):

| | main (0b34d359) | this branch | by hand |
|---|---|---|---|
| volume after `sheet.convert` | 19342.48 mm³ (slot lost) | 19300.59 mm³ | 19300.52 mm³ (the solid before converting) |
| flat pattern cut-outs | 0 | 1: 20 × 1.0053, 5/9…7/9 of the allowance from the base | 20 × 2/9 · 4.5239 |
| unfold, cut the same rectangle inside the zone, refold | 19149.91 mm³ | 19300.59 mm³ | 19300.52 mm³ |

Both columns measured with the CLI built from 0b34d359 and from this branch (89c4bdc9). Sampling the exported STL on the mid radius at z = 50 every 2° finds the slot over 201°…229° with material at 219° on main, and over exactly 200°…220° on this branch.

New tests:
- `cmd::sheet::convert_tests::a_slot_through_a_modelled_bend_is_a_cut_out_of_its_bend_zone`: the converted sheet keeps the solid's volume, the flat cut-out's size and place in the zone, reopening, unfold/refold; a slot that stops inside the sheet is refused as a pocket in a bend.
- `sheet::tests::cut_outs_inside_a_bend_zone_fold`: a slot and a round hole inside the zone of a 72°, 90° and 108° flange; flat area and folded volume (K 0.5, so the folded sheet holds its flat area times the thickness), and a valid body.

Runs: `cargo test -p solidcraft-doc --lib sheet` (94 passed), `cargo test -p solidcraft-engine --lib cmd::sheet` (all passed), `cargo clippy -p solidcraft-doc -p solidcraft-engine --all-targets -- -D warnings`, `cargo fmt --check`. I also converted eight bent production parts of ours (trims with hand-fold slits and bend-end relief slots, channels) with it: all convert, and their hand-fold slits and end relief slots now land on the bend lines, where the reference flats exported from another CAD system have them.

Screenshots: this was made by a command-line agent without a browser session, so it cannot attach images; the numbers above are from the CLI and the exported STL.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_018evSqk2WUxMsvg3eGB9rKi
