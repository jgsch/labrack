# core: mute and presets

- Source: roadmap Phase 1

## Why

Mute is the main safety control and presets are the main bulk change; neither may leave an output
unmuted that was muted before.

## Requirements

Edits `SEC-6` (what exactly a bulk change guarantees), `BEH-12` (IPD mutes are polled), `BEH-37`
(a file is checked as a whole; what is skipped) and `BEH-38` (what clears and sets MODIFIED);
implements:

- `BEH-24`, `BEH-25`, `BEH-26`: per-output, per-amp and global mute on ONLINE amps; one shared
  mute state.
- `BEH-36`, `BEH-37`: list, recall, store, save to a JSON file, load from a file (library
  snapshot).
- `BEH-11`, `BEH-38`: loaded preset and MODIFIED flag, cleared on start and reconnect.
- `SEC-6`: recall, file load and link copy mute the affected outputs, write, then restore the
  mute states from before.
- `SEC-7`: never unmute automatically.

## Tasks

- [x] Mute intents (output, amp, all) and the MUTE ALL / UNMUTE ALL label rule
- [x] Bulk change runner: record mutes, mute, write, restore earlier mutes; used by recall, file
      load and link copy
- [x] Recall: a muted output stays muted even when the preset says unmuted (test)
- [x] Store to slot, save file, load file (with snapshot validation)
- [x] MODIFIED tracking from app intents and amp change events
- [x] `amps`: the IPD simulator and the fake IPX apply the settings of a recalled preset (a recall
      that cannot unmute proves nothing)

## Done when

- [x] Tests for BEH-11, BEH-24 to BEH-26, BEH-36 to BEH-38, SEC-6, SEC-7 pass

## Notes

- Confirmation (SEC-5) is a UI concern, see `app-controls`.
- Depends on `core-dsp-control`.
- API (`Hub`): `set_mute`, `mute_amp`, `mute_all`, `unmute_all`, `amp_muted`, `all_muted`; async
  `presets`, `recall_preset`, `store_preset`, `save_file`, `load_file` (returns `LoadReport`);
  `modified`; `Update::Modified` and `Update::Preset`. The async ones run to the end in the
  caller's task; the UI spawns them and shows a busy state.
- **The bulk runner** (`hub/presets.rs`, `Hub::bulk`): holds a per-connection gate (the writer takes
  it per batch), drops the values waiting in the coalescer, reads both output mutes, mutes those
  that are not, runs the change, reads the mutes again and fixes them (mutes before unmutes). Any
  failure stops it with the outputs muted (`Error::LeftMuted`); mutation-tested for each rule.
  - Recall (`Effect::Replaces`): the amp swaps its configuration, mutes included, so the end state
    is "muted before OR muted by the preset". A preset that mutes an output the user had unmuted
    stays muted: restoring the old state there would unmute something the preset deliberately
    muted.
  - File load (`Effect::Keeps`): output mutes of the file are never applied; each output goes back
    to what it was. `core-link` will use `Keeps` for the copy.
  - The window between the amp applying an unmuting preset and the re-mute is one round trip. It
    cannot be closed: the amps recall atomically and the IPD cannot read a stored preset first.
    Told in `SEC-6`.
- **File load only writes `dsp::controllable` entries.** The limiter, link, compressor, polarity
  and names in a file are skipped and listed, because the IPX has no limiter range yet (see
  `core-dsp-control`), so a file could carry an unchecked Vpk. `core-limiter` adds the limiter to
  `controllable`, and files restore it from then on. Input and mixer mutes in a file are applied
  as any setting; only output mutes are protected (SEC-6 and SEC-7 speak of outputs).
- A file with one invalid value is refused whole, before any mute or write (ranges, kind, user
  ceilings). Cross-family files are accepted (the library snapshot is portable); entries the amp
  does not have are skipped.
- State after a bulk change: the cache keeps the output mutes just read and is cleared otherwise,
  then read again in the background (`loaded` false until done; the IPD takes seconds).
- Loaded preset after a recall is what the amp's `status()` says, not what was asked for.
- MODIFIED: set by a real change of something already known, or by an own write that changes
  something; the first sight of a value while the state is read does not count. Output mutes of an
  IPD are seen by polling every 100 ms (BEH-12); other IPD front-panel changes are not seen at all
  (backlog: BEH-38 question), so MODIFIED misses them as BEH-38 says.
- `observed()` replaced `record()`: one place for cache, `Update::Changed` and MODIFIED. Amp events
  that repeat a known value no longer produce `Changed`; confirmed own writes always do.
- Not tested: a real IPX applying a preset with different mutes (ARCH-8, manual, outputs muted).
