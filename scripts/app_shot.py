#!/usr/bin/env python3
"""Screenshots of the app's pages and their comparison with the mockup boards (ARCH-7).

`cargo run -p labrack-app --example render` writes each page as an HTML file with the app's own
stylesheet, Chromium takes the screenshots (1200 px wide, like the boards), and the top strip of each page
is compared, pixel by pixel, with the same strip of the matching mockup board. The result is in
target/app/: `<page>.png`, `<page>-strip.png` and `<page>-diff.png`. Needs the Nix shell (Chromium,
ImageMagick, the Rust toolchain).
"""

import argparse
import subprocess
import sys
from pathlib import Path

import mockup

ROOT = mockup.ROOT
OUT = ROOT / "target" / "app"
WIDTH, HEIGHT = 1200, 900

# the page of the app, the board of the mockup that shows it, and how much of the top to compare
PAGES = {
    "amplifiers": "01-a-rack-amplifiers",
    "mix-eq": "02-mix-eq-mixer-tab",
    "search": "06-search-add-amplifiers",
    "levels-demo": "03-mix-eq-levels-tab",
    "eq-in-demo": "04-mix-eq-inputs-eq-tab",
    "eq-out-demo": "05-mix-eq-outputs-eq-filters-delay-tab",
    "mixer-demo": "02-mix-eq-mixer-tab",
}
HEADER = 100


def render_pages(out: Path) -> None:
    cmd = ["cargo", "run", "-q", "-p", "labrack-app", "--example", "render", "--", str(out)]
    run = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if run.returncode != 0:
        raise mockup.Error(f"cannot render the pages: {run.stderr.strip()[-600:]}")


def strip(png: Path, out: Path, height: int) -> Path:
    mockup.magick(str(png), "-crop", f"{WIDTH}x{height}+0+0", "+repage", str(out))
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", type=Path, default=OUT)
    ap.add_argument("--height", type=int, default=HEADER, help="how many pixels from the top to compare")
    ap.add_argument("--fuzz", default="1%", help="colour distance that still counts as equal")
    ap.add_argument("--max-ratio", type=float, default=0.0, help="allowed share of differing pixels")
    args = ap.parse_args(argv)

    try:
        pages = args.out / "pages"
        render_pages(pages)
        boards = mockup.render(mockup.read_tree(mockup.SRC), args.out / "mockup", list(PAGES.values()))
        board_png = {path.stem: path for path in boards}
        browser = mockup.chromium()
        failed = False
        for page, board in PAGES.items():
            png = args.out / f"{page}.png"
            png.unlink(missing_ok=True)
            mockup.shoot(browser, pages / f"{page}.html", png, WIDTH, HEIGHT)
            ours = strip(png, args.out / f"{page}-strip.png", args.height)
            theirs = strip(board_png[board], args.out / f"{page}-mockup-strip.png", args.height)
            d = mockup.compare(ours, theirs, args.fuzz, args.out / f"{page}-diff.png")
            ok = d.within(args.max_ratio)
            failed |= not ok
            print(f"{page:11} {d.differing:6} of {d.total} pixels differ ({d.ratio:.4%}) {'ok' if ok else 'FAIL'}")
        return 1 if failed else 0
    except (mockup.Error, OSError, subprocess.SubprocessError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
