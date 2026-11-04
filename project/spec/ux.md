# User experience

How the product looks and feels to the user. Requirements use `UX-<n>` IDs.

UI reference: `project/mockup/mockup.html` (render it with `mise run mockup`).

## Navigation and header

- **UX-1** Three top-level tabs in the header centre: Amplifiers, Mix & EQ, Search. The active tab
  is filled lavender.
- **UX-2** Header left: the title LAB.RACK with the subtitle DSP CONTROLLER, letter-spaced to the
  exact width of the title.
- **UX-3** Header right, on every page: MUTE ALL / UNMUTE ALL. Whatever page is open, the
  outputs are one click away from silence.
- **UX-4** All controls are keyboard reachable, with a visible focus outline.

## Visual style

- **UX-5** Catppuccin Mocha with a lavender accent.
- **UX-6** Palette: page #11111b, panels #1e1e2e, inset #181825, borders #45475a / #585b70, buttons
  and meter tracks #313244, text #cdd6f4, secondary text #a6adc8.
- **UX-7** Accents: lavender #b4befe (primary, outputs), sapphire #74c7ec (inputs), mauve #cba6f7
  (IPD).
- **UX-8** Meters: green #a6e3a1, yellow #f9e2af, red #f38ba8 (clip), gain reduction peach
  #fab387. Pressed MUTE: maroon #eba0ac.
- **UX-9** Typography: Barlow Condensed for the title, subtitle and large indexes; IBM Plex Mono
  for everything else. Both fonts ship with the app (no network needed, see `PERF-4`): the Latin
  WOFF2 files are in `project/mockup/mockup/assets/` (Barlow Condensed 500, 600, 700; IBM Plex Mono
  400, 500, 600) and their licences (SIL OFL 1.1) in `project/mockup/mockup/licenses.txt`.
- **UX-10** Layout: 1200 px wide boards, 40 px side padding, 12 to 20 px gaps.

## Amplifiers page

- **UX-11** One panel per amplifier in a vertical list. Left: large index, name with the
  DISCONNECT / CONNECT button, status line, then IP in gray followed by the address (plain text,
  no border, editable in place), then PRESET in gray followed by the preset (clicking it opens
  Mix & EQ).
- **UX-12** Centre: one meter per output. Above each meter, the channel name on the left and level
  in dB with gain reduction on the right. The segmented level meter (green, yellow, red) has a
  thinner peach gain reduction bar under it.
- **UX-13** Right edge: one M button per output (44 × 44 px) and the amp-level MUTE / UNMUTE
  button, so all mute buttons sit in the same area.

## Search page

- **UX-14** Two panels side by side: the IPD / IPX switches with SCAN NOW, and the manual add form
  (name, protocol, IP, ADD AMP).
- **UX-15** Below, Known amplifiers: one row per amp with index, an editable name field, a protocol
  badge (IPD mauve, IPX sapphire), IP, source (AUTO / MANUAL) and REMOVE.

## Mix & EQ pages

- **UX-16** Six tabs in one row: Mixer, Levels, Inputs 1 EQ, Inputs 2 EQ, Outputs 1 EQ, Outputs 2
  EQ. The tab itself is the page title; there is no separate heading.
- **UX-17** The link buttons sit in the tab row. When linked, the two input tabs become one and the
  two output tabs become one.
- **UX-18** Mixer: two rows (IN 1, IN 2). Each row shows the four sources as bordered cards with
  their fader, meter, gain and mute, and a line showing where they go (→ IN 1, → IN 2).
- **UX-19** Levels: two rows. Row 1: IN 1, OUT 1, LIMITER 1. Row 2: IN 2, OUT 2, LIMITER 2. Each
  panel is titled with its own name. The dB box and the MUTE button to the right of each meter
  have the same size, the limiter meter has the same length as the other two, and there is no dB
  scale line under the meters.
- **UX-20** EQ tabs: one tab per channel (or one shared tab when linked), with the ten bands;
  outputs add HPF, LPF and delay.
  - Left: the response curve (20 Hz to 20 kHz, ±15 dB) with a numbered handle per band that is on,
    and under it the HPF and LPF rows (on/off, Hz, type) on outputs and the DELAY row (ms and the
    distance in metres of air) on both. Delay has no on/off: the amps store only a time, 0 ms is none.
  - Right: 10-BAND EQ with one row per band (on, number, type, Hz, dB, Q). Gain is greyed for the
    types that have none, Q for the shelves and filters. The types offered are the amps' own.
  - ALL OFF switches the ten bands off. RESET switches them off and sets gain 0 dB and Q 1.4,
    keeping type and frequency.

## Interaction details

- **UX-21** Names and IPs are edited in place and saved on Enter or when the field loses focus.
- **UX-22** Faders: clicking the track or the cap never changes the value. Dragging moves the value
  relative to where the drag started (0.2 dB per pixel); arrow keys and the mouse wheel step
  0.5 dB, Page Up / Page Down 3 dB.
- **UX-23** Typed values apply on Enter or when the field loses focus, never while typing (typing
  −20 passes through −2).
- **UX-24** Dangerous actions are confirmed. An action that mutes the outputs of an amp (preset
  recall, storing a preset, loading a file, linking channels) opens a validation popup that names
  the amplifier, says that its outputs will be muted and, for recall, store and load, that they
  stay muted until the user unmutes them (`SEC-6`). UNMUTE ALL takes a second click within 3 s on the
  same button (→ CONFIRM UNMUTE ALL). Raising a level by more than 6 dB by typing opens a
  confirmation dialog.
- **UX-25** Search switches use a toggle with a status line; a disabled SCAN NOW is dimmed.
- **UX-26** Error text is red and appears beside the control that caused it.
- **UX-27** Hover brightens buttons and tabs slightly.
