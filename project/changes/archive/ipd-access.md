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

- [x] `amps`: `Access` (level, locked, rights), `Amp::access()` and `Amp::login()`; the IPD simulator
      takes a level and rights, masks the password echo as the real amp does, and logs its reads
- [x] Core: user level and rights in the amp state (`Hub::access`, `Update::Access`); intents blocked
      by missing rights (`Error::AccessDenied`)
- [x] Core: login intent (`Hub::login`); the password is passed to the amp for that call only
- [x] Test: config, updates, errors and the settings file never contain the password; the amp is
      never asked for one (`crates/core/tests/access.rs`)
- [x] UI: level on the amp panel, LOGIN dialog, disabled unmute, LOAD and SAVE, and a locked Mix & EQ tab
      (`UX-30` to `UX-32`; `crates/app/src/login.rs`, `amplifiers.rs`, `mixeq.rs`, `presets.rs`, `app.rs`)

## Done when

- [x] Tests for BEH-13 to BEH-16 pass in `core` (mutation-checked: every guard removed in turn fails one)
- [x] The UI shows the level, disables what the rights forbid and offers the login (UI tests; checked on a rendered page)

## Notes

- The UI part depends on `app-amplifiers-page`; the view reads `Hub::access(id)` (`user`, `locked`,
  `rights`, `needs_login()`) and listens for `Update::Access`.
- **Decisions.** The access is read before the amp is ONLINE, so whoever sees ONLINE finds it. It is
  not polled: a level changed at the front panel is seen at the next connect (backlog). Turning a mute
  on is never refused by the app (`SEC-6`), so MUTE and MUTE ALL work even when locked; unmuting needs
  the output right. A file load needs both channel rights, because it sets both. A wrong password is
  not an error: the amp does not say, so `login` returns the access it reads afterwards.
- **UI rule (confirmed by the user):** the app never disables turning a mute on; the controls that
  rights disable are the ones `Hub` refuses with `AccessDenied`, unmuting included.
- **Passwords** (`SEC-8`, `BEH-16`): `Hub::login` takes `&str` and hands it to the driver; nothing
  stores it. The IPD echo of the password is masked by the amp, so the client's cache holds no
  password either (the simulator masks it too). The library's `IpdAmp::login` now returns `Access`.
- Read on a real IPD 2400 (2026-11-04, reads only): User, all five rights.
- **UI decisions (the user asked to go on, no mockup board exists):** the level reads `LEVEL USER` on the status
  line (orange when a login is needed); LOGIN stands before DISCONNECT; a tab whose channels the rights forbid
  is shielded under a `LOCKED` line while the tabs still switch; a login that leaves the level and rights as
  they were says so in a toast. Not run against an IPD that is really locked: the user's IPD 2400 is at User
  with every right, so only the simulator and rendered pages were checked.
- **IPX:** `BEH-13` said an IPX has no user levels. Its configuration has a lock (`DEVCFG` field 1, 0 on the
  user's IPX 1200), a mode and rights per level (`docs/ipx-protocol.md`), and the driver parses them. Whether it
  refuses writes when locked is not known (backlog).
- Open question (backlog): does the firmware block writes at User / Locked, and does the level
  survive a reconnect?
