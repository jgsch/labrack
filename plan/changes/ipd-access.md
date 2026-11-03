# IPD access levels and login

- Source: roadmap Phase 1

## Why

An IPD can be locked or limited by user level; the app must show it, disable what is forbidden and
let the user log in, without ever reading or storing passwords.

## Requirements

No spec change; implements:

- `BEH-13`: show the current user level.
- `BEH-14`: disable controls the access rights forbid.
- `BEH-15`: login prompt (8 characters) when locked or a right is missing.
- `BEH-16`, `SEC-8`: passwords never read from the amp and never stored.

## Tasks

- [ ] Core: user level and rights in the amp state; intents blocked by missing rights
- [ ] Core: login intent; the password is kept only for the call
- [ ] Test: config and logs never contain the password
- [ ] UI: level on the amp panel, disabled controls, login dialog

## Done when

- [ ] Tests for BEH-13 to BEH-16 pass

## Notes

- Depends on `amps-v1-gaps` (access rights) and, for the UI part, `app-amplifiers-page`.
- Open question (backlog): does the firmware block writes at User / Locked, and does the level
  survive a reconnect?
