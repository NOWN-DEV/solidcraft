#!/usr/bin/env python3
"""Generic speed repro: a sheet-metal tray with lips, unfolded, perforated with N round holes (batches of 400, each one
sketch from a DXF and one extrude cut), refolded. Usage: gen_tray.py N out.json [--walls] [--circles]"""
import json, math, sys, os
n = int(sys.argv[1]); out = sys.argv[2]
walls = "--walls" in sys.argv          # the field runs over the long bends onto the walls
circles = "--circles" in sys.argv      # sketch.circle.center instead of a DXF
BATCH = next((int(a.split("=")[1]) for a in sys.argv if a.startswith("--batch=")), 400)
OFF = "--off" in sys.argv              # the full check off before the first cut
L, W, H, LIP, T, R = 1200.0, 600.0, 40.0, 20.0, 1.0, 1.0
D, PITCH = 5.0, 10.0
cmds = [
    {"command": "sheet.manage_rules", "params": {"name": "R", "thickness": f"{T} mm", "bend_radius": f"{R} mm", "k_factor": "0.44", "active": True}},
    {"command": "sketch.create", "params": {"plane": "XY", "name": "Base_Sketch"}},
    {"command": "sketch.line", "params": {"points": [[-L/2, -W/2], [L/2, -W/2], [L/2, W/2], [-L/2, W/2]], "closed": True}},
    {"command": "sketch.finish", "params": {}},
    {"command": "sheet.flange", "params": {"type": "base", "sketch": "Base_Sketch", "rule": "R", "name": "Base"}},
    {"command": "sheet.flange", "params": {"edges": [[0, -W/2, T], [0, W/2, T]], "height": f"{H} mm", "angle": "90 deg", "name": "Walls_Long"}},
    {"command": "sheet.flange", "params": {"edges": [[L/2, 0, T], [-L/2, 0, T]], "height": f"{H} mm", "angle": "90 deg", "name": "Walls_Short"}},
    {"command": "sheet.flange", "params": {"edges": [[0, -W/2 + T, H], [0, W/2 - T, H]], "height": f"{LIP} mm", "angle": "90 deg", "name": "Lips"}},
    {"command": "sheet.unfold", "params": {}},
]
# a staggered field centred on the base; rows fill from the middle outwards until N holes
half_w = (W/2 + (H - 8 if walls else -15))
xs = int((L - 40) // PITCH)
rows = int(2 * half_w // (PITCH/2))
pts = []
for j in range(rows):
    y = -half_w + (j + 0.5) * PITCH/2 * 1.0
    off = PITCH/2 if j % 2 else 0.0
    for i in range(xs):
        x = -(L - 40)/2 + off + i * PITCH + PITCH/4
        if abs(x) < L/2 - 15:
            pts.append((x, y))
pts.sort(key=lambda p: (abs(p[1]), p[0]))
pts = sorted(pts[:n], key=lambda p: (p[1], p[0]))
assert len(pts) == n, (len(pts), n)
def dxf(ps):
    s = ["0", "SECTION", "2", "ENTITIES"]
    for x, y in ps:
        s += ["0", "CIRCLE", "8", "0", "10", f"{x:.6f}", "20", f"{y:.6f}", "30", "0.0", "40", f"{D/2:.6f}"]
    s += ["0", "ENDSEC", "0", "EOF"]
    return "\n".join(s) + "\n"
if OFF:
    cmds.append({"command": "diagnostics.validation", "params": {"mode": "off"}})
for k in range(0, n, BATCH):
    b = pts[k:k+BATCH]; name = f"Cut_{k//BATCH + 1}"
    cmds.append({"command": "sketch.create", "params": {"plane": {"origin": [0, 0, 0], "x_dir": [1, 0, 0], "y_dir": [0, 1, 0]}, "name": name}})
    if circles:
        cmds += [{"command": "sketch.circle.center", "params": {"center": [x, y], "diameter": D}} for x, y in b]
    else:
        f = os.path.abspath(out[:-5] + f".{name}.dxf"); open(f, "w").write(dxf(b))
        cmds.append({"command": "sketch.insert_dxf", "params": {"path": f}})
    cmds += [{"command": "sketch.finish", "params": {}},
             {"command": "solid.extrude", "params": {"sketch": name, "profiles": "all", "distance": f"{4*T} mm", "direction": "symmetric", "operation": "cut", "name": name}}]
cmds.append({"command": "sheet.refold", "params": {}})
json.dump({"commands": cmds}, open(out, "w"))
print(out, n, "holes", len(cmds), "commands")
