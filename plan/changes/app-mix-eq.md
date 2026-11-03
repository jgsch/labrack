# app: Mix & EQ pages

- Source: roadmap Phase 1

## Why

The per-amp DSP pages: mixer, levels with limiter, and the EQ tabs.

## Requirements

No spec change; implements:

- `UX-16` to `UX-20`: tab row with link buttons, Mixer, Levels and EQ tabs.
- `BEH-27` to `BEH-30`: mixer, levels, EQ, filters, delay.
- `BEH-5`, `BEH-28`: limiter with linked units and gain reduction.
- `BEH-31` to `BEH-35`: linked tabs, shared EQ, one fader / limiter with stereo meters.
- `BEH-37`: preset recall, store, save and load file.
- `BEH-39`: amp selector follows the shared list.

## Tasks

- [ ] Amp selector and tab row with link buttons
- [ ] Mixer tab
- [ ] Levels tab (IN / OUT / LIMITER rows, same-size dB box and MUTE)
- [ ] EQ tab component (10 bands; outputs add HPF, LPF, delay)
- [ ] Linked layouts
- [ ] Preset actions
- [ ] Screenshots compared with the Mix & EQ boards

## Done when

- [ ] The pages match the mockup boards, linked and unlinked
- [ ] Clicking a preset on the Amplifiers page opens this page for that amp

## Notes

- Depends on `core-dsp-control`, `core-limiter`, `core-link`, `core-mute-presets` and
  `app-controls`.
- The mockup shows a fixed list of three amps; the app uses the real list.
- Where preset actions sit is not in the spec or confirmed from the mockup; check the boards
  and record it in `spec/ux.md`.
