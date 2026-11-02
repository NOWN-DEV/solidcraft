## What happens

Cutting a few hundred round holes of 20–30 mm into an unfolded sheet-metal part is refused with:

> sheet metal flat geometry exceeds the bounded processing budget

Small holes of the same number go through. The size of each hole moves the limit: the larger the hole, the fewer fit.

## Repro

1. Make a sheet-metal rule: 1.5 mm thick, k 0.4, bend radius 1 mm.
2. Make a 1000 x 800 base flange, then 30 mm walls on its four edges.
3. Run `sheet.unfold`.
4. Sketch 250 circles of r 14 on the base at z 3 (a staggered grid at a 36 mm pitch, 20 mm clear of the edges). Cut them
   with a 10 mm extrude in the negative direction.

The cut is refused. The test in the linked branch builds exactly this, with a second cut of 250 more holes and a refold.

## Why

Two places count each arc by the chords of its 0.001 mm polyline, against a cap of 65 536:

- `regions_complexity` in `crates/doc/src/sheet.rs` (`MAX_FLAT_SAMPLES`);
- `planar_regions_boolean` in `crates/kernel/src/prism.rs` (`samples`, "planar boolean sampled geometry exceeds its
  budget").

A full circle at 0.001 mm is about 158 chords at Ø10, 224 at Ø20, 264 at Ø28 and 274 at Ø30. That makes 65 536 roughly 400
holes of Ø10 but only about 250 of Ø28. Yet the planar booleans never sample lines or arcs: they intersect them whole.
The only polyline on that path is a fallback for other segment kinds, which never reach it, because free-form loops are
fitted to arcs before the arrangement.

If only the `crates/doc` count is removed, the same cut is refused one level down, by the kernel's copy of the count
("planar boolean sampled geometry exceeds its budget").

## Proposed change

Remove both chord counts: `MAX_FLAT_SAMPLES` and its loop in `crates/doc/src/sheet.rs`, and the `samples` counter in
`planar_regions_boolean`. The kernel part is 5 removed lines (one `let`, one check).

Why this should be safe: every bound that limits the actual work stays.

- Arcs still count as edges against the flat's edge cap (`MAX_FLAT_POINTS`, 16 384) and its piece cap (4096).
- The flat work budget (`MAX_FLAT_WORK`) still bounds the arrangement.
- In the kernel, `MAX_PLANAR_SEGMENTS` (8192), `MAX_PLANAR_EVENTS` and `MAX_PLANAR_PAIRS` still bound the boolean.
- A circle is two arcs whatever its radius, so a large hole now costs what a small one does. That cost is what the
  booleans actually spend on it.

The existing budget test now drives the edge cap with arcs instead of the chord count. A new engine test cuts 500 Ø28
holes into an unfolded tray in two cuts and refolds it. On `main` the first cut is refused; with the change both go
through, and the removed volume is within 0.1 % of 500 holes.

## Question

This touches `crates/kernel/` during the pause (#3992). It is a 5-line removal that a refused sheet-metal operation needs,
and it adds no new kernel behaviour. Would a PR for it be accepted now? If not, should we park it until the new kernel is
decided, or send the case to the new kernel's regression suite? The `crates/doc` half alone does not fix the refusal.
