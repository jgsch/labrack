# amps: library gaps for v1

- Source: roadmap Phase 1

## Why

The `amps` crate covers the protocols, but v1 needs a few things it does not offer yet: NaN is
accepted by the range check, presets cannot be stored, IPD access rights are not readable, and
preset slots are numbered differently per family.

## Requirements

No spec change; this change makes the library able to satisfy:

- `SEC-2`: `ParamSpec::check` rejects NaN and infinity for every numeric kind, ranged or not.
- `BEH-6`, `BEH-36`: both drivers expose slots 1–50; the IPX slot is the wire index + 1, as in the
  vendor app.
- `BEH-37`: store the current settings to a slot under a name (IPD `CMD_SAVE_PRESET`, IPX
  store command).
- `BEH-14`: read the IPD access rights as named flags (input channel, output channel, load
  preset, save preset, unit name).
- `ARCH-4`: each addition gets a test against the IPD simulator or the fake IPX.

## Tasks

- [x] Failing test: `check` with NaN / ±inf on Float and Int, with and without a range; fix
- [x] Preset numbering: `Amp::preset_slots()` (1–50 on both); `presets()` lists only those,
      `recall_preset` rejects other slots
- [x] `store_preset(slot, name)` on the `Amp` trait; implement for IPD and IPX; simulator and
      fake IPX support it
- [x] `IpdAmp::access_rights()` returning the decoded `M_ACCESS_RIGHTS` bits
- [ ] Faster IPD `presets()` for 50 slots (currently up to 60 s for 100)

## Done when

- [ ] Tests for SEC-2, BEH-6, BEH-14, BEH-36, BEH-37 pass
- [ ] `mise run lint` and `mise run test` pass

## Notes

- IPX slots: 50 name entries (wire 0–49), "base" at wire 1, GETPRESET 50 gets no reply. The vendor
  app (decompiled: combo box item i is labelled "Preset" + (i + 1), read-only when item index <
  ReadOnlyPresetRange) shows wire 0–49 as presets 1–50, so "base" is slot 2 and slot 1 is the
  read-only one on this amp (DEVCFG read-only range 1).
- IPX store: SETPRESET (cmd 2), payload char[16] name + u32 wire index, verified on the IPX 1200
  (slot 50). The amp makes the stored slot its current preset. DELETEPRESET did not remove it, so a
  preset named "zz-test" is left in slot 50 of the test amp.
- IPD store follows the decompiled vendor app and is only tested against the simulator.
