use serde_json::{Value, json};
use solidcraft_geom::Vec3;

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

/// A 40 x 30 plate from z0, `t` thick, its own body, with a through hole of `d` at (20, 15).
fn plate(s: &mut Session, name: &str, z0: Value, t: Value, top: Value, d: f64) {
    run(s, "solid.box", json!({"length": 40, "width": 30, "height": t, "corner": [0, 0, z0], "operation": "new", "body_name": name}));
    run(s, "solid.hole", json!({"position": [20, 15, top], "diameter": d}));
}

/// The world bounding box of one occurrence's bodies.
fn bbox(s: &Session, occ: u64) -> (Vec3, Vec3) {
    let comp = s.doc.occurrences.iter().find(|o| o.id == occ).map_or(0, |o| o.component);
    let m = solidcraft_doc::joints::occurrence_world(&s.doc, occ);
    let st = s.model.state();
    let (mut lo, mut hi) = (Vec3::new(f64::MAX, f64::MAX, f64::MAX), Vec3::new(f64::MIN, f64::MIN, f64::MIN));
    for b in st.bodies.iter().filter(|b| s.doc.model_body_component(b) == comp) {
        for p in &b.mesh().positions {
            let q = solidcraft_doc::apply_point(&m, *p);
            lo = Vec3::new(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z));
            hi = Vec3::new(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z));
        }
    }
    (lo, hi)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

fn occ(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

/// A part library of hex bolts in three sizes, each length with the grip it holds.
struct Library(std::path::PathBuf);

impl Library {
    fn bolts() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path =
            solidcraft_io::vfs::temp_dir().join(format!("solidcraft-fasten-{}-{}.json", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let size = |size: &str, d: f64, head: f64| {
            json!({"size": size, "d": d, "head": {"d": head, "h": d * 0.7, "style": "hex"},
                   "lengths": [{"length": 10, "grip": [0, 5]}, {"length": 14, "grip": [5, 9], "article": format!("EX-{size}-14")}, {"length": 18, "grip": [9, 13]}]})
        };
        let lib = json!({"format": "solidcraft-parts/1", "id": "example.bolts", "name": "Example bolts", "families": [
            {"kind": "fastener_family", "name": "hex_bolt_grip", "label": "Bolt", "shape": "bolt",
             "sizes": [size("M4", 4.0, 7.0), size("M5", 5.0, 8.0), size("M6", 6.0, 10.0)]}
        ]});
        std::fs::write(&path, lib.to_string()).unwrap_or_else(|e| panic!("{e}"));
        Library(path)
    }
    fn path(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn an_m5_bolt_is_chosen_by_the_hole_and_the_grip() {
    let lib = Library::bolts();
    let mut s = Session::default();
    run(&mut s, "parts.library.load", json!({"path": lib.path()}));
    plate(&mut s, "Lower", json!(0), json!(3), json!(3), 5.5);
    plate(&mut s, "Upper", json!(3), json!(4), json!(7), 5.5);
    // A dry run says what would go where, and places nothing.
    let dry = run(&mut s, "parts.fasten", json!({"family": "hex_bolt_grip", "dry_run": true}));
    assert_eq!(
        (dry["dry_run"].clone(), dry["placed"][0]["size"].as_str(), dry["placed"][0]["length_mm"].as_f64()),
        (json!(true), Some("M5"), Some(14.0)),
        "{dry}"
    );
    assert!(s.doc.parts.is_empty() && dry["placed"][0]["occurrence"].is_null(), "{dry}");
    let undo = s.undo.len();
    let r = run(&mut s, "parts.fasten", json!({"family": "hex_bolt_grip"}));
    assert_eq!(s.undo.len(), undo + 1, "one undo step");
    let placed = r["placed"].as_array().cloned().unwrap_or_default();
    assert_eq!((placed.len(), r["unfilled"].clone()), (1, json!([])), "{r}");
    // M6 does not pass a 5.5 hole; M5's 14 holds a grip of 7 (5–9), with its article.
    let p = &placed[0];
    assert_eq!((p["size"].as_str(), p["length_mm"].as_f64(), p["grip_range"].clone()), (Some("M5"), Some(14.0), json!([5.0, 9.0])), "{p}");
    assert_eq!((p["article"].as_str(), p["part"].as_str(), p["head_side"].as_str()), (Some("EX-M5-14"), Some("EX-M5-14"), Some("start")), "{p}");
    assert!(near(p["grip_mm"].as_f64().unwrap_or(0.0), 7.0) && p["joint"].is_number(), "{p}");
    // Seated on the top face (z 7), the shank down the hole.
    let (lo, hi) = bbox(&s, occ(&p["occurrence"]));
    assert!(near(hi.z, 7.0 + 3.5) && near(lo.z, 7.0 - 14.0), "{lo:?} {hi:?}");
    assert!(near((lo.x + hi.x) / 2.0, 20.0) && near((lo.y + hi.y) / 2.0, 15.0), "{lo:?} {hi:?}");
    // Undo takes all of it away.
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc.parts.is_empty());
}

#[test]
fn a_built_in_bolt_gets_washers_and_a_nut_and_room_for_them() {
    let mut s = Session::default();
    plate(&mut s, "Lower", json!(0), json!(2), json!(2), 6.4);
    plate(&mut s, "Upper", json!(2), json!(3), json!(5), 6.4);
    let r = run(&mut s, "parts.fasten", json!({"family": "hex_bolt", "stack": 0, "washer": true, "nut": true}));
    let p = &r["placed"][0];
    // M6: grip 5 + two washers 1.6 + nut 5.2 + two pitches 2 → 15.4, the next standard length 16.
    assert_eq!((p["size"].as_str(), p["length_mm"].as_f64(), p["part"].as_str()), (Some("M6"), Some(16.0), Some("hex_bolt/M6x16")), "{r}");
    let washers: Vec<u64> = p["washers"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
    assert_eq!(washers.len(), 2, "{r}");
    let (lo, hi) = bbox(&s, washers[0]);
    assert!(near(lo.z, 5.0) && near(hi.z, 6.6), "washer under the head {lo:?} {hi:?}");
    let (lo, hi) = bbox(&s, washers[1]);
    assert!(near(lo.z, -1.6) && near(hi.z, 0.0), "washer under the nut {lo:?} {hi:?}");
    let (lo, hi) = bbox(&s, occ(&p["nut"]));
    assert!(near(lo.z, -6.8) && near(hi.z, -1.6), "nut {lo:?} {hi:?}");
    let (lo, hi) = bbox(&s, occ(&p["occurrence"]));
    assert!(near(hi.z, 6.6 + 4.0) && near(lo.z, 6.6 - 16.0), "bolt {lo:?} {hi:?}");
    // The two washers are one part placed twice; the placed parts' holes make no new stacks.
    assert_eq!(s.doc.parts.len(), 3, "bolt, washer, nut");
    let stacks = run(&mut s, "inspect.hole_stacks", json!({}));
    assert_eq!(stacks["stacks"].as_array().map(Vec::len), Some(1), "{stacks}");
    assert_eq!(stacks["stacks"][0]["layers"].as_array().map(Vec::len), Some(2), "{stacks}");
}

#[test]
fn a_washer_can_go_under_the_nut_only() {
    let mut s = Session::default();
    plate(&mut s, "Lower", json!(0), json!(2), json!(2), 6.4);
    plate(&mut s, "Upper", json!(2), json!(3), json!(5), 6.4);
    let r = run(&mut s, "parts.fasten", json!({"family": "hex_bolt", "washer": true, "washer_side": "nut", "nut": true}));
    let p = &r["placed"][0];
    let washers: Vec<u64> = p["washers"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
    assert_eq!(washers.len(), 1, "{r}");
    let (lo, hi) = bbox(&s, washers[0]);
    assert!(near(lo.z, -1.6) && near(hi.z, 0.0), "under the nut {lo:?} {hi:?}");
    // The head sits on the plate: grip 5 + one washer 1.6 + nut 5.2 + 2 → 13.8, length 16.
    let (_, hi) = bbox(&s, occ(&p["occurrence"]));
    assert!(near(hi.z, 5.0 + 4.0) && p["length_mm"].as_f64() == Some(16.0), "{r}");
    for bad in [json!({"family": "hex_bolt", "washer_side": "nut"}), json!({"family": "hex_bolt", "washer": true, "washer_side": "nut"})] {
        assert!(s.execute("parts.fasten", &bad).is_err(), "{bad}");
    }
}

#[test]
fn every_stack_gets_its_own_placement_of_one_part() {
    let lib = Library::bolts();
    let mut s = Session::default();
    run(&mut s, "parts.library.load", json!({"path": lib.path()}));
    run(&mut s, "solid.box", json!({"length": 80, "width": 30, "height": 3, "corner": [0, 0, 0], "operation": "new", "body_name": "Lower"}));
    run(&mut s, "solid.box", json!({"length": 80, "width": 30, "height": 4, "corner": [0, 0, 3], "operation": "new", "body_name": "Upper"}));
    for x in [10, 30, 50, 70] {
        run(&mut s, "solid.hole", json!({"position": [x, 15, 7], "diameter": 5.5}));
    }
    let r = run(&mut s, "parts.fasten", json!({"family": "hex_bolt_grip"}));
    let placed = r["placed"].as_array().cloned().unwrap_or_default();
    assert_eq!(placed.len(), 4, "{r}");
    // One component, four occurrences, each over its own hole.
    let comps: std::collections::BTreeSet<u64> = placed.iter().filter_map(|p| p["component"].as_u64()).collect();
    assert_eq!((comps.len(), s.doc.parts.len()), (1, 1), "{r}");
    let mut xs: Vec<f64> = placed
        .iter()
        .map(|p| {
            let (lo, hi) = bbox(&s, occ(&p["occurrence"]));
            assert!(near(hi.z, 7.0 + 3.5), "{lo:?} {hi:?}");
            ((lo.x + hi.x) / 2.0 * 1e3).round() / 1e3
        })
        .collect();
    xs.sort_by(f64::total_cmp);
    assert_eq!(xs, [10.0, 30.0, 50.0, 70.0]);
    // Filling again places more of the same part.
    run(&mut s, "parts.fasten", json!({"family": "hex_bolt_grip", "stack": 0}));
    assert_eq!(s.doc.parts.len(), 1);
}

#[test]
fn the_joint_keeps_the_fastener_seated_when_a_parameter_changes() {
    let mut s = Session::default();
    run(&mut s, "parameters.add", json!({"name": "hx", "expression": "20 mm"}));
    run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 2, "corner": [0, 0, 0], "operation": "new", "body_name": "Lower"}));
    run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 3, "corner": [0, 0, 2], "operation": "new", "body_name": "Upper"}));
    run(&mut s, "solid.hole", json!({"position": ["hx", 15, 5], "diameter": 5.5}));
    let r = run(&mut s, "parts.fasten", json!({"family": "socket_head_cap_screw", "size": "M5"}));
    let o = occ(&r["placed"][0]["occurrence"]);
    let (lo, hi) = bbox(&s, o);
    assert!(near((lo.x + hi.x) / 2.0, 20.0) && near(hi.z, 5.0 + 5.0), "{r}");
    run(&mut s, "parameters.change", json!({"name": "hx", "expression": "26 mm"}));
    let (lo, hi) = bbox(&s, o);
    assert!(near((lo.x + hi.x) / 2.0, 26.0) && near(hi.z, 5.0 + 5.0), "the screw follows the hole: {lo:?} {hi:?}");
}

#[test]
fn stacks_a_fastener_cannot_fill_are_reported() {
    let lib = Library::bolts();
    let mut s = Session::default();
    run(&mut s, "parts.library.load", json!({"path": lib.path()}));
    plate(&mut s, "A", json!(0), json!(2), json!(2), 3.2);
    plate(&mut s, "B", json!(2), json!(2), json!(4), 3.2);
    // Too narrow for any size, and a grip past the longest length.
    let r = run(&mut s, "parts.fasten", json!({"family": "hex_bolt_grip"}));
    assert_eq!(r["placed"], json!([]), "{r}");
    assert!(r["unfilled"][0]["reason"].as_str().is_some_and(|w| w.contains("passes a 3.2")), "{r}");
    let stacks = run(&mut s, "inspect.hole_stacks", json!({}));
    let mut thick = stacks["stacks"][0].clone();
    thick["grip_mm"] = json!(20.0);
    thick["layers"][0]["d_mm"] = json!(6.5);
    thick["layers"][1]["d_mm"] = json!(6.5);
    let r = run(&mut s, "parts.fasten", json!({"family": "hex_bolt_grip", "stacks": [thick]}));
    assert!(r["unfilled"][0]["reason"].as_str().is_some_and(|w| w.contains("grip")), "{r}");
    // A head side on a body neither outer layer belongs to.
    let r = run(&mut s, "parts.fasten", json!({"family": "dowel_pin", "head_side": {"body": "C"}}));
    assert!(r["unfilled"][0]["reason"].as_str().is_some_and(|w| w.contains("`C`")), "{r}");
    for bad in [
        json!({}),
        json!({"family": "nope"}),
        json!({"family": "hex_bolt", "stack": 7}),
        json!({"family": "hex_bolt", "stacks": "some"}),
        json!({"family": "hex_bolt", "head_side": "middle"}),
        json!({"family": "hex_bolt", "rule": "longest"}),
        json!({"family": "hex_bolt", "stack": {"axis": 1}}),
        json!({"family": "hex_bolt", "engagement": 5}),
        json!({"family": "hex_bolt", "rule": "thread", "nut": true}),
        json!({"family": "hex_bolt", "dry_run": "yes"}),
    ] {
        assert!(s.execute("parts.fasten", &bad).is_err(), "{bad}");
    }
}

#[test]
fn the_head_can_go_on_the_end_face_or_a_body_or_per_stack() {
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 2, "corner": [0, 0, 0], "operation": "new", "body_name": "Lower"}));
    run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 2, "corner": [0, 0, 2], "operation": "new", "body_name": "Upper"}));
    for x in [10, 30] {
        run(&mut s, "solid.hole", json!({"position": [x, 15, 4], "diameter": 4.3}));
    }
    let r = run(&mut s, "parts.fasten", json!({"family": "socket_head_cap_screw", "head_side": "end"}));
    for p in r["placed"].as_array().into_iter().flatten() {
        assert_eq!((p["size"].as_str(), p["head_side"].as_str()), (Some("M4"), Some("end")), "{r}");
        let (lo, hi) = bbox(&s, occ(&p["occurrence"]));
        // Under the bottom face, the shank up the hole: M4 head 4 high, length 4 (≥ grip 4).
        assert!(near(lo.z, -4.0) && near(hi.z, 4.0), "{lo:?} {hi:?}");
    }
    run(&mut s, "edit.undo", json!({}));
    // By body: the head on the Lower plate's face; and a stack of its own on top.
    let r = run(
        &mut s,
        "parts.fasten",
        json!({"family": "socket_head_cap_screw", "head_side": {"body": "Lower"}, "stacks": [0, {"index": 1, "head_side": "start"}]}),
    );
    let sides: Vec<&str> = r["placed"].as_array().into_iter().flatten().filter_map(|p| p["head_side"].as_str()).collect();
    assert_eq!(sides, ["end", "start"], "{r}");
}

#[test]
fn a_screw_into_a_tapped_layer_is_long_enough_to_engage_it() {
    let mut s = Session::default();
    // A 3 mm plate with a clearance hole on a 10 mm block drilled 4.2 for an M5 thread.
    plate(&mut s, "Block", json!(0), json!(10), json!(10), 4.2);
    run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 3, "corner": [0, 0, 10], "operation": "new", "body_name": "Plate"}));
    run(&mut s, "solid.hole", json!({"position": [20, 15, 13], "diameter": 5.5, "depth": 3}));
    let r = run(&mut s, "parts.fasten", json!({"family": "socket_head_cap_screw", "rule": "thread"}));
    let p = &r["placed"][0];
    // M5 passes the plate's hole and not the tapped one; 3 + 1.5 × 5 = 10.5 → 12.
    assert_eq!((p["size"].as_str(), p["length_mm"].as_f64()), (Some("M5"), Some(12.0)), "{r}");
    let r = run(&mut s, "parts.fasten", json!({"family": "socket_head_cap_screw", "rule": "thread", "engagement": 5, "dry_run": true}));
    assert_eq!(r["placed"][0]["length_mm"].as_f64(), Some(8.0), "{r}");
    // By grip, M4 passes both holes and is clamped through all 13 mm.
    let r = run(&mut s, "parts.fasten", json!({"family": "socket_head_cap_screw", "dry_run": true}));
    assert_eq!((r["placed"][0]["size"].as_str(), r["placed"][0]["length_mm"].as_f64()), (Some("M4"), Some(16.0)), "{r}");
}

#[test]
fn a_stack_on_a_tilted_axis_takes_a_bolt_and_nut_on_either_face() {
    // Two rings on an axis 7.6° off vertical (the far face is where seating used to fail).
    let mut s = Session::default();
    let a = Vec3::new(0.0, 0.13191, 0.99126);
    let a = a.normalized().unwrap_or(Vec3::Z);
    for (name, at) in [("A", 0.0), ("B", 2.0)] {
        let base = a * at;
        run(&mut s, "solid.cylinder", json!({"radius": 8, "height": 2, "base": base, "axis": a, "body_name": name}));
        let tool = format!("{name}_hole");
        run(&mut s, "solid.cylinder", json!({"radius": 3.2, "height": 4, "base": base - a, "axis": a, "body_name": tool}));
        run(&mut s, "solid.combine", json!({"target": name, "tools": [tool], "operation": "cut"}));
    }
    for side in ["start", "end"] {
        let r = run(&mut s, "parts.fasten", json!({"family": "hex_bolt", "washer": true, "nut": true, "head_side": side}));
        assert_eq!((r["placed"].as_array().map(Vec::len), r["unfilled"].clone()), (Some(1), json!([])), "{side}: {r}");
        // The bolt's axis is the stack's: its seat on the axis line, its z along it.
        let m = solidcraft_doc::joints::occurrence_world(&s.doc, occ(&r["placed"][0]["occurrence"]));
        let (at, z) = (Vec3::new(m[3][0], m[3][1], m[3][2]), Vec3::new(m[2][0], m[2][1], m[2][2]));
        let off = at - a * at.dot(a);
        assert!(off.len() < 1e-3 && z.dot(a).abs() > 1.0 - 1e-6, "{side}: {at:?} {z:?}");
        // On the start face (the far end of B, at 4 along the axis), or the end face (A's at 0).
        let along = at.dot(a);
        assert!(if side == "start" { along > 4.0 - 1e-3 } else { along < 1e-3 }, "{side}: {along}");
        run(&mut s, "edit.undo", json!({}));
    }
}
