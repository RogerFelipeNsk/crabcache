"""Generates the README benchmark charts (docs/img/*.svg) as static SVG with light/dark themes.

Usage: python3 scripts/gen-charts.py docs/img   (update the numbers below from docs/BENCHMARKS.md)

Small multiples: one panel per scenario, each with its own zero-based scale, horizontal bars with the
value at the tip. Redis is blue and CrabCache orange; the pair was checked for color-vision-deficiency
separation and >= 3:1 contrast against both chart surfaces.
"""
import sys
from pathlib import Path

OUT = Path(sys.argv[1])

STYLE = """
<style>
  .surface { fill: #fcfcfb; }
  .ring { fill: none; stroke: rgba(11,11,11,0.10); }
  .title { fill: #0b0b0b; font-size: 16px; font-weight: 600; }
  .subtitle { fill: #52514e; font-size: 12.5px; }
  .panel { fill: #0b0b0b; font-size: 13px; font-weight: 600; }
  .label { fill: #52514e; font-size: 12.5px; }
  .value { fill: #0b0b0b; font-size: 12.5px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .delta { fill: #006300; font-size: 12.5px; font-weight: 600; }
  .axis { stroke: #c3c2b7; stroke-width: 1; }
  .s-redis { fill: #2a78d6; }
  .s-crab { fill: #eb6834; }
  .note { fill: #898781; font-size: 11px; }
  @media (prefers-color-scheme: dark) {
    .surface { fill: #1a1a19; }
    .ring { stroke: rgba(255,255,255,0.10); }
    .title, .panel, .value { fill: #ffffff; }
    .subtitle, .label { fill: #c3c2b7; }
    .delta { fill: #0ca30c; }
    .axis { stroke: #383835; }
    .s-redis { fill: #3987e5; }
    .s-crab { fill: #d95926; }
  }
  text { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }
</style>
"""

W = 720
PAD = 24
LABEL_W = 100
VALUE_W = 150
BAR_H = 20
GAP = 2  # surface gap between adjacent bars
PANEL_H = 30 + 2 * BAR_H + GAP + 18


def bar(x, y, w, cls):
    """Bar square at the baseline, 4px rounded data end."""
    r = min(4, w / 2)
    return (
        f'<path class="{cls}" d="M{x:.1f},{y:.1f} H{x + w - r:.1f} '
        f"Q{x + w:.1f},{y:.1f} {x + w:.1f},{y + r:.1f} V{y + BAR_H - r:.1f} "
        f'Q{x + w:.1f},{y + BAR_H:.1f} {x + w - r:.1f},{y + BAR_H:.1f} H{x:.1f} Z"/>'
    )


def chart(name, title, subtitle, panels, fmt, delta_fmt, note):
    """panels: list of (panel title, redis value, crabcache value)."""
    head = 70
    h = head + len(panels) * PANEL_H + 34
    plot_x = PAD + LABEL_W
    plot_w = W - plot_x - VALUE_W - PAD
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{h}" viewBox="0 0 {W} {h}" '
        f'role="img" aria-labelledby="t d">',
        f"<title id=\"t\">{title}</title>",
        f"<desc id=\"d\">{subtitle}. "
        + "; ".join(f"{p}: Redis {fmt(r)}, CrabCache {fmt(c)}" for p, r, c in panels)
        + "</desc>",
        STYLE,
        f'<rect class="surface" x="0.5" y="0.5" width="{W - 1}" height="{h - 1}" rx="10"/>',
        f'<rect class="ring" x="0.5" y="0.5" width="{W - 1}" height="{h - 1}" rx="10"/>',
        f'<text class="title" x="{PAD}" y="32">{title}</text>',
        f'<text class="subtitle" x="{PAD}" y="52">{subtitle}</text>',
    ]
    # Legend (two series): swatch + name, top right.
    lx = W - PAD - 220
    for i, (cls, name_) in enumerate((("s-redis", "Redis 8.10"), ("s-crab", "CrabCache 0.2"))):
        x = lx + i * 104
        parts.append(f'<rect class="{cls}" x="{x}" y="22" width="12" height="12" rx="3"/>')
        parts.append(f'<text class="label" x="{x + 18}" y="32">{name_}</text>')

    y = head
    for ptitle, redis, crab in panels:
        scale = plot_w / max(redis, crab)
        parts.append(f'<text class="panel" x="{PAD}" y="{y + 16}">{ptitle}</text>')
        by = y + 28
        for i, (label, val, cls) in enumerate((("Redis", redis, "s-redis"), ("CrabCache", crab, "s-crab"))):
            yy = by + i * (BAR_H + GAP)
            w = max(val * scale, 2)
            parts.append(f'<text class="label" x="{plot_x - 10}" y="{yy + 14.5}" text-anchor="end">{label}</text>')
            parts.append(bar(plot_x, yy, w, cls))
            txt = f'<text class="value" x="{plot_x + w + 8:.1f}" y="{yy + 14.5}">{fmt(val)}'
            if cls == "s-crab":
                txt += f'<tspan class="delta" dx="8">{delta_fmt(redis, crab)}</tspan>'
            parts.append(txt + "</text>")
        parts.append(
            f'<line class="axis" x1="{plot_x}" y1="{by - 4}" x2="{plot_x}" y2="{by + 2 * BAR_H + GAP + 4}"/>'
        )
        y += PANEL_H
    parts.append(f'<text class="note" x="{PAD}" y="{h - 16}">{note}</text>')
    parts.append("</svg>")
    (OUT / name).write_text("\n".join(parts) + "\n")


def ops(v):
    return f"{v / 1e6:.2f}M ops/s" if v >= 1e6 else f"{v / 1e3:.0f}k ops/s"


def more(r, c):
    return f"+{(c / r - 1) * 100:.0f}%"


def less(r, c):
    return f"−{(1 - c / r) * 100:.0f}%"


OUT.mkdir(parents=True, exist_ok=True)
chart(
    "bench-per-core.svg",
    "Throughput com o mesmo núcleo",
    "Ambos limitados a ~1 núcleo de CPU (medido) · memtier_benchmark, 48 conexões, 90% GET, valores de 100 B",
    [("Sem pipeline", 112_212, 136_121), ("Pipeline de 16 comandos", 923_529, 1_711_115)],
    ops,
    more,
    "Apple M1 Pro, macOS, loopback · CrabCache com --threads 1 · scripts/bench-1cpu.sh",
)
chart(
    "bench-memory.svg",
    "Memória por chave",
    "1M de chaves (300k para 1 KB) carregadas via redis-cli --pipe · memória física (footprint)",
    [("Valores de 10 B", 86, 63), ("Valores de 100 B", 184, 159), ("Valores de 1 KB", 1110, 1066)],
    lambda v: f"{v:,} B".replace(",", "."),
    less,
    "Apple M1 Pro, macOS · menor é melhor · scripts/bench-memory.sh",
)
print("ok")
