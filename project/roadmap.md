# Roadmap

Initiatives too large for one change. Single-change items go to `backlog.md`.
Use Phases until v1 ships, then Now / Next / Later.

## Phases

### Phase 1: v1, replace the vendor app [ ]

- Goal: control IPD and IPX amplifiers from LAB.RACK safely, as specified.
- Spec: BEH-1 to BEH-41, UX-1 to UX-27, ARCH-1 to ARCH-12, SEC-1 to SEC-8, PERF-1 to PERF-4
- Changes (`project/changes/active/`), in dependency order:
  1. `core-amp-list`, then `core-connections` and `core-discovery` (done, archived)
  2. `core-dsp-control`, then `core-mute-presets`, `core-limiter`, `core-link` (done, archived)
  3. `ipd-access`, core part: user level, rights, login (done; the change stays active for its UI task)
  4. `app-shell`, then `app-controls`
  5. `app-amplifiers-page`, `app-search-page`, `app-mix-eq`, then the UI of `ipd-access` (level on the
     amp panel, disabled controls, login dialog), which needs the amp panel
  6. `v1-verification`
- Already in place: ARCH-9, ARCH-10, ARCH-11 (formatting in the mise tasks), ARCH-12
  (`re/flake.nix`).
- Exit criteria:
  - An amp added by discovery or by hand appears on all pages, survives a restart, and can be
    renamed and removed. (BEH-19 to BEH-23, BEH-39, BEH-40)
  - Each status in the table is shown in its situation, including BUSY when the vendor app holds
    an IPX. (BEH-8)
  - DISCONNECT releases an IPX so the vendor app can connect; CONNECT takes it back. (BEH-9)
  - Connecting to an amp sends no write (checked against the simulator and fake amp). (SEC-1)
  - No control can send a value outside the model range or above a user ceiling; a click on a
    fader track changes nothing. (SEC-2, SEC-3, UX-22)
  - UNMUTE ALL, preset recall and file load need confirmation; none of them, nor a reconnect,
    unmutes an output that was muted before. (SEC-5, SEC-6, SEC-7)
  - Linked channels follow channel 1 on both families; channel 2 is never written on a linked
    IPD. (BEH-31, BEH-32)
  - Limiter values convert between Vpk, Ohm, Watt and dBu the same way on both families. (BEH-5)
  - Meters refresh at about 10 Hz; mute state matches on the Amplifiers page and in Levels.
    (BEH-12, BEH-26, PERF-2)
  - An IPD in a locked level shows its level, disables what its rights forbid, and offers a
    login. MUTE and MUTE ALL stay available at every level; only unmuting needs the right.
    (BEH-13 to BEH-15, SEC-6)
  - The app's pages match the mockup boards. (UX-*, ARCH-7)
  - `mise run lint` and `mise run test` pass.

## Now / Next / Later

### Later

- **Compressors, input limiter and polarity**: compressors (inputs and outputs), the input limiter
  and output phase invert. Both firmwares have them; the vendor apps show some of them only at
  higher user levels.
- **IPD admin features**: changing passwords, user levels and access rights.
- **More IPD presets**: IPD presets 51–100, and importing the vendor `.icpreset` files.
- **Standby and locate (IPD)**.
- **Other Lab.gruppen families**: a new driver in `labgruppen`.
