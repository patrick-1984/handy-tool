"""Builds the README GIFs from capture.mjs frames.

python make_gifs.py <captures dir> <output dir> [story ...]

Each frame keeps its real timing; a caption strip on top names the step
("1  Press Ctrl+Space and speak"). One shared palette per GIF and no
dithering keep the frame-to-frame changes (and the files) small.
"""

import json
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFont

WIDTH = 800
STRIP = 46
BG = (20, 32, 43)
ACCENT = (41, 163, 200)
FONT = "C:/Windows/Fonts/segoeui.ttf"
FONT_BOLD = "C:/Windows/Fonts/segoeuib.ttf"
HOLD_LAST_MS = 1800


def strip(caption, step, width):
    img = Image.new("RGB", (width, STRIP), BG)
    if not caption:
        return img
    d = ImageDraw.Draw(img)
    bold = ImageFont.truetype(FONT_BOLD, 17)
    text = ImageFont.truetype(FONT, 18)
    d.ellipse((14, 10, 40, 36), fill=ACCENT)
    num = str(step)
    w = d.textlength(num, font=bold)
    d.text((27 - w / 2, 11), num, font=bold, fill=(255, 255, 255))
    d.text((52, 10), caption, font=text, fill=(235, 241, 245))
    return img


def build(src, dst):
    frames = json.load(open(os.path.join(src, "frames.json"), encoding="utf-8"))
    shots = [f for f in frames if not f.get("end")]
    steps = {}
    images, durations = [], []
    for i, f in enumerate(shots):
        cap = f["caption"]
        if cap and cap not in steps:
            steps[cap] = len(steps) + 1
        im = Image.open(os.path.join(src, f"f{i:04d}.png")).convert("RGB")
        h = round(im.height * WIDTH / im.width)
        im = im.resize((WIDTH, h), Image.LANCZOS)
        canvas = Image.new("RGB", (WIDTH, h + STRIP), BG)
        canvas.paste(strip(cap, steps.get(cap, 0), WIDTH), (0, 0))
        canvas.paste(im, (0, STRIP))
        dur = frames[i + 1]["t"] - f["t"]
        if images and ImageChops.difference(images[-1], canvas).getbbox() is None:
            durations[-1] += dur
            continue
        images.append(canvas)
        durations.append(dur)
    durations[-1] += HOLD_LAST_MS
    # One palette for the whole GIF, from a sample of its frames.
    sample = images[:: max(1, len(images) // 12)]
    sheet = Image.new("RGB", (WIDTH, sum(s.height for s in sample)))
    y = 0
    for s in sample:
        sheet.paste(s, (0, y))
        y += s.height
    pal = sheet.quantize(colors=128, method=Image.Quantize.MEDIANCUT)
    out = [im.quantize(palette=pal, dither=Image.Dither.NONE) for im in images]
    out[0].save(
        dst,
        save_all=True,
        append_images=out[1:],
        duration=[max(20, round(d)) for d in durations],
        loop=0,
        optimize=False,
        disposal=1,
    )
    print(f"{os.path.basename(dst)}: {len(out)} frames, "
          f"{sum(durations) / 1000:.1f} s, {os.path.getsize(dst) / 1e6:.2f} MB")


if __name__ == "__main__":
    src_root, dst_root = sys.argv[1], sys.argv[2]
    only = sys.argv[3:]
    os.makedirs(dst_root, exist_ok=True)
    for name in sorted(os.listdir(src_root)):
        if only and name not in only:
            continue
        if os.path.exists(os.path.join(src_root, name, "frames.json")):
            build(os.path.join(src_root, name), os.path.join(dst_root, f"{name}.gif"))
