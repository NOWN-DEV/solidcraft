//! A cut by a planar face's own extrusion (Extrude Cut from a whole face, a pocket's floor taken
//! deeper): the tool is a prism along the face's normal, its section the face's outline, reaching
//! from the face's plane (or from past it, outside the body) into the body. Its walls lie on the
//! face's neighbours, and where those are cylinders (a rounded outline) the B-rep boolean cannot
//! intersect them, nor can they be pushed apart (#1275). The 2D boolean of prisms takes such a
//! pair only when the whole body is a prism along the tool, which a chamfer or a round anywhere
//! else on it ends.
//!
//! What such a cut takes is the slab of material under the face, so the face moves in by the
//! tool's depth, its neighbours shortened or stretched to it ([`crate::coplanar::push_face_exact`]):
//! nothing is intersected, and the face keeps its plane and outline, the walls their surfaces.
//! That holds where the tool holds nothing of the body but that slab: every other face of the
//! body that comes near the tool is a wall along it standing outside the face's outline (or on
//! it), or a plane square to it beside the outline, which is decided exactly, in 2D, on the
//! face's outline.

use std::collections::HashSet;

use solidcraft_geom::{Plane, Region2, Seg2, Vec2, Vec3};
use truck_modeling as mt;

use super::{LazyIndex, MAX_PLANAR_SEGMENTS, Section, boolean2d, crossings, dist_to, inside, loop_box, prism_tagged, wall_along};
use crate::BoolOp;
use crate::body::{Body, from_p3};

/// `a` cut by `b`, when `b` is the extrusion of a planar face of `a` into it: that face moved in
/// (see the module notes). `None` where `b` is no such extrusion, where it holds more of `a` than
/// the slab under the face (a hole or a pocket reaching into that, a face there or past the face
/// that is not a wall along the tool), or where the face can't move that far (a wall it would
/// pass the end of). The result's history is over `[a, b]`: `a`'s faces, the face and its
/// neighbours modified.
pub(crate) fn cut_by_face_extrusion(a: &Body, b: &Body) -> Option<Body> {
    if a.is_mesh() || b.is_mesh() || b.solid.boundaries().len() != 1 {
        return None;
    }
    crate::guard("face extrusion cut", || Ok(face_extrusion_cut(a, b))).ok().flatten()
}

fn face_extrusion_cut(a: &Body, b: &Body) -> Option<Body> {
    let size = a.size().max(b.size());
    let tol = (size * 1e-7).max(1e-9) * 10.0;
    // The tool's directions: its planar faces' normals, each line once.
    let mut dirs: Vec<Vec3> = Vec::new();
    for f in b.solid.face_iter() {
        if let mt::Surface::Plane(p) = f.oriented_surface() {
            let n = p.normal();
            if let Some(n) = Vec3::new(n.x, n.y, n.z).normalized()
                && !dirs.iter().any(|m| m.dot(n).abs() > 1.0 - 1e-9)
            {
                dirs.push(n);
                if dirs.len() == 8 {
                    break;
                }
            }
        }
    }
    let (index, tool_index) = (LazyIndex::new(a, 0), LazyIndex::new(b, 1));
    for d in dirs {
        let plane = Plane::from_normal(Vec3::ZERO, d)?;
        let Some(tool) = prism_tagged(b, d, &plane, tol, &tool_index) else { continue };
        let section = tool.section.oriented().regions;
        for (fi, f) in a.solid.face_iter().enumerate() {
            let mt::Surface::Plane(p) = f.oriented_surface() else { continue };
            let n = p.normal();
            let Some(n) = Vec3::new(n.x, n.y, n.z).normalized() else { continue };
            let h = plane.height(from_p3(p.origin()));
            // How far the tool reaches into `a` behind the face, and out past it (the engine runs a
            // cut's end that lies on a face a little past it).
            let (depth, over) = if n.dot(d) > 1.0 - 1e-9 {
                (h - tool.lo, tool.hi - h)
            } else if n.dot(d) < -1.0 + 1e-9 {
                (tool.hi - h, h - tool.lo)
            } else {
                continue;
            };
            if depth.is_nan() || over.is_nan() || depth <= tol || over < -tol {
                continue;
            }
            let face = Face { index: fi, face: f, n, at: n.dot(from_p3(p.origin())) };
            let reach = Reach { depth, over: over.max(0.0) };
            if let Some(body) = lowered(a, b, &face, &plane, &section, reach, tol, &index) {
                return Some(body);
            }
        }
    }
    None
}

/// A planar face of the body: its place in the face order, its outward normal `n` and its plane
/// `n · p = at`.
struct Face<'a> {
    index: usize,
    face: &'a mt::Face,
    n: Vec3,
    at: f64,
}

/// How far the tool reaches into the body behind a face (`depth`), and out past it (`over`).
#[derive(Clone, Copy)]
struct Reach {
    depth: f64,
    over: f64,
}

/// `a` with `face` moved in by the tool's depth, when the tool's `section` (in `plane`, square to
/// the face) is the face's outline and the tool holds nothing of `a` but the slab under the face.
#[allow(clippy::too_many_arguments)]
fn lowered(a: &Body, b: &Body, face: &Face, plane: &Plane, section: &[Region2], reach: Reach, tol: f64, index: &LazyIndex) -> Option<Body> {
    let size = a.size().max(b.size());
    let (mut work, mut segments) = (2_000_000usize, MAX_PLANAR_SEGMENTS);
    let outline = Section::from_loops(index.get().loops(face.face, face.index, plane, tol, &mut work, &mut segments)?)?.oriented().regions;
    // The tool's section is the face's outline: neither has anything outside the other.
    let area = |rs: &[Region2]| rs.iter().map(Region2::area).sum::<f64>();
    let slack = tol * size;
    if (area(&outline) - area(section)).abs() > slack
        || area(&boolean2d(&outline, section, BoolOp::Cut, tol)?) > slack
        || area(&boolean2d(section, &outline, BoolOp::Cut, tol)?) > slack
    {
        return None;
    }
    if !only_the_slab(index, face, plane, &outline, reach, tol, &mut work) {
        return None;
    }
    let lowered = crate::coplanar::push_face_exact(a, face.index, face.n, face.at, -reach.depth).ok()?;
    // Sound: closed, and every face meshes (the caller weighs its volume against the operands'),
    // without the fine measure of [`Body::validity`], which on small rounds takes longer than the
    // whole cut.
    let closed = lowered.solid.boundaries().iter().all(|s| s.shell_condition() == truck_topology::shell::ShellCondition::Closed);
    if !closed || lowered.unmeshed_faces() > 0 {
        return None;
    }
    let history = crate::boolean_record::placed(lowered.history().cloned(), 0, 2);
    Some(lowered.with_checked_history(history, &crate::record::counts(&[a, b])))
}

/// Does the tool over `face`, `reach.depth` into `a` and `reach.over` out past the face, hold
/// nothing of `a` but the slab under the face, all material between the face's walls? Every face
/// of `a` that comes near the tool must be a wall along the face's normal (a plane square to the
/// face, or a surface swept along its normal) whose edges stand outside the face's `outline` or on
/// it; the face's neighbours must be such walls. Faces wholly beyond either end of the tool, faces
/// of other kinds wholly beside the outline's box, and planes square to the tool that share no
/// area with the outline, don't come near it. Which faces may is told from a mesh of `a` (its
/// triangles within the chord error of their faces); what a wall's edges do, and what a plane
/// shares with the outline, exactly, in 2D.
fn only_the_slab(index: &LazyIndex, face: &Face, plane: &Plane, outline: &[Region2], reach: Reach, tol: f64, work: &mut usize) -> bool {
    let a = index.body;
    let chord = (a.size() * 1e-3).max(1e-6);
    let Ok(mesh) = a.tessellate(chord) else { return false };
    let (n, count) = (face.n, a.face_count());
    // Per face: how far in its triangles lie (0 at the face's plane, `reach.depth` at the slab's
    // far end, negative past the face), least and most, and their box in the plane.
    let mut spans: Vec<Option<(f64, f64, Vec2, Vec2)>> = vec![None; count];
    for (t, fi) in mesh.triangles.iter().zip(&mesh.tri_face) {
        let (Some(tri), Some(slot)) = (mesh.tri(t), spans.get_mut(*fi as usize)) else { return false };
        for p in tri {
            let (h, q) = (face.at - n.dot(p), plane.to_local(p));
            let (t0, t1, lo, hi) = slot.get_or_insert((h, h, q, q));
            (*t0, *t1) = (t0.min(h), t1.max(h));
            (*lo, *hi) = (Vec2::new(lo.x.min(q.x), lo.y.min(q.y)), Vec2::new(hi.x.max(q.x), hi.y.max(q.y)));
        }
    }
    // The outline's box in the plane.
    let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
    for r in outline {
        let Some((l, h)) = loop_box(&r.outer) else { return false };
        (lo, hi) = (Vec2::new(lo.x.min(l.x), lo.y.min(l.y)), Vec2::new(hi.x.max(h.x), hi.y.max(h.y)));
    }
    let pad = 2.0 * chord + tol;
    let edges: HashSet<mt::EdgeID> = face.face.edge_iter().map(|e| e.id()).collect();
    let mut wall_work = 262_144;
    for (xi, x) in a.solid.face_iter().enumerate() {
        if xi == face.index {
            continue;
        }
        let surface = x.oriented_surface();
        let wall = match &surface {
            mt::Surface::Plane(p) => {
                let m = p.normal();
                Vec3::new(m.x, m.y, m.z).normalized().is_some_and(|m| m.dot(n).abs() < 1e-9)
            }
            s => wall_along(s, n, tol, &mut wall_work),
        };
        let neighbour = x.edge_iter().any(|e| edges.contains(&e.id()));
        if neighbour && !wall {
            return false;
        }
        let Some(Some((t0, t1, xl, xh))) = spans.get(xi).copied() else { return false };
        if !neighbour && (t1 < -reach.over - pad || t0 > reach.depth + pad) {
            continue;
        }
        if !wall {
            if xl.x > hi.x + pad || xh.x < lo.x - pad || xl.y > hi.y + pad || xh.y < lo.y - pad || beside(index, x, xi, plane, outline, tol, work) {
                continue;
            }
            return false;
        }
        for e in x.edge_iter() {
            let (p, q) = (from_p3(e.front().point()), from_p3(e.back().point()));
            if matches!(e.curve(), mt::Curve::Line(_)) && (q - p).cross(n).len() < tol * (q - p).len().max(1.0) {
                // An edge along the normal stands where its ends do.
                if within(outline, plane.to_local(p), tol) {
                    return false;
                }
                continue;
            }
            let Some(pieces) = crate::prism_curve::segment(&e, plane, tol, work) else { return false };
            if pieces.iter().any(|s| enters(outline, s, tol)) {
                return false;
            }
        }
    }
    true
}

/// Is face `x` (number `xi`) a plane square to the tool (a step, a floor, the top around a pocket)
/// that shares no area with the `outline`?
fn beside(index: &LazyIndex, x: &mt::Face, xi: usize, plane: &Plane, outline: &[Region2], tol: f64, work: &mut usize) -> bool {
    let mt::Surface::Plane(p) = x.oriented_surface() else { return false };
    let m = p.normal();
    if Vec3::new(m.x, m.y, m.z).normalized().is_none_or(|m| m.dot(plane.normal()).abs() < 1.0 - 1e-9) {
        return false;
    }
    let mut segments = MAX_PLANAR_SEGMENTS;
    let Some(regions) = index.get().loops(x, xi, plane, tol, work, &mut segments).and_then(Section::from_loops).map(|s| s.oriented().regions) else {
        return false;
    };
    let slack = tol * index.body.size();
    boolean2d(&regions, outline, BoolOp::Intersect, tol).is_some_and(|common| common.iter().map(Region2::area).sum::<f64>() <= slack)
}

/// Is `p` inside the regions, farther than the tolerance from their outline?
fn within(regions: &[Region2], p: Vec2, tol: f64) -> bool {
    inside(regions, p) && dist_to(regions, p) > tol * 10.0
}

/// Does the segment reach into the regions, farther than the tolerance from their outline? It is
/// cut where it crosses the outline; each piece lies inside or out, as its middle does.
fn enters(regions: &[Region2], s: &Seg2, tol: f64) -> bool {
    let mut cuts = vec![0.0, 1.0];
    for r in regions {
        for t in std::iter::once(&r.outer).chain(&r.holes).flat_map(|l| l.segs.iter()) {
            cuts.extend(crossings(s, t, tol).into_iter().map(|(u, _)| u.clamp(0.0, 1.0)));
        }
    }
    cuts.sort_by(f64::total_cmp);
    [s.start(), s.end()].into_iter().any(|p| within(regions, p, tol))
        || cuts.windows(2).any(|w| matches!(w, [u, v] if v - u > 1e-9 && within(regions, s.point_at((u + v) * 0.5), tol)))
}
