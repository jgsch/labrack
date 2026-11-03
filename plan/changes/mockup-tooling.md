# Mockup tooling

- Source: roadmap Phase 1

## Why

`spec/mockup.html` is a self-unpacking bundle (template, CSS and fonts in `__bundler` scripts).
The app reuses its CSS and fonts, and the UI is checked against screenshots of its boards, but
there is no tooling to render, unpack or repack it.

## Requirements

No spec change; implements:

- `ARCH-9`: `scripts/mockup.py` (Python 3, from the Nix shell).
- `ARCH-10`: `mockup`, `mockup-unpack` and `mockup-pack` tasks in `.mise.toml`.
- `ARCH-7`: headless Chromium screenshots of each board, compared with ImageMagick.
- `UX-9`, `PERF-4`: the Barlow Condensed and IBM Plex Mono files (with their OFL licences) are
  extracted for the app to bundle.

## Tasks

- [ ] `scripts/mockup.py unpack`: extract template, CSS and fonts into a source directory
- [ ] `scripts/mockup.py pack`: rebuild `spec/mockup.html` from it (renders the same)
- [ ] `mise run mockup`: render each board to PNG with headless Chromium
- [ ] Compare helper (ImageMagick `compare`) with a threshold, reused by the app screenshot tests
- [ ] Tasks run through `nix develop -c` like the others

## Done when

- [ ] `mise run mockup` produces one PNG per board
- [ ] Unpack then pack gives a mockup that renders the same

## Notes

- Decide where unpacked sources live (e.g. `spec/mockup/`) and whether they are committed or
  generated; the packed `spec/mockup.html` stays the reference.
- Rendered PNGs are outputs; keep them out of git.
