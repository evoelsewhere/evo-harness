"""Draw the Evo Mates app icon (docs/features/evo-mates/tasks.md, T6).

Same frame as Evo's icon (a dark rounded square); inside, two mates: a blue
one behind and a white one in front, each a soft body with two capsule eyes.

Run from apps/desktop:  python -I scripts/mates-icons.py
Writes src-tauri/icons/mates/{32x32,64x64,128x128,256x256,512x512}.png,
src-tauri/icons/mates.ico and src-tauri/icons/mates.icns.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw

SIZE = 1024
SCALE = 4  # draw large, then shrink: smooth edges
BG = (23, 23, 23, 255)
WHITE = (255, 255, 255, 255)
BLUE = (0, 114, 245, 255)


def capsule(draw, cx, cy, w, h, angle_dx, fill):
    """A capsule eye centred at (cx, cy), w x h, leaning by angle_dx px."""
    r = w / 2
    draw.rounded_rectangle([cx - w / 2 + angle_dx, cy - h / 2, cx + w / 2 + angle_dx, cy + h / 2], radius=r, fill=fill)


def mate(img, cx, cy, radius, body, eye, gaze):
    d = ImageDraw.Draw(img)
    d.ellipse([cx - radius, cy - radius, cx + radius, cy + radius], fill=body)
    ew, eh = radius * 0.2, radius * 0.42
    ex = cx + gaze * radius
    ey = cy - radius * 0.12
    capsule(d, ex - radius * 0.2, ey, ew, eh, 0, eye)
    capsule(d, ex + radius * 0.2, ey, ew * 0.92, eh * 0.92, 0, eye)


def draw() -> Image.Image:
    s = SIZE * SCALE
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    m = 64 * SCALE
    d.rounded_rectangle([m, m, s - m, s - m], radius=200 * SCALE, fill=BG)
    # The blue mate behind, up and to the right, its eyes clear of the white one.
    mate(img, 0.66 * s, 0.38 * s, 0.2 * s, BLUE, BG, 0.12)
    # A dark ring cuts the blue one where the white one sits in front.
    d.ellipse([0.42 * s - 0.27 * s, 0.6 * s - 0.27 * s, 0.42 * s + 0.27 * s, 0.6 * s + 0.27 * s], fill=BG)
    mate(img, 0.42 * s, 0.6 * s, 0.24 * s, WHITE, BG, 0.16)
    return img.resize((SIZE, SIZE), Image.LANCZOS)


def main(root: Path) -> None:
    icons = root / "src-tauri" / "icons"
    out = icons / "mates"
    out.mkdir(parents=True, exist_ok=True)
    big = draw()
    for size in (32, 64, 128, 256, 512):
        big.resize((size, size), Image.LANCZOS).save(out / f"{size}x{size}.png")
    big.save(icons / "mates.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    big.save(icons / "mates.icns")
    print(f"wrote {out} and mates.ico / mates.icns")


if __name__ == "__main__":
    main(Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd())
