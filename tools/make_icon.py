"""Draws the Sawasdee app icon (gold square, indigo elephant) and writes the PNGs.

Run:  python3 tools/make_icon.py        (needs Pillow: pip install pillow)
Writes assets/icon/icon-1024.png (full square, for iOS) and
android/res/mipmap-*/ic_launcher.png (rounded corners, for Android).
"""
import math
from PIL import Image, ImageDraw

GOLD, EAR, HEAD = (255, 201, 60), (107, 124, 196), (74, 91, 168)
EYE, WHITE = (27, 27, 58), (255, 255, 255)
BIG = 4096  # draw large, then shrink for smooth edges


def ellipse_poly(cx, cy, rx, ry, deg, n=48):
    a = math.radians(deg)
    pts = []
    for i in range(n):
        t = 2 * math.pi * i / n
        x, y = rx * math.cos(t), ry * math.sin(t)
        pts.append((cx + x * math.cos(a) - y * math.sin(a), cy + x * math.sin(a) + y * math.cos(a)))
    return pts


def bezier(p0, p1, p2, n=40):
    return [((1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * p1[0] + t * t * p2[0],
             (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * p1[1] + t * t * p2[1]) for t in (i / n for i in range(n + 1))]


def master():
    f = BIG / 100.0
    im = Image.new("RGB", (BIG, BIG), GOLD)
    d = ImageDraw.Draw(im)

    def circle(cx, cy, r, c):
        d.ellipse([(cx - r) * f, (cy - r) * f, (cx + r) * f, (cy + r) * f], fill=c)

    circle(24, 48, 17, EAR)
    circle(76, 48, 17, EAR)
    circle(50, 48, 26, HEAD)
    trunk = bezier((50, 62), (44, 86), (54, 86)) + bezier((54, 86), (62, 86), (60, 76))
    for x, y in trunk:
        circle(x, y, 6, HEAD)
    circle(54, 75, 5, HEAD)  # fill the small gap inside the trunk curl
    d.polygon([(x * f, y * f) for x, y in ellipse_poly(34, 68, 3.5, 8, 14)], fill=WHITE)
    d.polygon([(x * f, y * f) for x, y in ellipse_poly(66, 68, 3.5, 8, -14)], fill=WHITE)
    circle(41, 44, 3.5, EYE)
    circle(59, 44, 3.5, EYE)
    return im


def rounded(im, radius_ratio=0.22):
    s = im.size[0]
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, s - 1, s - 1], radius=int(s * radius_ratio), fill=255)
    out = im.convert("RGBA")
    out.putalpha(mask)
    return out


if __name__ == "__main__":
    big = master()
    big.resize((1024, 1024), Image.LANCZOS).save("assets/icon/icon-1024.png")
    for name, px in [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)]:
        rounded(big).resize((px, px), Image.LANCZOS).save(f"android/res/mipmap-{name}/ic_launcher.png")
    print("ok")
