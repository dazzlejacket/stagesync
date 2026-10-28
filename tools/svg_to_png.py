#!/usr/bin/env python3
"""Rasterise StageSync's drift-graph SVG to a PNG.

The application's artefact is `drift_graph.svg`, written by the Rust exporter in
`crates/stagesync-core/src/export/graph.rs`.  This script turns that file into a PNG, so
the graph can be dropped into a document or a chat without needing an SVG rasteriser to
be installed.

It is deliberately *not* a general SVG renderer.  It understands exactly the small,
fixed subset of SVG that the exporter emits -- `rect`, `line`, `circle`, `path` and
`text`, with the `transform="rotate(-90 ...)"` the vertical axis titles use -- and it
reads the shipped SVG itself rather than the intermediate CSV, so the picture is of the
artefact and not of a re-derivation of it.

Only Pillow is needed.  If a proper SVG rasteriser (cairosvg, rsvg-convert, Inkscape)
is available it will usually look better; use this when it is not.

Usage
-----
    python tools/svg_to_png.py <drift_graph.svg> <out.png> [scale]

`scale` multiplies the output resolution; the exporter's graph is 1600x900, so 2.0
gives a 3200x1800 PNG.
"""
import re
import sys

from PIL import Image, ImageDraw, ImageFont

ATTR = re.compile(r'([a-zA-Z][a-zA-Z0-9-]*)="([^"]*)"')
TAG = re.compile(r"<(?:rect|line|circle|path|text)\b[^>]*>(?:[^<]*</text>)?")
NUMBER = re.compile(r"-?\d+\.?\d*")


def attrs(tag: str) -> dict:
    """Every `name="value"` attribute of one element."""
    return dict(ATTR.findall(tag))


def font(size: float) -> ImageFont.FreeTypeFont:
    """A font at the requested size, falling back to whatever is installed."""
    for name in ("segoeui.ttf", "arial.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, int(round(size)))
        except OSError:
            continue
    return ImageFont.load_default()


def colour(value: str, default=(0, 0, 0)):
    """An `#rrggbb` colour as an RGB tuple, or None for 'none'."""
    value = value.strip()
    if value in ("none", ""):
        return None
    if value.startswith("#") and len(value) == 7:
        return tuple(int(value[i:i + 2], 16) for i in (1, 3, 5))
    return default


def main() -> None:
    if len(sys.argv) < 3:
        print(__doc__)
        return
    svg_path, out_path = sys.argv[1], sys.argv[2]
    scale = float(sys.argv[3]) if len(sys.argv) > 3 else 1.0

    doc = open(svg_path, encoding="utf-8").read()
    head = attrs(re.search(r"<svg[^>]*>", doc).group(0))
    width, height = float(head["width"]), float(head["height"])

    img = Image.new("RGB", (int(width * scale), int(height * scale)), "white")
    draw = ImageDraw.Draw(img)

    def S(value: str) -> float:
        """A coordinate in the SVG's own units, scaled to the output."""
        return float(value) * scale

    for tag in TAG.findall(doc):
        name = tag[1:].split()[0].rstrip("/>")
        a = attrs(tag)

        if name == "rect":
            x, y = S(a.get("x", 0)), S(a.get("y", 0))
            w, h = S(a.get("width", 0)), S(a.get("height", 0))
            fill = colour(a.get("fill", "#ffffff"))
            stroke = colour(a.get("stroke", "")) if "stroke" in a else None
            if w > 0 and h > 0 and (fill or stroke):
                draw.rectangle([x, y, x + w, y + h], fill=fill, outline=stroke,
                               width=1 if stroke else 0)

        elif name == "line":
            draw.line([S(a["x1"]), S(a["y1"]), S(a["x2"]), S(a["y2"])],
                      fill=colour(a.get("stroke", "#000000")),
                      width=max(1, int(round(S(a.get("stroke-width", 1))))))

        elif name == "circle":
            cx, cy, r = S(a["cx"]), S(a["cy"]), S(a["r"])
            draw.ellipse([cx - r, cy - r, cx + r, cy + r],
                         fill=colour(a.get("fill", "#000000")))

        elif name == "path":
            values = [float(v) for v in NUMBER.findall(a.get("d", ""))]
            points = list(zip(values[0::2], values[1::2]))
            if len(points) >= 2:
                draw.line([(S(px), S(py)) for px, py in points],
                          fill=colour(a.get("stroke", "#000000")),
                          width=max(1, int(round(S(a.get("stroke-width", 1))))))

        elif name == "text":
            content = re.search(r">([^<]*)</text>", tag)
            if not content:
                continue
            text = (content.group(1).replace("&amp;", "&")
                    .replace("&lt;", "<").replace("&gt;", ">"))
            x, y = S(a["x"]), S(a["y"])
            size = S(a.get("font-size", 12))
            face = font(size)
            ink = colour(a.get("fill", "#000000"))

            rotation = re.search(r"rotate\((-?[\d.]+)\s+([\d.]+)\s+([\d.]+)\)",
                                 a.get("transform", ""))
            if rotation:
                # A vertical axis title.  Draw it flat on its own layer, rotate it,
                # and paste it back about its anchor point.
                layer = Image.new("RGBA", (int(size * len(text)) + 8, int(size * 2) + 8),
                                  (255, 255, 255, 0))
                ImageDraw.Draw(layer).text((4, 4), text, font=face, fill=ink)
                layer = layer.rotate(-float(rotation.group(1)), expand=True)
                cx, cy = S(rotation.group(2)), S(rotation.group(3))
                img.paste(layer,
                          (int(cx - layer.width / 2), int(cy - layer.height / 2)), layer)
                continue

            width_px = draw.textlength(text, font=face)
            anchor = a.get("text-anchor", "start")
            if anchor == "middle":
                x -= width_px / 2
            elif anchor == "end":
                x -= width_px
            draw.text((x, y - size), text, font=face, fill=ink)

    img.save(out_path)
    print(f"wrote {out_path} ({img.width}x{img.height})")


if __name__ == "__main__":
    main()
