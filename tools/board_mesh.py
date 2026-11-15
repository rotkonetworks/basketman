#!/usr/bin/env python3
"""Turn a one-colour board design into triangles for the 3D arena.

A sponsor board drawn as a texture goes soft when it is seen small and at an
angle. A board drawn as geometry stays crisp at any distance. This script
takes a black-on-white picture of the board's foreground (the logo and the
lettering, rendered large from vector sources), finds its outlines, simplifies
them and cuts them into triangles.

Output: JSON {"w": aspect, "h": 1, "tris": [x0, y0, x1, y1, x2, y2, ...]}
with x in 0..aspect and y in 0..1, the origin at the bottom left.

    board_mesh.py mask.png out.json [--eps 1.5]
"""
import json
import sys

import cv2
import numpy as np


def area(ring):
    x, y = ring[:, 0], ring[:, 1]
    return 0.5 * float(np.dot(x, np.roll(y, -1)) - np.dot(y, np.roll(x, -1)))


def cross(o, a, b):
    return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])


def inside(p, a, b, c):
    d1, d2, d3 = cross(a, b, p), cross(b, c, p), cross(c, a, p)
    neg = d1 < 0 or d2 < 0 or d3 < 0
    pos = d1 > 0 or d2 > 0 or d3 > 0
    return not (neg and pos)


def segments_cross(p, q, a, b):
    d1, d2 = cross(p, q, a), cross(p, q, b)
    d3, d4 = cross(a, b, p), cross(a, b, q)
    return (d1 > 0) != (d2 > 0) and (d3 > 0) != (d4 > 0) and d1 != 0 and d2 != 0 and d3 != 0 and d4 != 0


def merge_hole(outer, hole):
    """Join a hole to the outer ring with a two-way bridge, choosing the
    shortest bridge that crosses neither ring."""
    best = None
    rings = [outer, hole]
    for i, p in enumerate(hole):
        for j, q in enumerate(outer):
            d = (p[0] - q[0]) ** 2 + (p[1] - q[1]) ** 2
            if best is not None and d >= best[0]:
                continue
            ok = True
            for ring in rings:
                n = len(ring)
                for k in range(n):
                    a, b = ring[k], ring[(k + 1) % n]
                    if (a == p).all() or (b == p).all() or (a == q).all() or (b == q).all():
                        continue
                    if segments_cross(p, q, a, b):
                        ok = False
                        break
                if not ok:
                    break
            if ok:
                best = (d, i, j)
    _, i, j = best
    return np.concatenate([outer[: j + 1], hole[i:], hole[: i + 1], outer[j:]])


def ear_clip(ring):
    pts = [tuple(p) for p in ring]
    idx = list(range(len(pts)))
    tris = []
    guard = 0
    while len(idx) > 3 and guard < 200000:
        guard += 1
        n = len(idx)
        clipped = False
        for k in range(n):
            ia, ib, ic = idx[k - 1], idx[k], idx[(k + 1) % n]
            a, b, c = pts[ia], pts[ib], pts[ic]
            if cross(a, b, c) <= 1e-9:
                continue
            ear = True
            for m in idx:
                if m in (ia, ib, ic):
                    continue
                p = pts[m]
                if p == a or p == b or p == c:
                    continue
                if inside(p, a, b, c):
                    ear = False
                    break
            if ear:
                tris.append((a, b, c))
                del idx[k]
                clipped = True
                break
        if not clipped:
            # Degenerate leftovers (collinear points): drop one and go on.
            del idx[0]
    if len(idx) == 3:
        a, b, c = (pts[i] for i in idx)
        if abs(cross(a, b, c)) > 1e-9:
            tris.append((a, b, c))
    return tris


def main():
    src, out = sys.argv[1], sys.argv[2]
    eps = float(sys.argv[sys.argv.index("--eps") + 1]) if "--eps" in sys.argv else 1.5
    img = cv2.imread(src, cv2.IMREAD_GRAYSCALE)
    h, w = img.shape
    mask = (img < 128).astype(np.uint8)
    contours, hier = cv2.findContours(mask, cv2.RETR_CCOMP, cv2.CHAIN_APPROX_NONE)
    tris = []
    verts = 0
    for i, c in enumerate(contours):
        if hier[0][i][3] != -1:
            continue  # a hole: handled with its parent
        outer = cv2.approxPolyDP(c, eps, True).reshape(-1, 2).astype(np.float64)
        if len(outer) < 3 or abs(area(outer)) < 12:
            continue
        # Image y runs down: flip so the rings are in board coordinates.
        outer[:, 1] = h - outer[:, 1]
        if area(outer) < 0:
            outer = outer[::-1]
        holes = []
        k = hier[0][i][2]
        while k != -1:
            hole = cv2.approxPolyDP(contours[k], eps, True).reshape(-1, 2).astype(np.float64)
            if len(hole) >= 3 and abs(area(hole)) >= 12:
                hole[:, 1] = h - hole[:, 1]
                if area(hole) > 0:
                    hole = hole[::-1]
                holes.append(hole)
            k = hier[0][k][0]
        ring = outer
        for hole in sorted(holes, key=lambda r: -r[:, 0].max()):
            ring = merge_hole(ring, hole)
        verts += len(ring)
        tris += ear_clip(ring)
    flat = []
    for a, b, c in tris:
        for p in (a, b, c):
            flat += [round(p[0] / h, 4), round(p[1] / h, 4)]
    json.dump({"w": round(w / h, 4), "h": 1, "tris": flat}, open(out, "w"), separators=(",", ":"))
    # Check: paint the triangles and compare with the source.
    canvas = np.zeros((h, w), np.uint8)
    for a, b, c in tris:
        poly = np.array([[a[0], h - a[1]], [b[0], h - b[1]], [c[0], h - c[1]]], np.int32)
        cv2.fillConvexPoly(canvas, poly, 1)
    both = int((canvas & mask).sum())
    either = int((canvas | mask).sum())
    print(f"{out}: {len(tris)} triangles from {verts} points, overlap with the source {both / max(either, 1):.4f}")
    if "--check" in sys.argv:
        cv2.imwrite(sys.argv[sys.argv.index("--check") + 1], 255 - canvas * 255)


if __name__ == "__main__":
    main()
