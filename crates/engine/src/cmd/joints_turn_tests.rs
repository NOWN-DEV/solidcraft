//! A new joint given no angle keeps the part's turn about the joint's axis: two faces that
//! already meet stay as they are instead of the part spinning a half turn about their normal.

use serde_json::json;
use solidcraft_geom::Vec3;

use super::tests::{occ, place, run};
use crate::Session;

/// A grounded panel 80 x 30 x 3 at z 3 and a hinge plate of the same size under it, each with a
/// hole off its middle at x = 20: the two faces that meet at z = 3 have opposite normals.
fn panel_and_hinge() -> Session {
    let mut s = Session::default();
    run(&mut s, "component.create", json!({"name": "Panel"}));
    run(&mut s, "solid.box", json!({"length": 80, "width": 30, "height": 3, "corner": [0, 0, 3]}));
    run(&mut s, "solid.hole", json!({"position": [20, 15, 6], "diameter": 5.5}));
    run(&mut s, "component.activate", json!({"component": "root"}));
    run(&mut s, "component.create", json!({"name": "Hinge"}));
    run(&mut s, "solid.box", json!({"length": 80, "width": 30, "height": 3, "corner": [0, 0, 0]}));
    run(&mut s, "solid.hole", json!({"position": [20, 15, 3], "diameter": 5.5}));
    run(&mut s, "component.activate", json!({"component": "root"}));
    let panel = occ(&s, "Panel");
    run(&mut s, "occurrence.ground", json!({"occurrence": panel, "grounded": true}));
    s
}

fn stays(s: &Session) -> bool {
    [Vec3::new(20.0, 15.0, 0.0), Vec3::new(80.0, 0.0, 3.0)].iter().all(|p| place(s, "Hinge", *p).dist(*p) < 1e-6)
}

#[test]
fn joining_two_faces_that_already_meet_leaves_the_part_where_it_is() {
    // Each face's frame takes an x of its own; with opposite normals those x are opposite, and
    // the joint used to spin the hinge a half turn about the normal, its hole to x = 60.
    for kind in ["rigid", "slider", "planar"] {
        let mut s = panel_and_hinge();
        let r = run(
            &mut s,
            "joint.create",
            json!({"type": kind, "a": {"occurrence": "Panel", "face": [30, 25, 3]}, "b": {"occurrence": "Hinge", "face": [30, 25, 3]}, "name": "J"}),
        );
        assert!(stays(&s), "{kind}: the hinge moved to {:?}: {r}", place(&s, "Hinge", Vec3::new(20.0, 15.0, 0.0)));
        // Its value still moves it along the joint's axis (the faces' normal), without a turn.
        if kind == "slider" {
            run(&mut s, "joint.drive", json!({"joint": "J", "value": 5}));
            let p = place(&s, "Hinge", Vec3::new(20.0, 15.0, 0.0));
            assert!(p.dist(Vec3::new(20.0, 15.0, -5.0)) < 1e-6, "{p:?}");
        }
    }
    // An angle given is kept as given.
    let mut s = panel_and_hinge();
    run(
        &mut s,
        "joint.create",
        json!({"type": "rigid", "a": {"occurrence": "Panel", "face": [30, 25, 3]}, "b": {"occurrence": "Hinge", "face": [30, 25, 3]}, "angle": 0}),
    );
    assert!(!stays(&s));
    let j = s.doc.assembly.joints.last().map(|j| j.angle);
    assert_eq!(j, Some(0.0));
}
