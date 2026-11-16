"""Generic repros for three kernel-perf issues: a 800 x 500 x 10 slab with tapered grooves in three directions."""
import json, math, sys
from shapely.geometry import Polygon
from shapely.ops import unary_union
W, H, T, DEPTH, WIDTH, TAPER = 800.0, 500.0, 10.0, -8.0, 8.0, -20.0
DIRS = [0.0, 60.0, 120.0]
def slab():
    return [{"command": "sketch.create", "params": {"plane": "XY", "name": "Slab"}},
            {"command": "sketch.rectangle.two_point", "params": {"p0": [0, 0], "p1": [W, H]}},
            {"command": "sketch.finish", "params": {}},
            {"command": "solid.extrude", "params": {"sketch": "Slab", "distance": T, "name": "Slab"}}]
def groove(i, n):
    d = i % 3; k = i // 3; a = math.radians(DIRS[d])
    cx = W / 2 + (k - n / 6) * 21.0 * (1 if d else 0); cy = H / 2 + (k - n / 6) * 15.0 * (0 if d else 1) + d * 3.0
    L = 1400.0; ux, uy = math.cos(a), math.sin(a); vx, vy = -uy, ux; w = WIDTH
    return [[cx - ux * L - vx * w / 2, cy - uy * L - vy * w / 2], [cx + ux * L - vx * w / 2, cy + uy * L - vy * w / 2],
            [cx + ux * L + vx * w / 2, cy + uy * L + vy * w / 2], [cx - ux * L + vx * w / 2, cy - uy * L + vy * w / 2]]
def plane(): return {"origin": [0, 0, T + 1], "x_dir": [1, 0, 0], "y_dir": [0, 1, 0]}
def cut(sk): return {"command": "solid.extrude", "params": {"sketch": sk, "distance": DEPTH - 1, "taper": TAPER, "operation": "cut", "bodies": ["Slab"]}}
def one_per_groove(n):
    c = slab()
    for i in range(n):
        c += [{"command": "sketch.create", "params": {"plane": plane(), "name": f"G{i}"}},
              {"command": "sketch.line", "params": {"points": groove(i, n), "closed": True, "infer": False}},
              {"command": "sketch.finish", "params": {}}, cut(f"G{i}")]
    return c
def one_per_direction(n):
    c = slab()
    for d in range(3):
        c.append({"command": "sketch.create", "params": {"plane": plane(), "name": f"D{d}"}})
        for i in range(d, n, 3):
            c.append({"command": "sketch.line", "params": {"points": groove(i, n), "closed": True, "infer": False}})
        c += [{"command": "sketch.finish", "params": {}}, cut(f"D{d}")]
    return c
def one_region(n):
    u = unary_union([Polygon(groove(i, n)) for i in range(n)])
    c = slab() + [{"command": "sketch.create", "params": {"plane": plane(), "name": "All"}}]
    for g in (list(u.geoms) if hasattr(u, "geoms") else [u]):
        for ring in [g.exterior] + list(g.interiors):
            pts = []
            for x, y in list(ring.coords)[:-1]:
                if not pts or math.hypot(x - pts[-1][0], y - pts[-1][1]) > 1e-6:
                    pts.append([round(x, 9), round(y, 9)])
            if len(pts) > 2 and math.hypot(pts[0][0] - pts[-1][0], pts[0][1] - pts[-1][1]) <= 1e-6:
                pts.pop()
            if len(pts) > 2: c.append({"command": "sketch.line", "params": {"points": pts, "closed": True, "infer": False}})
    return c + [{"command": "sketch.finish", "params": {}}, cut("All")]
for name, f, n in [("grooves_30_one_cut_each", one_per_groove, 30), ("grooves_60_one_cut_per_direction", one_per_direction, 60),
                   ("grooves_9_one_region", one_region, 9), ("grooves_9_one_cut_each", one_per_groove, 9), ("grooves_60_one_cut_each", one_per_groove, 60), ("grooves_9_one_region_straight", one_region, 9)]:
    json.dump({"commands": f(n)}, open(name + ".json", "w"))
