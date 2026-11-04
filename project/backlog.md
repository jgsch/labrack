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
- Limiter ranges per model. Measured in the vendor Windows app: IPX 1200 19.22 to 35.99 dBu (10 to
  69 Vpk), used for every IPX; IPD 2400 19.22 to 39.04 dBu (10 to 98 Vpk), while the library says
  16.78 to 36.78 for the IPD 2400 (vendor config). Read the other IPX models (2400, 4800) and the IPD
  1200 / 4800 in the Windows app, and compare the IPD's stored threshold (`labgruppen get
  out1.limiter.threshold`) with what the app shows for it: the ranges differ by about 2.3 dB at both
  ends, which looks like an offset between the stored value and the displayed dBu. Spec: BEH-5, SEC-3
- IPX gain reduction: the meters have two sets (`gain_reduction_a` / `_b`), both "inferred" in
  docs/ipx-protocol.md; which one is the output limiter is unknown, so the Levels limiter meter has
  nothing to show for the IPX yet. The IPD has `gain_reduction_out`. Spec: BEH-28
- IPX limiter: which of the four stored figures does the DSP limit on? The out2 of the live amp held
  a dBu that its Vpk does not give. Until known, the app shows the figures derived from Vpk and
  load, and a threshold edit writes all four. Spec: BEH-5
