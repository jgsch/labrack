# Required behavior

What the product does, one section per domain. Each requirement is a testable
statement with a `BEH-<n>` ID.

## Scope and hardware model

- **BEH-1** The app controls Lab.gruppen amplifiers over IP. Two protocols are supported side by
  side: IPD and IPX.
- **BEH-2** Each amplifier has 2 outputs (OUT 1, OUT 2) and 2 DSP input channels (IN 1, IN 2).
- **BEH-3** Signal path per amp: 4 sources (ANALOG 1, ANALOG 2, AES 1, AES 2) → mixer → IN 1 / IN 2
  → fixed routing IN n → OUT n → limiter → output.
- **BEH-4** 10-band EQ per channel (inputs and outputs). High-pass and low-pass filters on outputs
  only. Delay on inputs and on outputs.
- **BEH-5** Limiter per output, with linked units: Vpk, Ohm, Watt, dBu, converted the same way on
  both families. The dBu is the number the amp stores and shows on its front panel (the amps show and
  set dBu only); Vpk and Watt are the vendor app's. Vpk and dBu are one quantity, `dBu = 10 *
  log10(Vpk² / 1.2)`; the watts need the load, `W = Vpk² / (2 * Ohm)`, so a change of the load
  moves the watts only.
  - IPX: the amp stores all four figures as plain values and derives nothing. The app takes the
    dBu field as the threshold. Editing the threshold (in Vpk, Watt or dBu) writes all four together,
    as the vendor app does; editing the load writes the load and the watts only, so it cannot move a
    threshold.
  - IPD: the amp stores only the threshold in dBu, and shows it as stored. The vendor app's volts
    and watts go with a dBu 2.22 dB above the stored one (verified on an IPD 2400: stored 29.04 is
    40 Vpk, stored 32.56 is 60 Vpk). The app shows the stored dBu, and gives Vpk and Watt as the
    vendor app does; entering Vpk or Watt stores the dBu 2.22 dB lower, entering a dBu stores it as
    entered. The app keeps the load in Ohm per output (default 4 Ω) in its config.
  - Ranges: the IPD takes 16.78 up to 33.8 (IPD 1200), 36.78 (2400) or 39.83 (4800) dBu stored, which
    is 69, 98 or 138 Vpk at the top. The IPX reports none; the app takes what the Windows app allows
    on an IPX 1200, 19.22 dBu (10 Vpk) to 35.99 dBu (69 Vpk), for every IPX until the others are
    measured. The load takes 0.5 to 100 Ohm.
- **BEH-6** 50 presets per amplifier, slots 1–50, numbered as in the vendor apps (IPD slots 51–100
  are not used; the IPX counts from 0 on the wire, the vendor app and this app show index + 1).

## Amplifier list and status

- **BEH-7** Every amplifier has an index, a name, a protocol, an IP address, a source (AUTO or
  MANUAL) and a loaded preset. The index is the amp's position in the list, starting at 1; removing
  an amp renumbers the ones after it. The loaded preset is read when the amp goes ONLINE, forgotten
  when it is not, and never saved (`BEH-40`).
- **BEH-8** Each amplifier has one status at a time:

  | Status | When | Colour |
  |---|---|---|
  | ONLINE | connected, state read | green |
  | CONNECTING… | from adding, editing the IP or CONNECT until the connection succeeds or fails | peach |
  | RECONNECTING… | the connection was lost; the app retries | peach |
  | BUSY · OTHER CONTROLLER | the IPX refuses a second controller (vendor app or another LAB.RACK connected) | yellow |
  | UNREACHABLE · RETRYING | valid IP, no answer; the app keeps retrying | red, IP red |
  | CHECK IP · INVALID ADDRESS | the IP is not a valid IPv4 address | red, IP red |
  | DISCONNECTED | released by the user | gray |

- **BEH-9** Each amp has a DISCONNECT / CONNECT button. Disconnecting closes the connection, so
  another controller can use the amp; the amp stays in the list and the choice is kept after a
  restart.
- **BEH-10** The IP can be edited at any time. An amp that is not ONLINE shows no signal; its
  controls stay visible but are disabled (nothing can be written to it, see `SEC-1`).
- **BEH-11** The preset line shows number and name, plus a MODIFIED flag (see `BEH-38`).
- **BEH-12** Level, gain reduction and mute state of both outputs refresh about every 100 ms.
  The IPX reports mute changes itself; the app asks an IPD for the two output mutes every 100 ms.
- **BEH-42** The app connects to every amp that is not DISCONNECTED when it starts. A failed attempt
  (BUSY, UNREACHABLE) and a lost connection are retried after 1 s, then after twice as long each
  time, up to 15 s; the status shows the outcome of the latest attempt, and a lost connection shows
  RECONNECTING… until its first retry fails. CONNECT and editing the IP try again at once.
  Connecting only reads (`SEC-1`).
- **BEH-43** Once an amp is ONLINE the app reads its whole DSP state (reads only). The state follows
  what the app writes and what the amp reports (front panel, another controller), and is
  forgotten when the amp is not ONLINE. Until the read is done the state is partial. An IPD takes
  several seconds, so the amp is ONLINE before that. Intents (`BEH-27` to `BEH-30`) are accepted
  only for an amp that is ONLINE; they are refused otherwise, never queued (`SEC-1`). The basic
  controls are gain, mute, delay, mixer, EQ and the output filters.

## IPD access levels

- **BEH-13** The app reads and shows the amp's current user level (AllDSP, Factory, Developer,
  Admin, User, Locked).
- **BEH-14** Controls the amp's access rights do not allow (input channel, output channel, load
  preset, save preset, unit name) are disabled.
- **BEH-15** When the amp is locked or a right is missing, the app offers a login prompt
  (8-character password).
- **BEH-16** The app never reads passwords from the amp (the firmware would hand them out) and
  never stores them (see `SEC-8`).

## Discovery

- **BEH-17** Two switches, IPD and IPX, decide which protocols are searched. A protocol that is off
  is never searched; if both are off, scanning is disabled and no discovery traffic is sent.
- **BEH-18** Switching a protocol on starts a scan of that protocol. A scan can also be started
  manually (SCAN NOW), which searches every protocol that is on. A scan runs for 5 s, and SCAN NOW
  does nothing while one runs. The app also scans when it starts (only the protocols that are on). The IPX is searched
  by listening for its UDP broadcasts; the IPD by listening for its announcements and by probing
  every address of the /24 subnet the computer uses towards the outside, as the vendor app does.
- **BEH-19** Amplifiers found are added with source AUTO and a default name (`BEH-20`). An IP that
  is already known is never added twice. What a protocol finds after its switch went off is
  dropped.

## Manual add, rename and remove

- **BEH-20** Manual add takes a name (optional, defaults to Amp n, where n is the smallest number
  for which no amp is already named Amp n), a protocol and an IP address. It is the fallback when
  discovery fails.
- **BEH-21** Validation: the IP must be a valid IPv4 address and must not already be in the list.
  Each error is shown next to the add button. Editing the IP of a known amp (`BEH-10`) accepts text
  that is not a valid IPv4 address (the amp then shows CHECK IP, `BEH-8`), but rejects an address
  another amp already has; the old IP stays.
- **BEH-22** Manually added amps have source MANUAL.
- **BEH-23** Every known amplifier can be renamed (empty names are rejected and the old name
  returns) and removed.

## Mute

- **BEH-24** Mute exists per output, per amplifier (both outputs) and globally (MUTE ALL, which
  becomes UNMUTE ALL when everything is muted).
- **BEH-25** MUTE ALL and UNMUTE ALL act on the amps that are ONLINE.
- **BEH-26** A muted output shows MUTED and an empty meter. Mute state is the same on the
  Amplifiers page and in the Levels tab.

## Mixer, levels and limiter

- **BEH-27** Mixer: each of the 4 sources has a gain and a mute into IN 1 and into IN 2, with a
  level meter.
- **BEH-28** Levels: the routing is fixed, IN n → OUT n. IN n has a fader (input gain), a meter, a
  dB value and a mute; OUT n the same (output gain). Each output has a limiter with its own gain
  reduction meter and linked unit values (Vpk, Ohm, Watt, dBu).

## EQ, filters and delay

- **BEH-29** 10 bands per channel, each with on/off, type, frequency, gain and Q.
- **BEH-30** Outputs add a high-pass filter (HPF) and a low-pass filter (LPF), each with on/off,
  type and frequency, and output delay. Inputs include input delay.

## Link

- **BEH-31** Inputs and outputs can each be linked. In a linked pair, channel 1 is the master:
  channel 2 follows channel 1 in the gain, delay, EQ, output filters and output limiter (faders and
  limiter included). The mute stays per output (`SEC-7`: a link never unmutes) and the mixer per input
  channel. A setting of channel 2 of a linked pair cannot be edited; the app refuses it.
- **BEH-32** Linking makes channel 2 equal to channel 1. The amps differ, and the app hides it:
  - Both: the app reads every shared setting of channel 1, checks it (ranges, ceilings: if one fails
    nothing is written), mutes the outputs (`SEC-6`), writes it to channel 2 while the link is still
    off, then switches the link on. It does not rely on what the amp copies by itself.
  - IPD: the amp mirrors channel 1 to channel 2 itself. The app writes only channel 1 and never
    writes channel 2 while linked (the amp ignores it and may stop answering).
  - IPX: the amp copies the gain and the limiter dBu once when the link is switched on, then keeps
    the channels independent. The app writes every channel 1 change to channel 2 as well.
  - The app shows what is stored in the amp. A pair that the amp flags linked while its channels
    differ (a change made on the amp itself, or found so when the state is read) is shown as two
    unlinked channels with their stored values, with a notice that it is linked in the amp. The app
    writes nothing for it (`SEC-1`) and corrects nothing. If the channels become equal on the amp,
    the pair is shown linked again. LINK on such a pair makes channel 2 equal to channel 1 (checked,
    outputs muted around it); UNLINK clears the flag in the amp. An IPD cannot differ.
- **BEH-33** Linked inputs share one Inputs EQ; linked outputs share one Outputs EQ.
- **BEH-34** In Levels, linked outputs show one fader and one limiter, with stereo meters (two
  meters and two gain reduction bars).
- **BEH-35** Unlinking restores separate channels and keeps the current values.

## Presets

- **BEH-36** 50 presets per amplifier, identified by number and name.
- **BEH-37** Actions (recall, store and load leave both outputs muted, `SEC-6`): recall a stored preset, store the current settings to a slot under a name,
  save the settings to a JSON file on the computer, load such a file into an amp. The file format
  is the library's snapshot (`labgruppen dump`). A file is checked as a whole before anything is
  written: if one value is outside the range of the amp or above a user ceiling, the load is
  refused. Entries that cannot be set from a file are skipped and listed: output mutes (`SEC-6`),
  and the controls that only have their own change yet (the limiter, link).
- **BEH-38** MODIFIED means "changed since this app recalled or loaded the preset": any change made
  in the app, and any change from elsewhere (front panel, another controller) that the app sees.
  Recalling or storing a preset clears it; loading a file sets it, as the file is not the stored
  preset. Reading the state of the amp is not a change.
  The app does not compare with the stored preset, so:
  - after the app starts or reconnects, the flag is cleared (the app cannot know);
  - changes made elsewhere that the app does not see are missed.

## Shared amplifier list and persistence

- **BEH-39** The Amplifiers and Search pages share one amplifier list. Renaming, adding and removing
  are reflected everywhere, including Mix & EQ (which shows a fixed list of three amps in the
  design). Mute state is shared as in `BEH-26`.
- **BEH-40** The app keeps a TOML config file (`labrack/config.toml` in the user's config
  directory): the amplifier list (name, protocol, IP, source, connected or not), the IPD / IPX
  switches, and per amp the load per output (Ω) and the user safety ceilings (maximum output gain in
  dB, maximum limiter threshold in dBu as the amp stores it; no ceiling until one is set). No passwords and no amp
  settings. The list is saved after every change. A missing file means an empty list; a file that
  cannot be read is an error and is never overwritten.

## Error and empty states

- **BEH-41** No amplifiers: an empty-state message pointing to the Search tab.

Other error states are covered above: not ONLINE (`BEH-8`, `BEH-10`), both protocols off
(`BEH-17`), duplicate and invalid IP (`BEH-21`).
