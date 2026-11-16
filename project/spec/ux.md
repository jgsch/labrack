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
- **UX-10** Layout: 1200 px wide boards, 40 px side padding, 12 to 20 px gaps. Every page follows the width
  of the window down to 640 px and keeps the side padding: below 880 px the header puts its tabs on a second
  line; below 1100 px the limiter of Levels goes under IN and OUT; below 1230 px the four sources of a Mixer
  row make two rows of two, and below 740 px its IN label goes under them. The labels of the tabs and of
  MUTE ALL stay on one line. Narrower than 640 px the page scrolls sideways.

## Amplifiers page

- **UX-11** One panel per amplifier in a vertical list. At the top left the name, large and without an
  index; under it the status line: the address (plain text, no border, editable in place), the protocol
  IPD / IPX, and the dot and status. At the top right the DISCONNECT / CONNECT button and the amp-level
  MUTE / UNMUTE. Below, on the left, two boxes of the same height: MODE in gray followed by the device
  modes as two buttons (STEREO, 2 WAYS), the one the amp holds pressed (`BEH-44`), and PRESET in gray
  followed by the preset name (cut with an ellipsis when long; clicking it opens Mix & EQ), with a dot
  after it when the preset is modified (`BEH-11`). Clicking the other mode asks first (`UX-24`). Every amp
  shows the MODE box; its buttons are disabled until the amp is ONLINE and its mode read. On the status
  line, after the status, `TEMP 61%` while the amp is ONLINE and reports one (`BEH-46`).
- **UX-12** On the right of those boxes, one row per output, level with the box on its left. In each row:
  the channel name in bold with, under it in gray, the level in dB and the gain reduction (MUTED instead
  of the level while the output is muted), then the segmented level meter (green, yellow, red) with a
  thinner peach gain reduction bar under it. The meter has the scale of the amp's own front panel: −60 to
  0 dB for an IPX (yellow from −24 dB, red from −9 dB), −48 to +12 dB for an IPD, 0 dB at 80 % of the bar
  (yellow from −12 dB, red above 0 dB). The level bar of each amp in the Mix & EQ picker has the same scale.
- **UX-13** At the end of each output row, one M button (36 × 36 px).
- **UX-30** The status line does not show the user level (the Search page does, `UX-33`). An ONLINE amp has
  LOCK or UNLOCK before DISCONNECT (`BEH-47`). LOCK is pressed without a question; UNLOCK unlocks an IPX at
  once and opens the login dialog for an IPD (`UX-31`). An unlocked IPD that misses a right (`BEH-15`) has
  LOGIN there instead. Without the right of the output channels, UNMUTE and the M button of a muted output
  are disabled; turning a mute on never is (`BEH-14`, `SEC-6`).
- **UX-31** LOGIN opens a small dialog like the confirmations (`UX-24`): a password box (1 to 8
  characters, the text hidden, no suggestions), LOGIN (disabled while the box is empty) and CANCEL;
  Enter logs in, Escape cancels. The password is dropped as soon as it is sent (`BEH-16`). The amp does not
  say whether it was right: a toast says so when the level is still the one it was.
- **UX-32** On Mix & EQ, LOAD and SAVE are disabled without the load and save rights. A tab whose
  channels the amp does not allow to change (the outputs' for the Levels and the output EQ tabs, the inputs'
  for the Mixer and the input EQ tabs; Levels needs both) shows a line, `LOCKED · level … · see the
  Amplifiers page`, and cannot be clicked (`BEH-14`).

## Search page

- **UX-14** Two panels side by side: the IPD / IPX switches with SCAN NOW, and the manual add form
  (name, protocol, IP, ADD AMP).
- **UX-15** Below, Known amplifiers: one row per amp with index, an editable name field, a protocol
  badge (IPD mauve, IPX sapphire), IP, source (AUTO / MANUAL) and REMOVE. After the index, an up and a
  down arrow move the row (`BEH-45`); the first row cannot go up and the last cannot go down.
  The settings of the amp take a second line of the row, under the name: PASSWORD, DISPLAY and NETWORK
  (`UX-33`, `UX-34`), then two boxes that hold the user ceilings of the amp (`SEC-3`): MAX GAIN dB and MAX LIMITER dBu.
  An empty box is no ceiling and shows the highest value the amp itself takes (`amp max` while it is
  not ONLINE), never `none`, so it is not read as having no maximum; a number sets it; text that is not a number is refused
  with an error toast and changes nothing.

- **UX-33** Each row of Known amplifiers shows the level of its amp after the source: `User`, `Locked`,
  `Unlocked`, or `—` while the amp is not ONLINE or has not told it (`BEH-13`). An IPD that is ONLINE and not
  locked has a PASSWORD button that opens a dialog like the login's (`UX-31`): the new password and the
  same again, hidden, 1 to 8 characters, CHANGE (disabled until both are filled and equal) and CANCEL
  (`BEH-48`).
- **UX-34** After PASSWORD, each row has DISPLAY, a choice of ON, DELAYED OFF and DIMMED under a caption,
  showing what the amp has (`BEH-49`), and NETWORK. Both are disabled while the amp is not ONLINE or is locked;
  DISPLAY shows `—` while the amp has not told it, and NETWORK is there for an IPD only (an IPX row keeps its
  place empty, so the columns line up). NETWORK opens a dialog like the
  login's: AUTOMATIC (DHCP), the address and the mask (filled with what the amp has, unused while AUTOMATIC
  is ticked), a line saying the amp restarts its network and the app follows it to the new address, APPLY
  (disabled while an address or mask is not valid, `BEH-50`) and CANCEL; Escape cancels.

## Mix & EQ pages

- **UX-16** Six tabs in one row: Mixer, Levels, Inputs 1 EQ, Inputs 2 EQ, Outputs 1 EQ, Outputs 2
  EQ. The tab itself is the page title; there is no separate heading.
- **UX-17** Links are on the Levels tab only: two buttons in the tab row, LINK INPUTS and LINK
  OUTPUTS, each linked (pressed) or split. The Mixer and the EQ tabs have no link button, and the six
  tabs never change. A linked pair is one card in Levels (`BEH-34`).
- **UX-18** Mixer: two rows (IN 1, IN 2). Each row shows the four sources as bordered cards with
  their fader, meter, gain and mute, and a line showing where they go (→ IN 1, → IN 2). A muted
  source keeps its meter, to see which channel has sound.
- **UX-19** Levels: two rows. Row 1: IN 1, OUT 1, LIMITER 1. Row 2: IN 2, OUT 2, LIMITER 2. Each
  panel is titled with its own name. The dB box and the MUTE button to the right of each meter
  have the same size, the limiter meter has the same length as the other two, and there is no dB
  scale line under the meters. Each meter's scale says its unit, `dB`, in small type above its numbers.
  An IN or OUT card, and a source card of the Mixer, has two columns, each titled in small type and with
  its own ruler: GAIN, the fader (the gain set, -48 to +12 dB on the ruler), and beside it SIGNAL, the
  level meter (the signal measured; two bars for a linked pair). The fader's track holds no meter. The
  SIGNAL ruler is the scale of the amp's front panel: an output's is the one of `UX-12`, an input's and a
  source's -48 to +12 dB with 0 dB at 80 % (yellow from -12 dB, red above 0 dB). A card too narrow for
  the two columns and its box puts the dB box and MUTE under them. The
  limiter's gain reduction is labelled from 0 down to 48 dB, every 12 dB like the faders (`BEH-5`): that is
  the whole range its reduction can reach, so 24 dB is half the bar. A muted IN keeps a meter: the amp reports nothing for it, so it shows the
  loudest source that is on in the mixer, at the mixer and input gains (an estimate; a muted OUT shows
  what the amp reports). A muted channel dims its whole card (a linked card when both of its
  channels are muted), on Levels and on the Mixer alike.
- **UX-20** EQ tabs: one tab per channel, with the ten bands; outputs add HPF, LPF and delay. A
  COPY FROM INPUT 2 / INPUT 1 / OUTPUT 2 / OUTPUT 1 button takes the other channel's EQ (`BEH-33`).
  - Left: the response curve (20 Hz to 20 kHz, ±15 dB) with a numbered handle per band that is on,
    and under it the HPF and LPF rows (on/off, Hz, type) on outputs and the DELAY row (on/off and
    ms) on both. The amps store only a time: DELAY is ticked while it is above
    0 ms, unticking writes 0 ms, and ticking again restores the time it had (while the app runs; with
    none, the box opens for a time).
  - Right: 10-BAND EQ with one row per band (on, number, type, Hz, dB, Q). Gain is greyed for the
    types that have none; every type has a Q (on a shelf, its slope). The types offered are the
    amps' own.
  - ALL OFF switches the ten bands off. RESET switches them off and sets gain 0 dB and Q 1.4,
    keeping type and frequency.
- **UX-28** The preset bar sits between the amplifier cards and the tab row, on every tab: PRESET,
  the list of the amp's slots (`01 · Name`, `02 · Empty`, the loaded one marked `(loaded)` or
  `(loaded, modified)`), the name to store under (the slot's name when it is picked), LOAD (a stored
  slot only), SAVE, OPEN FILE… and SAVE TO COMPUTER…, and at the end the line `Running: 01 · Name`
  (the name alone while its slot is not known). LOAD, SAVE, a picked file and LINK ask first
  (`UX-24`); SAVE TO COMPUTER… changes nothing on the amp and does not ask. Nothing works while the
  amp is not ONLINE.

## Interaction details

- **UX-21** Names and IPs are edited in place and saved on Enter or when the field loses focus.
- **UX-22** Faders: clicking the track or the cap never changes the value. Dragging moves the value
  relative to where the drag started (0.2 dB per pixel); arrow keys and the mouse wheel step
  0.5 dB, Page Up / Page Down 3 dB. A held key does not repeat (one press, one step). One drag raises
  a level by 6 dB at most from where it started (lowering is free; let go and drag again to go on),
  and the wheel raises at most once every 200 ms (`SEC-4`). A drag ends when the pointer leaves the
  track: coming back with the button held does not resume it.
- **UX-23** Typed values apply on Enter or when the field loses focus, never while typing (typing
  −20 passes through −2).
- **UX-24** Dangerous actions are confirmed. An action that mutes the outputs of an amp (preset
  recall, storing a preset, loading a file, linking channels, copying an EQ, changing the device mode) opens a validation popup that names
  the amplifier, says that its outputs will be muted and, for recall, store and load, that they
  stay muted until the user unmutes them (`SEC-6`). UNMUTE ALL takes a second click within 3 s on the
  same button (→ CONFIRM UNMUTE ALL), and so does REMOVE on the Search page (→ CONFIRM REMOVE): the
  ceilings of the amp (`SEC-3`) go with its row. Raising a level by more than 6 dB by typing opens a
  confirmation dialog. ALL OFF and RESET of an EQ take a second click within 3 s (→ CONFIRM ALL OFF,
  CONFIRM RESET), and switching an output's HPF or any EQ band off asks (SWITCH OFF / CANCEL). A click that unmutes an
  output within 600 ms of a click that muted it is ignored (a double-click on MUTE ends muted). An EQ band's gain counts the raise from 0 dB at least: easing a cut off
  (-12 to -1 dB) does not ask. A question that is open or a button that is armed belongs to one amp and
  one tab: picking another amp or tab closes it.
  A limiter threshold typed in volts or watts asks by the same rule, counted in dB of level (20·log10 of
  the voltage ratio, 10·log10 of the power ratio) and worded in dB; the load (Ω) never asks.
- **UX-25** Search switches use a toggle with a status line; a disabled SCAN NOW is dimmed.
- **UX-26** Error text is red and appears beside the control that caused it. The error of a typed
  value floats over the page (it moves nothing) and goes after 3 s.
- **UX-27** Hover brightens buttons and tabs slightly.
- **UX-29** Toasts, on every page, at the bottom right of the window: a spinner and what runs ("Linking the
  outputs…", "Copying the EQ…", recall, store, load) while an action on an amp runs, which cannot
  be started again meanwhile; and the error of an action, in red, wrapped (at most 360 px wide),
  gone after 12 s or when clicked. Errors are not written in the preset bar. They are large enough
  to be read from a distance. While an action runs, or while the picked amp is not read, the Mix & EQ
  page cannot be clicked (the header, with MUTE ALL, and the amp picker can).
