#!/usr/bin/env python3
"""Generates assets/icon.ico (+ assets/icon.png preview): a purple squircle with a white clock face.
Pure standard library; every size is drawn from signed-distance functions, so it stays crisp at 16 px.
Run from the repo root:  python tools/make_icon.py
"""
import math
import struct
import zlib

SIZES = [16, 24, 32, 48, 64, 128, 256]


def smooth(a, b, x):
    t = min(1.0, max(0.0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)


def sd_round_box(px, py, hx, hy, r):
    qx, qy = abs(px) - hx + r, abs(py) - hy + r
    return math.hypot(max(qx, 0.0), max(qy, 0.0)) + min(max(qx, qy), 0.0) - r


def sd_segment(px, py, ax, ay, bx, by):
    pax, pay, bax, bay = px - ax, py - ay, bx - ax, by - ay
    h = min(1.0, max(0.0, (pax * bax + pay * bay) / (bax * bax + bay * bay)))
    return math.hypot(pax - bax * h, pay - bay * h)


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def over(dst, src, alpha):
    return tuple(dst[i] * (1 - alpha) + src[i] * alpha for i in range(3))


def shade(u, v, aa):
    """Colour + alpha at (u, v) in 0..1 icon space; aa = one pixel in icon units."""
    x, y = u - 0.5, v - 0.5
    d_box = sd_round_box(x, y, 0.47, 0.47, 0.22)
    a_box = 1.0 - smooth(-aa * 0.5, aa * 0.5, d_box)
    if a_box <= 0.0:
        return (0, 0, 0, 0.0)
    # violet diagonal gradient, soft top-left glow
    t = min(1.0, max(0.0, (x + y) * 0.75 + 0.5))
    col = mix((0.72, 0.48, 1.00), (0.33, 0.10, 0.62), t)
    glow = math.exp(-((x + 0.25) ** 2 + (y + 0.30) ** 2) * 9.0) * 0.28
    col = tuple(min(1.0, c + glow) for c in col)
    # clock face
    cx, cy = 0.0, 0.015
    d_face = math.hypot(x - cx, y - cy) - 0.30
    shadow = (1.0 - smooth(-0.01, 0.07, d_face)) * 0.22
    col = over(col, (0.12, 0.0, 0.30), shadow * (1 if d_face > 0 else 0))
    a_face = 1.0 - smooth(-aa * 0.5, aa * 0.5, d_face)
    face = mix((1.0, 1.0, 1.0), (0.90, 0.84, 1.0), min(1.0, max(0.0, (y - cy + 0.3))))
    col = over(col, face, a_face)
    ink = (0.30, 0.09, 0.58)
    # ticks at 12/3/6/9 (skipped at tiny sizes where they turn to mush)
    if aa < 0.045:
        for k in range(4):
            ang = k * math.pi / 2
            tx, ty = math.sin(ang), -math.cos(ang)
            d = sd_segment(x - cx, y - cy, tx * 0.235, ty * 0.235, tx * 0.265, ty * 0.265) - 0.008
            col = over(col, ink, (1.0 - smooth(-aa * 0.5, aa * 0.5, d)) * a_face)
    # hands: hour toward 10, minute toward 2
    for (ang, ln, w) in ((math.radians(-60), 0.145, 0.026), (math.radians(60), 0.215, 0.020)):
        hx, hy = math.sin(ang) * ln, -math.cos(ang) * ln
        d = sd_segment(x - cx, y - cy, 0, 0, hx, hy) - w
        col = over(col, ink, (1.0 - smooth(-aa * 0.5, aa * 0.5, d)) * a_face)
    d_dot = math.hypot(x - cx, y - cy) - 0.032
    col = over(col, ink, (1.0 - smooth(-aa * 0.5, aa * 0.5, d_dot)) * a_face)
    # thin light rim on the squircle
    rim = (1.0 - smooth(0.0, aa * 1.4, abs(d_box + aa * 0.7))) * 0.25
    col = over(col, (1, 1, 1), rim)
    return (col[0], col[1], col[2], a_box)


def render(size, ss=3):
    aa = 1.0 / size
    rows = []
    for py in range(size):
        row = bytearray()
        for px in range(size):
            r = g = b = a = 0.0
            for sy in range(ss):
                for sx in range(ss):
                    cr, cg, cb, ca = shade((px + (sx + 0.5) / ss) / size, (py + (sy + 0.5) / ss) / size, aa)
                    r, g, b, a = r + cr * ca, g + cg * ca, b + cb * ca, a + ca
            n = ss * ss
            if a > 0:
                row += bytes((round(255 * r / a), round(255 * g / a), round(255 * b / a), round(255 * a / n)))
            else:
                row += b"\0\0\0\0"
        rows.append(bytes(row))
    return rows


def png(size, rows):
    def chunk(kind, data):
        c = struct.pack(">I", len(data)) + kind + data
        return c + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    raw = b"".join(b"\0" + r for r in rows)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def main():
    images = []
    for s in SIZES:
        data = png(s, render(s))
        images.append((s, data))
        if s == 256:
            open("assets/icon.png", "wb").write(data)
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for s, data in images:
        out += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    out += b"".join(d for _, d in images)
    open("assets/icon.ico", "wb").write(out)
    print("wrote assets/icon.ico", len(out), "bytes")


if __name__ == "__main__":
    main()
