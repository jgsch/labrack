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

- [x] Amp panel component bound to core state
- [x] Status line with the BEH-8 colours; IP red when invalid / unreachable
- [x] Mute buttons and MUTE ALL / UNMUTE ALL with confirmation
- [x] Empty state
- [x] Screenshot compared with the Amplifiers board

## Done when

- [x] The page matches the mockup board (`mise run app-shot` compares the header; the panels were checked
      on a screenshot with sample amps, 2026-11-04)
- [x] Mute state matches the Levels tab (BEH-26): both read the same core state

## Notes

- Depends on `app-controls`, `core-connections` and `core-mute-presets`.
- **Split.** `crates/app/src/amplifiers.rs`: `AmpView` (what a panel draws, filled by `app::views` from
  the hub), `status_colour` / `ip_wrong` (BEH-8), `AmplifierPanel` and `AmplifiersPage` with the empty
  state (BEH-41). Eight render tests: the statuses and their colours, a red address, a muted output, an
  offline amp with its three controls disabled, the empty state, MODIFIED and the dash.
- **What the page does.** CONNECT / DISCONNECT, the address edited in place (committed on change),
  the preset link opening Mix & EQ, the two output mutes and the amp mute. Mute buttons are never
  refused by the access rights (BEH-14); unmuting is, and `Hub` refuses it.
- **MUTE ALL is in the header, on every page** (UX-3), and UNMUTE ALL arms on the first click and acts
  on the second within 3 s (`control::SecondClick`). MUTE ALL acts at once.
- The MUTE buttons (output, amp, MUTE ALL) are never disabled by an IPD's access rights, whatever the
  level: the user must always be able to mute (BEH-14). Unmuting is disabled without the output right.
