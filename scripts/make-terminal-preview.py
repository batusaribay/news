#!/usr/bin/env python3
"""Render a plain terminal-style preview PNG from a text transcript.

Usage:
    make-terminal-preview.py INPUT.txt OUTPUT.png

Transcript conventions:
    $ command      green prompt, normal text
    ## heading     white bold
    .. muted       muted gray
    anything else  normal light gray

The canvas is always 1200x500, black, square, and uses Liberation Mono at 18px
with a 26px line height. Lines are limited to 17.
"""

from __future__ import annotations

import html
import subprocess
import sys
import tempfile
from pathlib import Path

WIDTH = 1200
HEIGHT = 500
LEFT = 34
TOP = 48
LINE_HEIGHT = 26
FONT_SIZE = 18
FONT_FAMILY = '"Liberation Mono", "DejaVu Sans Mono", monospace'
MAX_LINES = 17
COLOR_TEXT = "#d8d8d8"
COLOR_MUTED = "#8a8a8a"
COLOR_HEADER = "#ffffff"
COLOR_PROMPT = "#8ae234"

SVG_HEAD = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
  <style>
    text {{ font-family: {FONT_FAMILY}; font-size: {FONT_SIZE}px; fill: {COLOR_TEXT}; }}
    .muted {{ fill: {COLOR_MUTED}; }}
    .header {{ fill: {COLOR_HEADER}; font-weight: bold; }}
    .prompt {{ fill: {COLOR_PROMPT}; }}
  </style>
  <rect width="{WIDTH}" height="{HEIGHT}" fill="#0c0c0c"/>
  <g xml:space="preserve">
'''
SVG_TAIL = '''  </g>
</svg>
'''


def text_line(index: int, value: str) -> str:
    y = TOP + index * LINE_HEIGHT
    if value == "$":
        return (
            f'    <text x="{LEFT}" y="{y}" xml:space="preserve">'
            f'<tspan class="prompt">$ </tspan></text>\n'
        )
    if value.startswith("$ "):
        command = html.escape(value[2:], quote=False)
        return (
            f'    <text x="{LEFT}" y="{y}" xml:space="preserve">'
            f'<tspan class="prompt">$ </tspan>{command}</text>\n'
        )
    if value.startswith("## "):
        heading = html.escape(value[3:], quote=False)
        return f'    <text x="{LEFT}" y="{y}" class="header">{heading}</text>\n'
    if value.startswith(".. "):
        muted = html.escape(value[3:], quote=False)
        return f'    <text x="{LEFT}" y="{y}" class="muted">{muted}</text>\n'
    return (
        f'    <text x="{LEFT}" y="{y}" xml:space="preserve">'
        f'{html.escape(value, quote=False)}</text>\n'
    )


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2

    source = Path(sys.argv[1])
    output = Path(sys.argv[2])
    lines = source.read_text(encoding="utf-8").splitlines()
    if not lines:
        print("input transcript is empty", file=sys.stderr)
        return 1
    if len(lines) > MAX_LINES:
        print(f"input has {len(lines)} lines; maximum is {MAX_LINES}", file=sys.stderr)
        return 1

    body = "".join(text_line(index, line) for index, line in enumerate(lines))
    if lines[-1] in ("$", "$ "):
        cursor_y = TOP + (len(lines) - 1) * LINE_HEIGHT - 15
        cursor_x = LEFT + 2 * 10.84
        body += (
            f'    <rect x="{cursor_x:.1f}" y="{cursor_y}" '
            'width="10" height="19" fill="#d8d8d8"/>\n'
        )

    svg = SVG_HEAD + body + SVG_TAIL
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", suffix=".svg", delete=False) as handle:
        handle.write(svg)
        temp = Path(handle.name)

    try:
        subprocess.run(["magick", str(temp), str(output)], check=True)
    except FileNotFoundError:
        print("ImageMagick's `magick` command was not found", file=sys.stderr)
        return 1
    finally:
        temp.unlink(missing_ok=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
