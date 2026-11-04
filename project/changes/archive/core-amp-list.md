# core: amplifier list and config

- Source: roadmap Phase 1

## Why

Every page works on one shared list of amplifiers that must survive a restart. This is the first
piece of the `core` crate and the base for every other core change.

## Requirements

Edits the spec for the two open questions (index, default name) and the edit-IP rule; implements:

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

- [x] Create `crates/core`; pick a package name that does not clash with Rust's `core`
      (e.g. `labrack-core`)
- [x] Amp list model and intents: add, rename, remove, edit IP
- [x] IP validation and duplicate check, with errors the UI can show next to ADD AMP
- [x] Config file: location (XDG config dir), load, save on change, round-trip test
- [x] Test that the config holds no password and no DSP setting

## Done when

- [x] Tests for BEH-7, BEH-20 to BEH-23, BEH-39, BEH-40 pass

## Notes

- Index: the position in the list, from 1, renumbered on remove (BEH-7). Intents name an amp by
  `AmpId`, which is stable for the session and never reused.
- Default name "Amp n": n is the smallest number not already used by a name (BEH-20).
- `Store` is the single list plus its config file; the app holds one and every page reads it. Every
  successful intent saves; a rejected one saves nothing. The loaded preset is not saved.
- Editing an IP accepts invalid text (the amp then shows CHECK IP) but rejects a duplicate
  (BEH-21). Adding rejects both.
- The ceilings and the IPD load are stored per amp and have no setter yet; `core-dsp-control` and
  `core-limiter` add them. The ceiling defaults stay open (backlog): until chosen, there is no
  ceiling. The limiter ceiling is stored in dBu, the one unit the IPD amp stores.
- Both discovery switches default to on; `core-discovery` may change that.
- `core` does not depend on `amps` yet; `core-connections` maps `Protocol` to the driver.
