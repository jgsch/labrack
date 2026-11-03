# core: limiter units

- Source: roadmap Phase 1

## Why

The limiter is set in Vpk, Ohm, Watt or dBu on both families, but the IPX stores Vpk and Ohm and
the IPD only dBu.

## Requirements

No spec change; implements:

- `BEH-5`: linked Vpk / Ohm / Watt / dBu with the same conversion on both families; IPD load per
  output from the config (default 4 Ω).
- `BEH-28`: limiter values and gain reduction per output.
- `SEC-3`: the maximum limiter threshold ceiling applies in every unit.

## Tasks

- [ ] `amps`: make IPX limiter Watt (field 2) and dBu (field 3) writable; they are plain stored
      values (docs/ipx-protocol.md), the amp derives nothing
- [ ] Conversion functions with tests: W = Vpk² / (2·Ω), dBu = 10·log10(W·Ω / 0.6); same results
      on both families
- [ ] IPX: every limiter edit writes Vpk, Ω, W and dBu together, as the vendor app does
- [ ] Limiter intents in any unit, converted to what the family stores
- [ ] IPD load per output in the config

## Done when

- [ ] Tests for BEH-5 and the limiter part of BEH-28 pass

## Notes

- Not blocked: writing all four IPX values consistently makes "which one the DSP uses" moot. That
  question is still open (see `docs/ipx-protocol.md`).
- Depends on `core-dsp-control`.
