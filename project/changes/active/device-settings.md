# Device settings: IPD lock, display, network

- Source: the user, after `lock-and-password`

## Why

The user wants to lock an IPD from the app as the Windows app does, set the front-panel display of both
families (On, Delayed Off, Dimmed), and change the address of an amp from the Search page instead of the
vendor app (which let a mask of 0.0.0.0 through and made the IPD unreachable). The level on the Amplifiers
page is noise there.

## Requirements

- Modify `BEH-47`: an IPD is locked from the app too (the lock password of the vendor app), unlocked by `BEH-15`
- Modify `UX-30`: no level on the Amplifiers page; an IPD has LOCK, and UNLOCK opens the login dialog
- Add `BEH-49` (display), `BEH-50` (network of an IPD), `UX-34` (DISPLAY and NETWORK on the Search rows)

## Tasks

- [x] Hardware (2026-11-12, user present): IPX DEVCFG field 0 is 0 On, 1 Delayed Off, 2 Dimmed, a network write
      is taken and shows on the front panel; IPD HARDWARE_STATUS 0x40 dims the display, the top byte changes on
      its own; IPD lock with CHECK_PASSWORD `dq8r0mzu` reads Locked within 0.5 s, holds across connections and
      locks the front panel, the User password unlocks; IPD IP and mask written, then command 14: the amp
      answers on the new address
- [x] `amps`: `Amp::set_locked` for the IPD, `Amp::display` / `set_display`, `Amp::network` / `set_network`,
      the simulator and the fake IPX
- [x] Core: `Hub::display`, `Hub::set_display`, `Hub::network`, `Hub::set_network`
- [x] App: the Amplifiers panel without the level, LOCK / UNLOCK for the IPD; DISPLAY and NETWORK on Search
- [x] Hardware (2026-11-12, through the library): IPX display On / Delayed Off / Dimmed read back; IPD display
      Dimmed and On; IPD locked (rights 0x4000 left), unlock refused without the password, `Password` unlocks;
      IPD moved to .184, then automatic: back at its DHCP lease .183, control port after about 30 s
- [ ] The user tries the app's buttons on both amps

## Done when

- [x] Tests for BEH-47, BEH-49, BEH-50, UX-30, UX-34 pass
- [ ] Checked on the real amps

## Notes

- From the decompiled IntelliDrive Controller 1.2.11 (`child_unit.py`, `network_settings.py`): Lock Unit writes
  CHECK_PASSWORD = `dq8r0mzu` and polls CURRENT_USER until 5; Display rewrites HARDWARE_STATUS (On clears 0x40
  and 0x80, Delayed Off sets 0x80, Dimmed clears 0x80 and sets 0x40) and confirms ignoring 0xFF00003F;
  Network Settings writes IP (13, 87) and mask (13, 40) as big-endian u32, IP 0 is automatic (DHCP, then
  ZeroConfig), there is no gateway, then command 14.
- IPX password and IPX network: not known (backlog). The Search row got a second line for the settings: at
  1200 px the name box had no room left.
