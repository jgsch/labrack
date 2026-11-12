# Lock and password

- Source: the user, after `ipd-access`

## Why

The user wants to be sure nobody touches the amps: lock an amp from the Amplifiers page, see every amp's level on
the Search page, and change the password that unlocks.

## Requirements

- Modify `BEH-13`, `BEH-15`, `UX-30`, `UX-32`: an IPX has `Locked` / `Unlocked`; levels are read again every few
  seconds
- Add `BEH-47` (lock), `BEH-48` (password), `UX-33` (level and PASSWORD on Search)

## Tasks

- [x] Hardware (2026-11-12, user present): an IPD refuses a write of its level (`CURRENT_USER`); an IPX takes a
      write of `DEVCFG` field 1 without a password, in both directions (left locked, as found)
- [x] `amps`: `Amp::set_locked`, `Amp::change_password`, an IPX `access()` from the lock flag, the fake IPX and
      the simulator
- [x] Core: `Hub::set_locked`, `Hub::change_password`, access read again every `Timing::access_interval`
- [x] App: LOCK / UNLOCK on an IPX panel, level and PASSWORD on the Search rows, the password dialog

## Done when

- [x] Tests for BEH-13, BEH-47, BEH-48, UX-30, UX-33 pass
- [x] The password change on the real IPD is not tried yet: moved to the backlog

## Notes

- A locked IPX has no rights in the app (nothing but a mute is taken); the amp itself may still take writes
  from another controller (backlog).
- The old password is not asked: an unlocked amp cannot tell a right password from a wrong one (a login at User
  level leaves User). The dialog asks the new one twice, and the toast says it was sent, not that it worked.
- IPX password: not in the app (the amp checks none over the network, and `USERPASSWORD` was not tried).
