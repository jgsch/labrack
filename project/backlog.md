# Backlog

Bugs, features, and chores small enough for one change, highest priority first.
Larger initiatives go to `roadmap.md`. Remove an item when its work is
committed.

## Bugs

## Features

## Chores

Open questions to resolve; each blocks or shapes the requirements listed.

- IPD login: does the firmware block writes at User or Locked level, and does the level survive a
  reconnect or power cycle? Spec: BEH-13 to BEH-15
- How does the app see IPD changes made on the front panel? Polling the full state takes about
  16 s. Spec: BEH-38
- IPD mixer gain tops out at 0 dB while the mockup faders go to +12: show the model's range on the
  fader. Spec: BEH-27, UX-18, SEC-2
- Choose defaults for the user ceilings. Spec: SEC-3, BEH-40
- IPD MK2 units (different framing) have not been tried on hardware. Spec: BEH-1
- IPX limiter range: the IPX reports none. The app takes 16.78 to 39.83 dBu (the widest IPD
  ceiling), which a real IPX 1200 respects (it holds 35.98 dBu) but is far above what an IPX 1200
  needs. Find the real limits per model, or choose a tighter default ceiling. Spec: BEH-5, SEC-3
- IPX gain reduction: the meters have two sets (`gain_reduction_a` / `_b`), both "inferred" in
  docs/ipx-protocol.md; which one is the output limiter is unknown, so the Levels limiter meter has
  nothing to show for the IPX yet. The IPD has `gain_reduction_out`. Spec: BEH-28
- IPX limiter: which of the four stored figures does the DSP limit on? The out2 of the live amp held
  a dBu that its Vpk does not give. Until known, the app shows the figures derived from Vpk and
  load, and a threshold edit writes all four. Spec: BEH-5
