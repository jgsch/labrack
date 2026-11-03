# core: DSP control and safe writes

- Source: roadmap Phase 1

## Why

All mixer, level, EQ, filter and delay changes go through core, which must refuse anything outside
the model range or a user ceiling and must not flood the amp while a fader moves.

## Requirements

No spec change; implements:

- `BEH-1` to `BEH-4`, `BEH-27` to `BEH-30`: state and intents for mixer, input / output gain
  and mute, 10-band EQ, HPF / LPF and delays, on both families.
- `SEC-2`: values checked against the library spec before sending; NaN / inf never sent.
- `SEC-3`: per-amp user ceilings (max output gain, max limiter threshold).
- `SEC-4`: latest value only, at most every 50 ms per parameter.
- `BEH-10`: intents for an amp that is not ONLINE are rejected, not queued.

## Tasks

- [ ] Per-amp DSP state, filled from the read on connect and from amp change events
- [ ] Intent API (set parameter) with range and ceiling checks; tests at the limits
- [ ] Per-parameter coalescing writer (50 ms, latest wins); tests with a paused clock
- [ ] Ceilings stored in the config

## Done when

- [ ] Tests for SEC-2, SEC-3, SEC-4, BEH-10, BEH-27 to BEH-30 pass

## Notes

- Depends on `amps-v1-gaps` (NaN fix) and `core-connections`.
- IPD mixer gain tops out at 0 dB (backlog): the UI must take the range from here.
- Default ceilings are an open question (backlog).
