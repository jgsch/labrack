# core: limiter units

- Source: roadmap Phase 1

## Why

The limiter is set in Vpk, Ohm, Watt or dBu on both families, but the IPX stores Vpk and Ohm and
the IPD only dBu.

## Requirements

Rewrites `BEH-5` (the backlog bug: the IPX stores all four figures; how an edit writes them; the
ranges), edits `SEC-3` (the limiter ceiling is a dBu and holds in every unit and in files); implements:

- `BEH-5`: linked Vpk / Ohm / Watt / dBu with the same conversion on both families; IPD load per
  output from the config (default 4 Ω).
- `BEH-28`: limiter values and gain reduction per output.
- `SEC-3`: the maximum limiter threshold ceiling applies in every unit.

## Tasks

- [x] `amps`: make IPX limiter Watt (field 2) and dBu (field 3) writable; they are plain stored
      values (docs/ipx-protocol.md), the amp derives nothing
- [x] Conversion functions with tests: W = Vpk² / (2·Ω), dBu = 10·log10(W·Ω / 0.6); same results
      on both families (`labgruppen::limiter`, `labrack_core::Limiter`)
- [x] IPX: every limiter threshold edit writes Vpk, Ω, W and dBu together, as the vendor app does
      (a load edit writes Ω and W only, see Notes)
- [x] Limiter intents in any unit, converted to what the family stores
- [x] IPD load per output in the config (setter and intent)
- [x] Settings files restore the output limiter, with the same range and ceiling checks
- [ ] Gain reduction per output: not done for the IPX, see Notes

## Done when

- [x] Tests for BEH-5 and the limiter part of BEH-28 pass (the gain reduction part: see Notes)

## Notes

- Depends on `core-dsp-control`.
- API (`Hub`): `limiter(id, output) -> Limiter {vpk, ohm, watt, dbu}`, `set_limiter(id, output,
  Unit, value)`, `set_load`, `limiter_range(id, output, Unit)`; `Update::Settings` after a load or
  ceiling change. `Hub::set` still refuses the limiter parameters: their figures must go together.
- **IPX limiter range.** The IPX reports none. The first plan was the IPD ceiling of the same model
  number, but the fixture is from a real IPX 1200 holding 69 Vpk = 35.98 dBu, above the IPD 1200
  ceiling of 33.8. So every IPX takes the widest IPD range, 16.78 to 39.83 dBu (`LIMITER_DBU` in
  `amps`, enforced in `IpxAmp::set` as well). That blocks absurd values, not a high threshold:
  **the user ceiling is the tight limit, and its default is still open** (backlog, SEC-3). IPX
  ranges per model are a new backlog chore.
- **A load edit does not touch the threshold.** Dropping the vendor-app habit of "write everything
  on every edit" there is deliberate: the live out2 holds a dBu that its Vpk does not give, and
  nobody knows which field the DSP limits on. Rewriting all four after a load edit would have
  moved the dBu up by 1.2 dB without the user asking. A threshold edit writes all four, so all of
  them agree on the level the user chose.
- Display is derived from Vpk and load on the IPX (from dBu and load on the IPD), so an amp whose
  stored figures disagree shows the Vpk's reading until the first threshold edit (backlog).
- The IPD stores the dBu in 0.01 steps, so a threshold entered in Watt comes back up to 0.2 % off
  on an IPD; the IPX keeps what was written.
- An edge of the range reached through another unit is the edge (`snap`), so dragging a Watt box to
  its maximum is not refused for a float.
- Negative or zero Vpk / Watt are refused (Vpk² would hide the sign), as are NaN and infinity.
- **Gain reduction (BEH-28).** `Hub::meters` passes the raw sets: IPD `gain_reduction_out`, IPX
  `gain_reduction_a` / `_b`. Which IPX set is the output limiter is "inferred" in the docs, so
  there is no accessor; added to the backlog. The Levels limiter meter can show the IPD now.
- `restore`d files: the snapshot keeps floats to 4 decimals, so a restore moves a stored dBu by up
  to 5e-5 dB.
- The existing `ipx` tests that said limiter watts / dBu were read-only, and that a restore
  applies 280 values, were updated (284 now): that is the change.
- Mutation-tested: no ceiling, no range, a load edit writing all four, a negative Vpk, a file past
  the ceiling.

## Revised after reading a live IPD 2400 and IPX 1200 (see `BEH-5`)

- **The IPD stores 2.22 dB less than the vendor app shows** (`labgruppen::limiter::IPD_DISPLAY_OFFSET`).
  The app shows and takes the vendor's dBu, Vpk and Watt, and stores 2.22 dB less (rounded to the
  IPD's 0.01 steps). The library's stored ranges become 19.0 to 36.02 / 39.0 / 42.05 dBu shown. The
  user ceiling is the dBu as shown, in files too.
- **The IPX figures follow the dBu field**, which is what both amps' front panels show and set, not
  the Vpk. The notes above that say the display is derived from Vpk describe the first version.
- The IPX range is the Windows app's for a 1200 (19.22 to 35.99 dBu); the 2400 and 4800 are in the
  backlog.

## Second revision: the dBu shown is the dBu stored

The IPD's dBu is shown exactly as stored (the amps' front panels show and set dBu only). Only the
volts and watts are the vendor app's, which go with a dBu 2.22 dB above the stored one
(`Limiter::stored(dbu, ohm, offset)`); entering volts or watts stores 2.22 dB less. Ranges and the
user ceiling are in the stored dBu. The same edit in dBu is therefore not the same volts on the two
families; in Vpk or Watt it is. This replaces the first revision's "shown dBu = stored + 2.22".
