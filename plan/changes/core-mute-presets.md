# core: mute and presets

- Source: roadmap Phase 1

## Why

Mute is the main safety control and presets are the main bulk change; neither may leave an output
unmuted that was muted before.

## Requirements

No spec change; implements:

- `BEH-24`, `BEH-25`, `BEH-26`: per-output, per-amp and global mute on ONLINE amps; one shared
  mute state.
- `BEH-36`, `BEH-37`: list, recall, store, save to a JSON file, load from a file (library
  snapshot).
- `BEH-11`, `BEH-38`: loaded preset and MODIFIED flag, cleared on start and reconnect.
- `SEC-6`: recall, file load and link copy mute the affected outputs, write, then restore the
  mute states from before.
- `SEC-7`: never unmute automatically.

## Tasks

- [ ] Mute intents (output, amp, all) and the MUTE ALL / UNMUTE ALL label rule
- [ ] Bulk change runner: record mutes, mute, write, restore earlier mutes; used by recall, file
      load and link copy
- [ ] Recall: a muted output stays muted even when the preset says unmuted (test)
- [ ] Store to slot, save file, load file (with snapshot validation)
- [ ] MODIFIED tracking from app intents and amp change events

## Done when

- [ ] Tests for BEH-11, BEH-24 to BEH-26, BEH-36 to BEH-38, SEC-6, SEC-7 pass

## Notes

- Confirmation (SEC-5) is a UI concern, see `app-controls`.
- Depends on `amps-v1-gaps` (store, slot numbering) and `core-dsp-control`.
- Open question (backlog): how the app sees IPD front-panel changes, which affects MODIFIED.
