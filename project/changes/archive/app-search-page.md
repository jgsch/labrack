# app: Search page

- Source: roadmap Phase 1

## Why

Where amps are found, added by hand, renamed and removed.

## Requirements

No spec change; implements:

- `UX-14`, `UX-15`, `UX-25`: switches with a status line, SCAN NOW, add form, known amps list.
- `BEH-17`, `BEH-18`: switches and scan.
- `BEH-20` to `BEH-23`: add, validation errors beside ADD AMP, rename, remove.
- `BEH-39`: changes show on every page.

## Tasks

- [x] Discovery panel (toggles, status line, dimmed SCAN NOW when disabled)
- [x] Manual add form with error text
- [x] Known amplifiers rows (editable name, protocol badge, IP, source, REMOVE)
- [x] Screenshot compared with the Search board

## Done when

- [x] The page matches the mockup board (header compared by `mise run app-shot`; the page itself checked
      on a screenshot with sample amplifiers, 2026-11-04)
- [x] Changes appear on the Amplifiers page and in Mix & EQ (BEH-39): one core list feeds every page

## Notes

- Depends on `app-controls`, `core-amp-list` and `core-discovery`.
- **Split.** `crates/app/src/search.rs`: `SearchView` (switches, counts, scanning, the last scan's message,
  the rows), `status_of` (UX-25's line) and `can_scan` (BEH-17), `Switch` (a `role="switch"` with
  `aria-checked`) and `SearchPage`. Six render tests: the switches and their lines, SCAN NOW dimmed while
  scanning or with both protocols off, the rows with their badges, source and REMOVE, the form with its
  error, and the empty list.
- **In the window.** `App` reads `Update::Scan`: STARTED dims SCAN NOW, FINISHED shows what the scan
  found or why it failed. The add form keeps its fields and clears them on submit; the core's error is
  shown beside ADD AMP. Renames and removals go through the hub, so the Amplifiers page follows (BEH-39).
- **Not done.** Reordering amps, and a confirmation before REMOVE.
