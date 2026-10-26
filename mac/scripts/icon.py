#!/usr/bin/env python3
"""Writes the app's first icon into JawApp/Resources/Assets.xcassets/AppIcon.appiconset: the Mimic at
whole multiples of its 45 by 26 pixels, never smoothed, on Tokyo Night's ground in the shape of a
macOS icon. Icon Composer's layered icon replaces it in Mac-M5.

    python3 mac/scripts/icon.py
"""

import json
from pathlib import Path

from PIL import Image, ImageDraw

MAC = Path(__file__).resolve().parents[1]
MIMIC = MAC.parent / "creatures" / "mimic.png"
OUT = MAC / "JawApp" / "Resources" / "Assets.xcassets" / "AppIcon.appiconset"
GROUND = (0x1A, 0x1B, 0x26, 255)
EDGE = (0x3B, 0x42, 0x61, 255)
SIZES = [16, 32, 64, 128, 256, 512, 1024]


def icon(size: int, mimic: Image.Image) -> Image.Image:
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    # Apple's grid: the body is 824 of 1024, its corners about 185.
    inset = round(size * 100 / 1024)
    radius = round(size * 185 / 1024)
    body = [inset, inset, size - inset - 1, size - inset - 1]
    ImageDraw.Draw(canvas).rounded_rectangle(body, radius=radius, fill=GROUND, outline=EDGE, width=max(1, size // 256))
    room = size - 2 * inset
    scale = max(1, int(room * 0.72 // mimic.width))
    w, h = mimic.width * scale, mimic.height * scale
    if w > room:
        scale = room * 0.8 / mimic.width
        w, h = max(1, round(mimic.width * scale)), max(1, round(mimic.height * scale))
    sprite = mimic.resize((w, h), Image.NEAREST)
    canvas.alpha_composite(sprite, ((size - w) // 2, (size - h) // 2 + round(size * 0.02)))
    return canvas


def main() -> None:
    mimic = Image.open(MIMIC).convert("RGBA")
    OUT.mkdir(parents=True, exist_ok=True)
    images = []
    for size in [16, 32, 128, 256, 512]:
        for scale in (1, 2):
            px = size * scale
            name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
            icon(px, mimic).save(OUT / name)
            images.append({"filename": name, "idiom": "mac", "scale": f"{scale}x", "size": f"{size}x{size}"})
    (OUT / "Contents.json").write_text(json.dumps({"images": images, "info": {"author": "xcode", "version": 1}}, indent=2) + "\n")
    (OUT.parent / "Contents.json").write_text(json.dumps({"info": {"author": "xcode", "version": 1}}, indent=2) + "\n")
    print(f"{OUT.relative_to(MAC.parent)}: {len(images)} images")


if __name__ == "__main__":
    main()
