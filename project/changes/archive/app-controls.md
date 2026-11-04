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

- [x] Fader component with the drag / key / wheel rules; tests on its value logic
- [x] Value field component (Enter / blur commit)
- [x] Confirm button (3 s window) and confirm dialog
- [x] Meter and gain reduction bar
- [x] Error text component

## Done when

- [x] Tests for UX-21 to UX-24 pass (logic tested without a window)
- [x] The controls draw as in the mockup's Levels strip (checked on a screenshot of
      `examples/render.rs`; the pixel comparison with the board happens in `app-mix-eq`, which puts them
      in the page)

## Notes

- Keep the value logic in plain Rust (or in core) so it is testable without WebKitGTK.
- Depends on `app-shell`.
- **Split.** `crates/app/src/control.rs` holds every rule and no drawing: `Bounds` (the model's range,
  SEC-2), `Drag` (0.2 dB per pixel, from where the drag started, clamped), `key_step` / `wheel_step`
  (0.5 dB, Page 3 dB), `step` (keeps inside the range), `typed_db` (Enter or blur only; accepts a comma,
  a minus sign, `dB`, `off`; refuses NaN, infinity and out-of-range values with the reason),
  `needs_confirmation` (a raise of more than 6 dB), `SecondClick` (3 s window, arms again when it
  expired), `meter_fill` / `gr_fill` / `meter_colour`, `number`. 13 unit tests, no window.
- **Drawing.** `crates/app/src/components.rs`: `Fader` (a `role="slider"` with the keyboard, wheel and
  pointer rules, plus its `ValueField`), `ValueField` (draft state, an inline `alertdialog` for a raise
  over 6 dB, a red `ErrorText`), `ConfirmButton` (armed label from the caller, `aria-pressed`),
  `Meter` (the mockup's gradient bar covered from the right, and the peach gain reduction bar) and
  `ErrorText` (`role="alert"`). Four tests render them with `dioxus-ssr` and check what the keyboard and
  a screen reader see.
- **Not done here.** The fader has no pointer capture: dragging works while the pointer is over the
  track. `app-mix-eq` and `app-amplifiers-page` place the controls; the confirmation of UNMUTE ALL in
  the header waits for `ConfirmButton` to be wired there, and MUTE ALL is still plain.
