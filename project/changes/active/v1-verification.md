# v1 verification

- Source: roadmap Phase 1 exit criteria

## Why

Some exit criteria can only be checked as a whole or on hardware: the performance budgets, the
vendor app holding an IPX, and a full pass over the mockup boards.

## Requirements

No spec change; checks:

- `PERF-2`: meters at ~10 Hz for 16 amps (simulated) without visible lag.
- `PERF-3`: startup to a usable window under 2 s.
- `ARCH-7`: every page compared with its mockup board.
- `ARCH-8`: manual run on the IPD2400 and IPX 1200, outputs muted, state dumped before and
  restored after.
- `BEH-8`, `BEH-9`: BUSY while the vendor app holds the IPX; DISCONNECT lets it connect.

## Tasks

- [ ] Load test with 16 simulated amps; measure meter rate and UI frame time
- [ ] Measure startup time
- [ ] Hardware checklist (in this file), run on both amps
- [ ] Record the measured values in `project/spec/performance.md` (drop "not yet verified")

## Done when

- [ ] All Phase 1 exit criteria in `project/roadmap.md` are met
- [ ] `mise run lint` and `mise run test` pass

## Notes

- Move this file to `project/changes/archive/` and mark Phase 1 done in the same commit.
