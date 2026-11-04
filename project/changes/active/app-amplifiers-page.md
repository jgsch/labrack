# app: Amplifiers page

- Source: roadmap Phase 1

## Why

The main page: every amp, its status, meters and mutes.

## Requirements

No spec change; implements:

- `UX-3`, `UX-11`, `UX-12`, `UX-13`: panel layout, status line, editable IP, preset link,
  meters, mute buttons.
- `BEH-8`, `BEH-9`, `BEH-10`: status colours, DISCONNECT / CONNECT, disabled controls when not
  ONLINE.
- `BEH-11`: preset line with MODIFIED.
- `BEH-12`, `BEH-24` to `BEH-26`: live meters; output, amp and global mute.
- `BEH-41`: empty state pointing to Search.

## Tasks

- [ ] Amp panel component bound to core state
- [ ] Status line with the BEH-8 colours; IP red when invalid / unreachable
- [ ] Mute buttons and MUTE ALL / UNMUTE ALL with confirmation
- [ ] Empty state
- [ ] Screenshot compared with the Amplifiers board

## Done when

- [ ] The page matches the mockup board
- [ ] Mute state matches the Levels tab (BEH-26)

## Notes

- Depends on `app-controls`, `core-connections` and `core-mute-presets`.
- The MUTE buttons (output, amp, MUTE ALL) are never disabled by an IPD's access rights, whatever the
  level: the user must always be able to mute (BEH-14). Unmuting is disabled without the output right.
