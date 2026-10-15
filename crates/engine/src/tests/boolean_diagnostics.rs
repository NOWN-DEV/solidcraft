use super::*;
use solidcraft_kernel::BooleanStrategy;

#[test]
fn boolean_retry_warning_and_details_survive_cache_undo_and_reopen() {
    let slope = 1f64.to_radians().tan();
    let mut session = Session::default();
    run(&mut session, "sketch.create", json!({"plane":"XZ","name":"Floor section"}));
    run(&mut session, "sketch.line", json!({"points":[[0,0],[100,0],[100,-10.-slope*100.],[0,-10.]],"closed":true}));
    run(&mut session, "sketch.finish", json!({}));
    run(&mut session, "solid.extrude", json!({"sketch":"Floor section","distance":80,"body_name":"Floor"}));
    run(&mut session, "solid.cylinder", json!({"base":[50,40,-5],"radius":10,"height":30,"operation":"cut"}));
    run(&mut session, "sketch.create", json!({"plane":"XY","name":"Ring section"}));
    run(&mut session, "sketch.circle.center", json!({"center":[50,40],"radius":20}));
    run(&mut session, "sketch.circle.center", json!({"center":[50,40],"radius":10}));
    run(&mut session, "sketch.finish", json!({}));
    run(
        &mut session,
        "solid.extrude",
        json!({"sketch":"Ring section","profiles":[{"point":[65,40]}],"start_offset":-5,"distance":7,"body_name":"Ring"}),
    );
    let id = run(&mut session, "solid.combine", json!({"target":"Floor","tools":["Ring"],"operation":"join"}))["feature"].as_u64().unwrap();
    let check = |session: &Session| {
        let result = session.model.result(id).unwrap();
        assert!(result.error.is_none());
        assert!(result.warning.as_ref().is_some_and(|warning| warning.contains("Boolean retry:")), "{result:?}");
        assert!(
            result.boolean_diagnostics.events.iter().any(|event| event.strategy == BooleanStrategy::CommonBore),
            "{:?}",
            result.boolean_diagnostics
        );
        result.boolean_diagnostics.clone()
    };
    let original = check(&session);
    let expected = (10. + slope * 50.) * (8000. - PI * 100.) + PI * 300. * 5.;
    assert!(rel(volume(&mut session), expected) < 2e-5);
    let inspected = run(&mut session, "timeline.inspect", json!({"feature":id}));
    assert_eq!(inspected["boolean_diagnostics"], serde_json::to_value(&original).unwrap());
    session.refresh();
    assert_eq!(check(&session), original);
    run(&mut session, "edit.undo", json!({}));
    run(&mut session, "edit.redo", json!({}));
    assert_eq!(check(&session), original);
    let reopened_doc = solidcraft_doc::Document::from_json(&session.doc.to_json()).unwrap();
    let reopened = Session::new(reopened_doc);
    assert_eq!(check(&reopened), original);
    // A later independent feature must not inherit the earlier retry's warning.
    run(&mut session, "solid.box", json!({"corner":[200,0,0],"length":5,"width":5,"height":5}));
    let last = session.model.results.last().unwrap();
    assert!(last.warning.is_none());
    assert!(last.boolean_diagnostics.events.is_empty());
}

/// A barrel joined under a plate's back edge, its ends flush with the sides of the leaf it hangs
/// from (#827 without its fillets): the join is exact, so the command's reply carries no Boolean
/// retry, and the barrel's ends stay in the leaf's sides (it is not nudged to make the join go).
#[test]
fn a_barrel_joined_between_the_sides_of_a_leaf_needs_no_retry() {
    let mut session = Session::default();
    run(&mut session, "solid.box", json!({"length": 40, "width": 20, "height": 6, "corner": [0, 0, 5], "body_name": "Plate"}));
    run(&mut session, "solid.box", json!({"length": 14, "width": 10, "height": 6, "corner": [10, 10, 0], "operation": "join", "targets": ["Plate"]}));
    let reply = run(
        &mut session,
        "solid.cylinder",
        json!({"radius": 5, "height": 14, "axis": "X", "base": [10, 20, 0], "operation": "join", "targets": ["Plate"]}),
    );
    assert!(reply["warning"].is_null(), "{reply}");
    let result = session.model.result(reply["feature"].as_u64().unwrap()).unwrap();
    assert!(result.boolean_diagnostics.events.is_empty(), "{:?}", result.boolean_diagnostics);
    let state = session.model.state();
    let plate = &state.body("Plate").unwrap().body;
    assert_eq!(plate.shell_count(), 1);
    assert!(plate.validity().is_empty(), "{:?}", plate.validity());
    // Every edge ends on a level along x of the plate, the leaf or the barrel.
    for edge in plate.edges(1e-3).unwrap() {
        for p in [edge.points.first(), edge.points.last()].into_iter().flatten() {
            assert!([0., 10., 24., 40.].iter().any(|x| (p.x - x).abs() < 1e-9), "an edge ends at {p:?}");
        }
    }
    // A quarter of the barrel lies in the leaf.
    let expected = 5500. + 0.75 * PI * 25. * 14.;
    let actual = plate.tessellate(1e-4).unwrap().measure().volume;
    assert!(rel(actual, expected) < 1e-5, "{actual} vs {expected}");
}

/// Extrude Cut from a whole face after a chamfer or a round around the bottom of a body with
/// rounded corners (#1275, the issue's script): a rounded square 40 × 40, radius 8, 20 high, its
/// top face cut 5 down. The cut's walls lie on the body's walls and corner cylinders; it failed
/// after every retry ("boolean Cut: the operation failed: no result"). The top moves down
/// instead, with no retry: the body keeps the rounded square's section up to z = 15, and every
/// edge ends at the level of its bottom, its blend or its new top.
#[test]
fn a_whole_face_cut_after_a_bottom_blend_lowers_the_face() {
    let outline = 1600. - (4. - PI) * 64.;
    // What the 2 mm blend takes along the four 24 mm sides, and turned a quarter about each
    // corner's axis (Pappus): the chamfer's triangle of area 2, its centroid (8 + 8 + 6) / 3 from
    // the axis; the round's region of area 4 − π, its centroid 2 (10 − 3π) / (3 (4 − π)) in from
    // the wall.
    let chamfer = 2. * 96. + 2. * 2. * PI * 22. / 3.;
    let round = (4. - PI) * (96. + 2. * PI * (8. - 2. * (10. - 3. * PI) / (3. * (4. - PI))));
    let edges =
        json!([[20, 0, 0], [40, 20, 0], [20, 40, 0], [0, 20, 0], [2.343, 2.343, 0], [37.657, 2.343, 0], [37.657, 37.657, 0], [2.343, 37.657, 0]]);
    for (blend, params, takes) in
        [("solid.chamfer", json!({"distance": 2, "edges": edges}), chamfer), ("solid.fillet", json!({"radius": 2, "edges": edges}), round)]
    {
        let mut session = Session::default();
        run(&mut session, "sketch.create", json!({"plane": "XY"}));
        run(&mut session, "sketch.rectangle.rounded", json!({"p0": [0, 0], "p1": [40, 40], "radius": 8}));
        run(&mut session, "sketch.finish", json!({}));
        run(&mut session, "solid.extrude", json!({"distance": 20, "operation": "new"}));
        run(&mut session, blend, params);
        let reply = run(&mut session, "solid.extrude", json!({"face": [20, 20, 20], "distance": -5, "operation": "cut"}));
        assert!(reply["warning"].is_null(), "{blend}: {reply}");
        let result = session.model.result(reply["feature"].as_u64().unwrap()).unwrap();
        assert!(result.boolean_diagnostics.events.is_empty(), "{blend}: {:?}", result.boolean_diagnostics);
        let state = session.model.state();
        let body = &state.body("Body1").unwrap().body;
        assert_eq!(body.shell_count(), 1, "{blend}");
        assert!(body.validity().is_empty(), "{blend}: {:?}", body.validity());
        for edge in body.edges(1e-3).unwrap() {
            for p in [edge.points.first(), edge.points.last()].into_iter().flatten() {
                assert!([0., 2., 15.].iter().any(|z| (p.z - z).abs() < 1e-9), "{blend}: an edge ends at {p:?}");
            }
        }
        // Measured on a mesh, whose chords along the curved walls take a few hundredths of a mm³.
        let expected = outline * 15. - takes;
        let actual = body.tessellate(1e-4).unwrap().measure().volume;
        assert!(rel(actual, expected) < 2e-6, "{blend}: {actual} vs {expected}");
    }
}
