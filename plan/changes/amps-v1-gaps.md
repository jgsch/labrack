# amps: library gaps for v1

- Source: roadmap Phase 1

## Why

The `amps` crate covers the protocols, but v1 needs a few things it does not offer yet: NaN is
accepted by the range check, presets cannot be stored, IPD access rights are not readable, and
preset slots are numbered differently per family.

## Requirements

No spec change; this change makes the library able to satisfy:

- `SEC-2`: `ParamSpec::check` rejects NaN and infinity for every numeric kind, ranged or not.
- `BEH-6`, `BEH-36`: both drivers expose slots 1–50 with the same numbering.
- `BEH-37`: store the current settings to a slot under a name (IPD `CMD_SAVE_PRESET`, IPX
  store command).
- `BEH-14`: read the IPD access rights as named flags (input channel, output channel, load
  preset, save preset, unit name).
- `ARCH-4`: each addition gets a test against the IPD simulator or the fake IPX.

## Tasks

- [ ] Failing test: `check` with NaN / ±inf on Float and Int, with and without a range; fix
- [ ] Preset numbering: 1-based slots 1–50 on both families; IPD `presets()` reads only 1–50,
      `recall_preset` rejects out-of-range slots on both
- [ ] `store_preset(index, name)` on the `Amp` trait; implement for IPD and IPX; simulator and
      fake IPX support it
- [ ] `IpdAmp::access_rights()` returning the decoded `M_ACCESS_RIGHTS` bits
- [ ] Faster IPD `presets()` for 50 slots (currently up to 60 s for 100)

## Done when

- [ ] Tests for SEC-2, BEH-6, BEH-14, BEH-36, BEH-37 pass
- [ ] `mise run lint` and `mise run test` pass

## Notes

- IPX slot base (0 or 1) is not settled in `docs/ipx-protocol.md`; the fake IPX lists "base" at
  index 1. Confirm on the IPX 1200 before fixing the numbering.
- The IPX store command (SETPRESET 2 or PC_STORE_PRESET 21) is unused and untested; capture the
  vendor app or test on hardware with outputs muted.
