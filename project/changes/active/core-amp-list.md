# core: amplifier list and config

- Source: roadmap Phase 1

## Why

Every page works on one shared list of amplifiers that must survive a restart. This is the first
piece of the `core` crate and the base for every other core change.

## Requirements

No spec change; implements:

- `ARCH-2`: create `crates/core` with no UI dependency.
- `BEH-7`: amp record (index, name, protocol, IP, source, loaded preset).
- `BEH-20`, `BEH-21`, `BEH-22`: manual add with default name, IPv4 and duplicate validation,
  source MANUAL.
- `BEH-23`: rename (empty rejected, old name kept) and remove.
- `BEH-39`: one list shared by all views.
- `BEH-40`: TOML config (list, connected flag, IPD / IPX switches, IPD load per output, user
  ceilings); no passwords, no amp settings.
- `ARCH-5`: unit tests for validation, duplicates and config round-trips.

## Tasks

- [ ] Create `crates/core`; pick a package name that does not clash with Rust's `core`
      (e.g. `labrack-core`)
- [ ] Amp list model and intents: add, rename, remove, edit IP
- [ ] IP validation and duplicate check, with errors the UI can show next to ADD AMP
- [ ] Config file: location (XDG config dir), load, save on change, round-trip test
- [ ] Test that the config holds no password and no DSP setting

## Done when

- [ ] Tests for BEH-7, BEH-20 to BEH-23, BEH-39, BEH-40 pass

## Notes

- Index: position in the list (renumbered on remove) or stable? Decide and record in BEH-7.
- Default name "Amp n": is n the next free number or the index?
