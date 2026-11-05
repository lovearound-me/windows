"""The app icon, 1024 px: a rounded square in the Mac icon's coral-to-rose
gradient with a white heart. `cargo tauri icon` makes the other sizes from it."""
import sys

from PIL import Image, ImageDraw

S = 1024
top, bottom = (0.96, 0.52, 0.47), (0.85, 0.30, 0.43)
grad = Image.new("RGBA", (S, S))
px = grad.load()
for y in range(S):
    t = y / (S - 1)
    c = tuple(int(255 * (a + (b - a) * t)) for a, b in zip(top, bottom))
    for x in range(S):
        px[x, y] = c + (255,)

mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle((40, 40, S - 40, S - 40), radius=210, fill=255)
icon = Image.new("RGBA", (S, S), (0, 0, 0, 0))
icon.paste(grad, (0, 0), mask)


def bezier(p0, p1, p2, p3, n=60):
    return [
        (
            (1 - t) ** 3 * p0[0] + 3 * (1 - t) ** 2 * t * p1[0] + 3 * (1 - t) * t * t * p2[0] + t ** 3 * p3[0],
            (1 - t) ** 3 * p0[1] + 3 * (1 - t) ** 2 * t * p1[1] + 3 * (1 - t) * t * t * p2[1] + t ** 3 * p3[1],
        )
        for t in (i / n for i in range(n + 1))
    ]


def p(x, y):
    return (S * (0.22 + x * 0.56), S * (0.25 + y * 0.56))


pts = []
start = (0.5, 0.92)
for a, b, c in [
    ((0.12, 0.62), (-0.04, 0.38), (0.1, 0.18)),
    ((0.24, 0.0), (0.44, 0.04), (0.5, 0.24)),
    ((0.56, 0.04), (0.76, 0.0), (0.9, 0.18)),
    ((1.04, 0.38), (0.88, 0.62), (0.5, 0.92)),
]:
    pts += bezier(p(*start), p(*a), p(*b), p(*c))
    start = c

# Drawn 4x larger and scaled down, for smooth edges.
big = Image.new("L", (S * 4, S * 4), 0)
ImageDraw.Draw(big).polygon([(x * 4, y * 4) for x, y in pts], fill=255)
heart = big.resize((S, S), Image.LANCZOS)
white = Image.new("RGBA", (S, S), (255, 250, 245, 255))
icon.paste(white, (0, 0), heart)
icon.save(sys.argv[1])
