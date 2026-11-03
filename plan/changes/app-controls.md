# app: shared controls

- Source: roadmap Phase 1

## Why

Faders, value fields, meters and confirm buttons are used on every page and carry most of the
UI-side safety rules.

## Requirements

No spec change; implements:

- `UX-21`: in-place edits saved on Enter or blur.
- `UX-22`: fader: no jump on click, relative drag 0.2 dB/px, keys and wheel 0.5 dB, Page 3 dB.
- `UX-23`: typed values apply on Enter or blur only.
- `UX-24`, `SEC-5`: second click within 3 s (UNMUTE ALL, LOAD) or a dialog (file load, typed
  raise above 6 dB).
- `UX-26`: red error text beside its control.
- `UX-8`, `UX-12`: segmented meter with a peach gain reduction bar.
- `SEC-2`: fader ranges come from the model spec (e.g. IPD mixer max 0 dB).

## Tasks

- [ ] Fader component with the drag / key / wheel rules; tests on its value logic
- [ ] Value field component (Enter / blur commit)
- [ ] Confirm button (3 s window) and confirm dialog
- [ ] Meter and gain reduction bar
- [ ] Error text component

## Done when

- [ ] Tests for UX-21 to UX-24 pass (logic tested without a window)
- [ ] Screenshots of the controls match the mockup

## Notes

- Keep the value logic in plain Rust (or in core) so it is testable without WebKitGTK.
- Depends on `app-shell`.
