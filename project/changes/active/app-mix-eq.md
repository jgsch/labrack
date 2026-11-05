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
- [x] Mixer tab
- [x] Levels tab (IN / OUT / LIMITER rows, same-size dB box and MUTE)
- [x] EQ tab component (10 bands; outputs add HPF, LPF, delay)
- [x] Linked layouts
- [x] Preset actions
- [ ] Screenshots compared with the Mix & EQ boards

## Done when

- [ ] The pages match the mockup boards, linked and unlinked
- [x] Clicking a preset on the Amplifiers page opens this page for that amp

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
  and the meter bars, and the LINK button links the pair of the open tab (outputs on Levels, inputs
  on the Mixer, as the user asked; the board hides it there). `mise run app-shot` renders `levels-demo` beside board 03.
- **EQ tabs** (2026-11-04): `EqPage` in `mixeq.rs`, its logic in `eq.rs` (type names, which boxes a
  type has, the curve from the cookbook biquads and the Butterworth / Linkwitz-Riley / Bessel filters at
  48 kHz, ALL OFF and RESET). A shared tab edits channel 1. The types offered are `EQ_TYPES` and
  `XOVER_TYPES`, the amps' own, not the mockup's list. Decided with the user and written in UX-20: RESET
  is all off, 0 dB, Q 1.4, keeping type and frequency. DELAY then got a checkbox on the user's
  request (the amps store a time only: unticking writes 0 ms, ticking restores the time, kept by `App`
  while it runs), and Q is open on every type, the shelves included. `mise run app-shot` renders `eq-in-demo` and `eq-out-demo` beside boards 04 and 05.
- **Mixer** (2026-11-04): `MixerPage` in `mixeq.rs`, the source cards are the Levels' `LevelCard`
  (a muted source dims its fader). `→ IN n` opens that input's EQ. The faders take the model's mixer
  range (-48 to 0 dB on the IPD), not the board's +12. The card meters are the IPD's `raw_input` plus
  the level; the IPX has no source meter, so its cards show none. The IPD driver marks the polarity of
  the mixer mute as unverified (`ipd/params.rs`): check it on an amp. `mise run app-shot` renders
  `mixer-demo` beside board 02.
- **Seen in the window**: a checkbox the amp refused (amp not ONLINE) stays ticked, as nothing in the
  view changed to redraw it; the same can happen to a type list.
- **Linked layouts** (2026-11-05): decided with the user, one link for everything (UX-17): LINK 1 + 2
  links inputs and outputs (`App` calls `set_link` for each pair not yet as asked). Levels linked is
  one row (BEH-34): channel 1's fader with both meters, LIMITER 1 + 2 with both gain reductions, and
  MUTE 1 / MUTE 2 in each card. `linked-demo` is rendered beside board 07. The linked EQ tabs were
  already one shared tab editing channel 1.
- **Preset bar** (2026-11-05): `presets.rs` (`PresetBar`, `Ask` and its texts, `ConfirmDialog`),
  placed and described in UX-28. `App` reads the list on ONLINE and on `Update::Preset`, runs the
  confirmed action and keeps its message per amp. The file dialogs are `rfd`, the version and backend
  dioxus-desktop already builds. LINK now asks too (UX-24 lists it; it did not before). Core:
  `Hub::preset_slots`. The preset of an amp on the Amplifiers page now opens Mix & EQ on that amp (it
  kept the amp picked before).
- **Still to do**: the boxes show
  `0` where the boards show `0.0` (`control::number`).

- Depends on `core-dsp-control`, `core-limiter`, `core-link`, `core-mute-presets` and
  `app-controls`.
- The mockup shows a fixed list of three amps; the app uses the real list.
