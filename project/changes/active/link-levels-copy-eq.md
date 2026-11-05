# Links on Levels only, EQ copy

- Source: user feedback on the real amps (2026-11-05), after `app-mix-eq`

## Why

The amps' input and output links are separate, and the EQs of channels 1 and 2 must never be merged.
A link only means "same level": the gain of both inputs, or the gain and the limiter of both outputs.
The IPD's own link flag copies everything (EQ, filters, delay) to channel 2, so on the IPD the link is
the app's.

## Requirements

- Modify `BEH-31`, `BEH-32`, `BEH-34`: a link shares the level only (gain; outputs: limiter); IPD link
  is app-side.
- Replace `BEH-33`: COPY of the other channel's EQ.
- Modify `UX-17`, `UX-20`, `UX-24`, `SEC-5`: link buttons on Levels only, copy button on EQ tabs.

## Tasks

- [x] `core`: a link follows gain and limiter only; IPD link kept in the config, both channels written
- [x] `core`: `Hub::copy_eq`
- [x] `app`: no link on Mixer and EQ tabs, six fixed tabs, LINK INPUTS / LINK OUTPUTS on Levels
- [x] `app`: COPY button and its popup on the EQ tabs
- [x] Remove the "amp took the link back" check (the IPD flag is no longer written to link)

## Done when

- [x] Tests for `BEH-31` to `BEH-35`, `UX-17`, `UX-20` pass
- [x] The Levels, EQ and linked screens rendered and looked at

## Notes

- Not archived yet: the new links and COPY wait for a try on the real amps.
- The IPD2400 refused its link flag from this app for a while and then accepted it after the Windows
  app set it; the cause is unknown. Not needed any more: the app does not write the flag to link.
- COPY copies the bands and the HPF/LPF, not the delay (a time alignment, not part of the tone).
