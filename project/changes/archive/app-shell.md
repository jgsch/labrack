# app: window, header and style

- Source: roadmap Phase 1

## Why

The Dioxus app needs a window, navigation and the mockup's look before any page can be built.

## Requirements

No spec change; implements:

- `ARCH-1`, `ARCH-2`: `crates/app` (Dioxus 0.7 desktop), mockup CSS reused, markup in RSX.
- `UX-1` to `UX-10`: tabs, header, focus outline, Catppuccin palette, fonts, layout.
- `UX-27`: hover brightening.
- `PERF-1`: runs on Wayland and X11, x86_64 and aarch64.
- `PERF-4`: fonts and assets bundled, works offline.

## Tasks

- [x] Create `crates/app` and wire it to core (state in, intents out)
- [x] Import the mockup CSS (in `project/mockup/mockup/boards/`) and the fonts
      (`project/mockup/mockup/assets/*.woff2`, licences in `project/mockup/mockup/licenses.txt`)
- [x] Header, three tabs, MUTE ALL slot, keyboard focus
- [x] App screenshot task (mise) and comparison with the mockup header (`scripts/mockup.py compare`;
      the mockup screenshots are taken with the meters frozen, `mise run mockup`)
- [x] `mise run build` builds the app; add a `run` task

## Done when

- [x] Header and tabs match the mockup board screenshots (`mise run app-shot`: 0 of 120,000 pixels differ
      on all three pages)
- [x] The app starts with the network down and shows the bundled fonts (the fonts are in the stylesheet,
      nothing is fetched; checked with the window running, 2026-11-04)

## Notes

- Depends on `core-amp-list`.
- **What is in the crate.** `crates/app` is `labrack-app`: the library (`App`, `Header`, `Page`,
  `stylesheet`) and the `labrack` binary. `mise run run` starts the window, `mise run app-shot` renders
  the pages without a window (`examples/render.rs`), screenshots them with Chromium at 1200 px and
  compares the top 100 px with the matching board.
- **State and intents.** `App` builds one `Hub` (the config file from `Store::default_path`, falling back
  to an in-memory list when the file cannot be used, which is then never written), and follows
  `Hub::subscribe`: meters are left to the pages that show them, everything else bumps a revision the
  view reads. The header takes `page`, `all_muted` and two `EventHandler`s.
- **Where the style lives.** `src/style.css` is the mockup's inline style turned into classes (same
  values: the 40 px padding, the 46/44 px header pieces, the focus ring, the hover). `src/style.rs`
  prepends the six `@font-face` rules with the WOFF2 files embedded as data URIs, so the stylesheet is
  self-contained (PERF-4). The window sets it as its custom head and its background to `#11111b`, so
  there is no white flash before the page paints.
- **MUTE ALL is on the Amplifiers page only** (UX-3, confirmed by the user). The mockup had it on the
  Mix & EQ boards too; boards 02-05 and 07-09 were changed to drop it and `mockup.html` repacked.
- **MUTE ALL writes the two output mutes** of each ONLINE amp and nothing else (BEH-25, confirmed by the
  user); `mute_all_writes_the_two_output_mutes_only` pins it on the IPD simulator and the fake IPX.
- **UX-4 and the keyboard.** Tabs are links (`href="#…"`, `aria-current` on the active one), MUTE ALL a
  button: both are in the tab order and draw the mockup's focus ring. Arrow-key movement between the
  tabs is not built.
- **Seen in a real window** (niri on Wayland, 2026-11-04; `mise run app`): the header at several window
  sizes; a click on SEARCH switches the tab and drops MUTE ALL; a scan found the IPD 2400 and saved it
  to the config. The tiling desktop gave a 949 px window once, which wrapped the tabs over the title:
  the frame now has a 900 px minimum width and the labels do not wrap, so a narrow window scrolls
  instead (test).
- **The stylesheet is part of the page** (`Style`), not the window's head: with the head, WebKitGTK
  applied the rules but not the font faces, and the page was caught unstyled. The page is the same in
  the window and in `app-shot`, and a test fails if a font face or the rules go missing.
- **A WebKitGTK scale bug on Wayland** (niri, display at 1.5, 2026-11-04): the window reported a device
  pixel ratio of `-2/96` instead of 2, so every page was drawn at a fraction of its size and only the
  native title bar looked right. The page's ratio is proportional to its zoom, so `App` reads it once
  and, when it is not the window's scale, sets the zoom to `scale / dpr` (`corrected_zoom`, tested:
  -2/96 at scale 2 gives -96, which is what this WebKitGTK needs; a right ratio is left alone).
  X11 was not affected; the app runs natively on Wayland, nothing forces XWayland.
- **The title bar is dark**: `main` sets `GTK_THEME=Adwaita:dark` when the user set no theme, so the
  window frame of a dark app is not a light band.
- **`mise run app`** starts the window (the task was `run`).
- **UNMUTE ALL is disabled** until `app-controls` adds the second click (SEC-5); MUTE ALL works.
