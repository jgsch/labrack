# app: window, header and style

- Source: roadmap Phase 1

## Why

The Dioxus app needs a window, navigation and the mockup's look before any page can be built.

## Requirements

No spec change; implements:

- `ARCH-1`, `ARCH-2`: `crates/app` (Dioxus 0.7 desktop), mockup CSS reused, markup in RSX.
- `UX-1` to `UX-10`: tabs, header, focus outline, Catppuccin palette, fonts, layout.
- `UX-27`: hover brightening.
- `PERF-1`: runs on Wayland and X11, x86_64 and aarch64.
- `PERF-4`: fonts and assets bundled, works offline.

## Tasks

- [ ] Create `crates/app` and wire it to core (state in, intents out)
- [ ] Import the mockup CSS and fonts (from `mockup-tooling`)
- [ ] Header, three tabs, MUTE ALL slot, keyboard focus
- [ ] App screenshot task (mise) and comparison with the mockup header
- [ ] `mise run build` builds the app; add a `run` task

## Done when

- [ ] Header and tabs match the mockup board screenshots
- [ ] The app starts with the network down and shows the bundled fonts

## Notes

- Depends on `mockup-tooling` and `core-amp-list`.
