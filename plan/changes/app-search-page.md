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

- [ ] Discovery panel (toggles, status line, dimmed SCAN NOW when disabled)
- [ ] Manual add form with error text
- [ ] Known amplifiers rows (editable name, protocol badge, IP, source, REMOVE)
- [ ] Screenshot compared with the Search board

## Done when

- [ ] The page matches the mockup board
- [ ] Changes appear on the Amplifiers page and in Mix & EQ (BEH-39)

## Notes

- Depends on `app-controls`, `core-amp-list` and `core-discovery`.
