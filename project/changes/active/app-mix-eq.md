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
- [x] Levels tab (IN / OUT / LIMITER rows, same-size dB box and MUTE)
- [ ] EQ tab component (10 bands; outputs add HPF, LPF, delay)
- [ ] Linked layouts
- [ ] Preset actions
- [ ] Screenshots compared with the Mix & EQ boards

## Done when

- [ ] The pages match the mockup boards, linked and unlinked
- [ ] Clicking a preset on the Amplifiers page opens this page for that amp

## Notes

- **Done so far** (2026-11-04): `crates/app/src/mixeq.rs` holds `MixTab` (`row` and `visible`, UX-16 and
  UX-17: six tabs, four with both pairs linked, and a hidden tab falls back to the shared one),
  `MixeqView`, `Strip` / `LevelRow`, `StripPanel`, `LevelsPage` and `MixeqPage` (the amplifier picker,
  the tab row with LINK IN / LINK OUT, and the tab itself). Four render tests. `App` fills the view
  (`app::mixeq_view`) from the hub's gains, mutes and limiter, and sends gains, mutes and links back.
- **Levels redone after board 03** (2026-11-04): the first version used the generic fader (box under
  it), a horizontal meter and an unstyled heading, and fed the gain to the meter. Now each IN / OUT card
  is the board's: dB scale, the level meter inside the fader track, the box and MUTE beside it; the
  LIMITER card has the vertical gain reduction bar and the Load / V peak / Power / Level boxes
  (`hub.set_limiter`). The tabs are the board's folder tabs on the panel, the amp cards have the index
  and the meter bars, and the LINK button links the pair of the open tab (outputs on Levels), hidden on
  the Mixer. `mise run app-shot` renders `levels-demo` beside board 03.
- **Still to do**: the linked Levels layout (BEH-34: one row, stereo meters; what the IN fader does
  while only the outputs are linked is open), the preset bar, the boxes show `0` where the board shows
  `0.0` (`control::number`), the Mixer tab (UX-18: two rows of four source cards with fader, meter, gain and
  mute, and where they go), the EQ tabs (UX-20: ten bands, and HPF, LPF and delay on the outputs), and
  the screenshot of those against boards 02-05 and 07-09.

- Depends on `core-dsp-control`, `core-limiter`, `core-link`, `core-mute-presets` and
  `app-controls`.
- The mockup shows a fixed list of three amps; the app uses the real list.
- Where preset actions sit is not in the spec or confirmed from the mockup; check the boards
  and record it in `project/spec/ux.md`.
