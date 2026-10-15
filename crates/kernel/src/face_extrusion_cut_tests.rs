//! A cut by a planar face's own extrusion on a body that is no prism (#1275): a 40 × 40 × 20
//! block with corners rounded to 8 mm, chamfered or rounded 2 mm around its bottom, its top face
//! extruded 5 mm into it and cut. The cut's walls lie on the block's walls, its quarter-cylinders
//! on the block's corners, which the B-rep boolean can't intersect: it failed after every retry
//! ("no result", 7 to 20 seconds). The face moves down instead, exactly.

use std::f64::consts::PI;

use solidcraft_geom::{Loop2, Plane, Region2, Seg2, Vec2, Vec3};

use crate::body::from_p3;
use crate::*;

/// The rectangle (x0, y0)–(x1, y1) with its corners rounded to `r`, counter-clockwise.
fn rounded(x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> Region2 {
    let line = |a: Vec2, b: Vec2| Seg2::Line { a, b };
    let arc = |center: Vec2, start: f64| Seg2::Arc { center, radius: r, start, sweep: PI / 2. };
    let segs = vec![
        line(Vec2::new(x0 + r, y0), Vec2::new(x1 - r, y0)),
        arc(Vec2::new(x1 - r, y0 + r), -PI / 2.),
        line(Vec2::new(x1, y0 + r), Vec2::new(x1, y1 - r)),
        arc(Vec2::new(x1 - r, y1 - r), 0.),
        line(Vec2::new(x1 - r, y1), Vec2::new(x0 + r, y1)),
        arc(Vec2::new(x0 + r, y1 - r), PI / 2.),
        line(Vec2::new(x0, y1 - r), Vec2::new(x0, y0 + r)),
        arc(Vec2::new(x0 + r, y0 + r), PI),
    ];
    Region2 { outer: Loop2 { segs }, holes: vec![] }
}

/// `section` extruded from `z0` to `z1`.
fn prism(section: &Region2, z0: f64, z1: f64) -> Body {
    extrude(&Plane::XY, std::slice::from_ref(section), z0, z1).unwrap().pop().unwrap()
}

/// The rounded square 40 × 40, radius 8, extruded from `z0` to `z1`.
fn rounded_block(z0: f64, z1: f64) -> Body {
    prism(&rounded(0., 0., 40., 40., 8.), z0, z1)
}

/// A point on each edge of the block's bottom loop: its four sides and four corner arcs.
fn bottom_loop() -> Vec<Vec3> {
    let c = 8. - 8. / 2f64.sqrt();
    [(20., 0.), (40., 20.), (20., 40.), (0., 20.), (c, c), (40. - c, c), (40. - c, 40. - c), (c, 40. - c)]
        .iter()
        .map(|(x, y)| Vec3::new(*x, *y, 0.))
        .collect()
}

/// The rounded square's area: 40² less the four corners the radius-8 arcs leave out.
fn outline_area() -> f64 {
    1600. - (4. - PI) * 64.
}

/// What the 2 mm chamfer around the bottom takes: a triangle of area 2 along the four 24 mm sides,
/// and turned about each corner's axis (its centroid (8 + 8 + 6) / 3 from it) a quarter turn.
fn chamfer_takes() -> f64 {
    2. * 96. + 2. * 2. * PI * 22. / 3.
}

/// What the 2 mm round around the bottom takes: the region between the corner and the arc,
/// (4 − π), along the sides and turned about each corner's axis, its centroid
/// 2 (10 − 3π) / (3 (4 − π)) in from the wall.
fn round_takes() -> f64 {
    let section = 4. - PI;
    section * 96. + section * 2. * PI * (8. - 2. * (10. - 3. * PI) / (3. * (4. - PI)))
}

/// Cut by the extrusion of its top face 5 mm down, the block with its bottom chamfered or rounded
/// loses that slab and nothing else: its top lies at z = 15 with the same outline, its walls
/// still run down to the blend, and no retry was needed. Every vertex is at a level of the block
/// (0, 2) or the new top (15): nothing was nudged. The same with the tool run a little past the
/// top, as the engine runs a cut's end that lies on a face.
#[test]
fn a_whole_face_cut_after_a_bottom_blend_lowers_the_face() {
    for (what, blended, takes, top) in [
        ("chamfer", chamfer(&rounded_block(0., 20.), &bottom_loop(), 2.).unwrap(), chamfer_takes(), 20.),
        ("chamfer, the tool past the top", chamfer(&rounded_block(0., 20.), &bottom_loop(), 2.).unwrap(), chamfer_takes(), 20.4),
        ("round, the tool past the top", fillet(&rounded_block(0., 20.), &bottom_loop(), 2.).unwrap(), round_takes(), 20.4),
    ] {
        // Measured on meshes, whose chords along the curved walls take a few hundredths of a
        // mm³ off: within 2e-6 of the volume (the slab cut takes 7725 mm³).
        let v0 = blended.tessellate(1e-4).unwrap().measure().volume;
        let want0 = outline_area() * 20. - takes;
        assert!((v0 - want0).abs() < want0 * 2e-6, "{what}: the block {v0} vs {want0}");
        let tool = rounded_block(15., top);
        let (cut, d) = capture_boolean_diagnostics(|| boolean(&blended, &tool, BoolOp::Cut));
        let cut = cut.unwrap().unwrap();
        assert!(d.events.is_empty(), "{what}: {d:?}");
        assert_eq!(cut.shell_count(), 1, "{what}");
        assert!(cut.validity().is_empty(), "{what}: {:?}", cut.validity());
        assert_eq!(cut.face_count(), blended.face_count(), "{what}");
        let v = cut.tessellate(1e-4).unwrap().measure().volume;
        assert!((v - (v0 - outline_area() * 5.)).abs() < 2e-2, "{what}: {v} vs {}", v0 - outline_area() * 5.);
        let want = outline_area() * 15. - takes;
        assert!((v - want).abs() < want * 2e-6, "{what}: {v} vs {want}");
        for p in cut.solid.vertex_iter().map(|v| from_p3(v.point())) {
            assert!([0., 2., 15.].iter().any(|z| (p.z - z).abs() < 1e-9), "{what}: a vertex at {p:?}");
        }
        // The top, still the block's top face, moved (its area measured on chords of its arcs).
        let top = cut.faces(1e-4).unwrap().into_iter().find(|f| f.plane_normal.is_some_and(|n| n.z > 0.5)).unwrap();
        assert!((top.centroid.z - 15.).abs() < 1e-9 && (top.area - outline_area()).abs() < 1e-2, "{what}: {} at {:?}", top.area, top.centroid);
        let was = blended.faces(1e-3).unwrap().into_iter().find(|f| f.plane_normal.is_some_and(|n| n.z > 0.5)).unwrap().index;
        let history = cut.history().expect("the cut's history");
        let origin = history.faces.get(top.index).unwrap();
        assert_eq!(origin.origins, vec![history::FaceOrigin::Face { operand: 0, face: was }], "{what}");
        assert_eq!(origin.change, history::Change::Modified, "{what}");
    }
}

/// The reporter's part in small: a pocket (a rounded square 20 × 20, radius 4) 5 mm into the
/// block's top, the bottom chamfered, then the pocket's floor extruded 5 mm down and cut, the
/// tool ending at the floor or reaching up into the empty pocket, or out past the block's top.
/// The pocket's walls stretch down to the new floor at z = 10.
#[test]
fn a_pocket_floor_cut_deeper_after_a_bottom_chamfer_lowers_the_floor() {
    let pocket = rounded(10., 10., 30., 30., 4.);
    let floor = pocket.area();
    let tool = prism(&pocket, 15., 20.);
    let pocketed = boolean(&rounded_block(0., 20.), &tool, BoolOp::Cut).unwrap().unwrap();
    let part = chamfer(&pocketed, &bottom_loop(), 2.).unwrap();
    for top in [15., 15.4, 25.] {
        let deeper = prism(&pocket, 10., top);
        let (cut, d) = capture_boolean_diagnostics(|| boolean(&part, &deeper, BoolOp::Cut));
        let cut = cut.unwrap().unwrap();
        assert!(d.events.is_empty(), "to {top}: {d:?}");
        assert!(cut.validity().is_empty(), "to {top}: {:?}", cut.validity());
        let want = outline_area() * 20. - chamfer_takes() - floor * 10.;
        let v = cut.tessellate(1e-4).unwrap().measure().volume;
        assert!((v - want).abs() < want * 2e-6, "to {top}: {v} vs {want}");
        for p in cut.solid.vertex_iter().map(|v| from_p3(v.point())) {
            assert!([0., 2., 10., 20.].iter().any(|z| (p.z - z).abs() < 1e-9), "to {top}: a vertex at {p:?}");
        }
    }
}

/// Where the slab under the face is not all material, or the tool is not the face's extrusion,
/// the face is not moved: a blind hole up from the bottom to 2 mm under the top, a tool smaller
/// than the face, and one deeper than the walls under the face reach (into the chamfer).
#[test]
fn a_face_is_not_moved_through_what_lies_under_it() {
    let block = chamfer(&rounded_block(0., 20.), &bottom_loop(), 2.).unwrap();
    let hole = cylinder(Vec3::new(20., 20., -1.), Vec3::Z, 3., 19.).unwrap();
    let drilled = boolean(&block, &hole, BoolOp::Cut).unwrap().unwrap();
    let tool = rounded_block(15., 20.);
    assert!(crate::prism::cut_by_face_extrusion(&drilled, &tool).is_none());
    let smaller = prism(&rounded(1., 1., 39., 39., 7.), 15., 20.);
    assert!(crate::prism::cut_by_face_extrusion(&block, &smaller).is_none());
    assert!(crate::prism::cut_by_face_extrusion(&block, &rounded_block(1., 20.)).is_none());
    // The tool as the face's extrusion: moved.
    assert!(crate::prism::cut_by_face_extrusion(&block, &tool).is_some());
}

/// Past the face the tool must hold nothing of the body either: a pocket's floor at z = 10 under
/// a bar across the block's top (z = 20 to 22). A tool from z = 5 that ends under the bar moves
/// the floor; one that reaches into the bar is no extrusion of the floor alone.
#[test]
fn a_face_is_not_moved_under_what_the_tool_reaches_past_it() {
    let pocket = rounded(10., 10., 30., 30., 4.);
    let tool = prism(&pocket, 10., 20.);
    let pocketed = boolean(&rounded_block(0., 20.), &tool, BoolOp::Cut).unwrap().unwrap();
    let bar = box_solid(Vec3::new(0., 18., 20.), Vec3::new(40., 22., 22.)).unwrap();
    let bridged = boolean(&pocketed, &bar, BoolOp::Union).unwrap().unwrap();
    assert_eq!(bridged.shell_count(), 1);
    let under = prism(&pocket, 5., 19.);
    let cut = crate::prism::cut_by_face_extrusion(&bridged, &under).expect("the floor moved");
    let volume = |b: &Body| b.tessellate(1e-4).unwrap().measure().volume;
    let want = volume(&bridged) - pocket.area() * 5.;
    assert!((volume(&cut) - want).abs() < want * 2e-6, "{} vs {want}", volume(&cut));
    let into = prism(&pocket, 5., 21.);
    assert!(crate::prism::cut_by_face_extrusion(&bridged, &into).is_none());
}
