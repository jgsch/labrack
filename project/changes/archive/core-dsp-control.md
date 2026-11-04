# core: DSP control and safe writes

- Source: roadmap Phase 1

## Why

All mixer, level, EQ, filter and delay changes go through core, which must refuse anything outside
the model range or a user ceiling and must not flood the amp while a fader moves.

## Requirements

Adds `BEH-43` (the DSP state of an ONLINE amp); edits `SEC-2` (wrong kind refused, the IPX takes the
IPD ranges) and `SEC-3` (refused, not clamped; lowering writes nothing); implements:

- `BEH-1` to `BEH-4`, `BEH-27` to `BEH-30`: state and intents for mixer, input / output gain
  and mute, 10-band EQ, HPF / LPF and delays, on both families.
- `SEC-2`: values checked against the library spec before sending; NaN / inf never sent.
- `SEC-3`: per-amp user ceilings (max output gain, max limiter threshold).
- `SEC-4`: latest value only, at most every 50 ms per parameter.
- `BEH-10`: intents for an amp that is not ONLINE are rejected, not queued.

## Tasks

- [x] Per-amp DSP state, filled from the read on connect and from amp change events
- [x] Intent API (set parameter) with range and ceiling checks; tests at the limits
- [x] Per-parameter coalescing writer (50 ms, latest wins); tests with explicit instants (a paused
      clock does not mix with real sockets)
- [x] Ceilings stored in the config
- [x] `amps`: the IPX had no ranges at all; it takes the shared ones

## Done when

- [x] Tests for SEC-2, SEC-3, SEC-4, BEH-10, BEH-27 to BEH-30 pass

## Notes

- Depends on `core-connections`.
- **The IPX had no value ranges.** `docs/ipx-protocol.md` says amp-side ranges were not determined,
  so `IpxAmp::set` would have passed gain +500 dB. `labgruppen::shared_range` now gives the IPD's
  (vendor app) ranges for gain, delay, EQ frequency / gain / Q, crossover frequency and mixer level
  to the IPX spec and to `IpxAmp::set`; `crates/amps/tests/ranges.rs` keeps the two tables equal.
  This can only restrict: if the real IPX accepts more (e.g. mixer above 0 dB), widen it there.
  Compressor and limiter ranges for the IPX are still unknown; `core-limiter` must set them before
  the limiter can be controlled.
- `Hub::set` takes only the basic controls (`dsp::controllable`): gain, mute, delay, EQ, output
  filters, input mixer. Limiter, link, compressor, polarity and names are `NotControllable` until
  the change that owns them extends it.
- A value is refused when it is NaN / inf, of the wrong kind, outside the range, or above a ceiling.
  The output gain ceiling is the only one applied here; the limiter ceiling is stored and applied by
  `core-limiter`. Defaults stay "no ceiling" (backlog).
- `Hub::range(id, &param)` returns the range for the control to show: the amp's own, ended by the
  ceiling. The IPD mixer tops out at 0 dB, so the mixer faders must take their range from here
  (backlog item on BEH-27 / UX-18).
- State: `Hub::value`, `values`, `loaded`; `Update::Loaded`, `Update::Changed` (the amp's notices and
  our own confirmed writes alike, which is what MODIFIED tracking needs), `Update::WriteFailed`.
  The IPD fills the state as it answers; `loaded` says it is complete.
- Writes: one `Writer` per connection. `Hub::set` returns when the value is accepted; the write
  follows within the interval and is confirmed by the amp's echo (IPD) or a read back (IPX). What is
  still waiting when the connection ends is dropped, never sent to the next one (tested).
- Mute goes through `set` as any control. Bulk mutes (MUTE ALL, recall order) are
  `core-mute-presets`.
