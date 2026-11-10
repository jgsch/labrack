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
- **BEH-5** Limiter per output, in linked units: Vpk, Ohm, Watt, dBu, the same on both families and
  always one consistent set: the volts and watts are those a threshold in dBu gives at the load,
  `dBu = 10 * log10(Vpk² / 1.2)` and `W = Vpk² / (2 * Ohm)`. A change of the load moves the watts only.
  - IPX: the amp stores all four figures as plain values and derives nothing. Editing the threshold
    (in Vpk, Watt or dBu) writes all four together, as the vendor app does; editing the load writes the
    load and the watts only, so it cannot move a threshold.
  - IPD: the amp stores only the threshold, **in dBV** (0 dBV is 1 V rms, 0 dBu is 0.7746 V rms), and
    the app shows dBu, `dBu = dBV + 2.2185`, as the vendor app does. Read on an IPD 2400: stored 29.04
    dBV is 31.26 dBu, 40 Vpk, 200 W into 4 ohm; stored 32.56 dBV is 34.78 dBu, 60 Vpk, 450 W. The app
    keeps the load in Ohm per output (default 4 Ω) in its config.
  - Ranges: the IPD takes 16.78 dBV (19.0 dBu) up to 33.8 (1200), 36.78 (2400) or 39.83 (4800) dBV,
    shown as 36.02, 39.0 and 42.05 dBu, which is 69, 98 and 138 Vpk at the top. The IPX reports none;
    the app takes what the Windows app allows on an IPX 1200, 19.22 dBu (10 Vpk) to 35.99 dBu (69 Vpk),
    for every IPX until the others are measured. The load takes 0.5 to 100 Ohm.
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
- **BEH-11** The preset line shows number and name, plus a MODIFIED flag, drawn as a dot (see `BEH-38`). An IPD can
  say the name of its preset and a number of 0: it is then shown by its name, and the number is the
  slot of the one stored preset that has that name, found when the app reads the stored presets
  (after the amp's state, which an IPD reads slowly).
- **BEH-12** Level and gain reduction of both outputs refresh about every 100 ms, their mute state too.
  The IPX reports mute changes itself; the app asks an IPD for the two output mutes 5 times a second,
  but not before its state is read and never while a bulk change runs: an IPD answers one thing at a
  time, and the polling made the bulk change time out. Linking and copying an EQ wait for the state, take channel 1 from the state the app holds, and ask the
  amp for the output mutes once; unlinking the app's own link does not wait for the amp.
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
  Admin, User, Locked) and its access rights when the amp goes ONLINE and after a login. A level
  changed on the amp meanwhile is seen at the next connect. An IPX has no user levels: nothing is
  shown and nothing is held back.
- **BEH-14** Controls the amp's access rights do not allow (input channel, output channel, load
  preset, save preset, unit name) are disabled, and the app refuses the intent before sending
  anything. Input and output settings need the input or output channel right; preset recall the
  load right; storing a preset the save right; loading a file both channel rights, since it sets
  both. Linking needs the right of its channels. Turning a mute **on** is never held back, because
  a muted output is never the dangerous state (`SEC-6`): the amp has the last word. Unmuting needs
  the output channel right.
- **BEH-15** When the amp is locked or a right is missing, the app offers a login prompt
  (8-character password: 1 to 8 printable ASCII characters, a short one is padded; anything else is
  refused before it is sent). The amp does not say whether a password was right; the app reads the
  level and rights afterwards and shows them, so a wrong password leaves the level as it was.
- **BEH-16** The app never reads passwords from the amp (the firmware would hand them out) and
  never stores them (see `SEC-8`).

## Discovery

- **BEH-17** Two switches, IPD and IPX, decide which protocols are searched. A protocol that is off
  is never searched; if both are off, scanning is disabled and no discovery traffic is sent.
- **BEH-18** Switching a protocol on starts a scan of that protocol. A scan can also be started
  manually (SCAN NOW), which searches every protocol that is on. A scan runs for 5 s, and SCAN NOW
  does nothing while one runs. The app also scans when it starts (only the protocols that are on). The IPX is searched
  by listening for its UDP broadcasts and by asking every address of the /24 subnets below on its
  control port (TCP 50168; only the system attributes are read, and an amp that already has a
  controller is not found). The IPD by listening for its announcements and by probing every address
  of the /24 subnet of each network interface of the computer (not only the one of its default route;
  loopback and link-local addresses excluded), all at the same time.
- **BEH-19** Amplifiers found are added with source AUTO and a default name (`BEH-20`). An IP that
  is already known is never added twice. What a protocol finds after its switch went off is
  dropped.

## Manual add, rename and remove

- **BEH-20** Manual add takes a name (optional, defaults to Amp n, where n is the smallest number
  for which no amp is already named Amp n), a protocol and an IP address. It is the fallback when
  discovery fails. An amp still named by default takes the name of its model (`IPX 1200`, `IPD2400`,
  followed by 2, 3… when taken) once it is online; a name the user chose stays.
- **BEH-21** Validation: the IP must be a valid IPv4 address and must not already be in the list.
  Each error is shown next to the add button. Editing the IP of a known amp (`BEH-10`) accepts text
  that is not a valid IPv4 address (the amp then shows CHECK IP, `BEH-8`), but rejects an address
  another amp already has; the old IP stays.
- **BEH-22** Manually added amps have source MANUAL.
- **BEH-23** Every known amplifier can be renamed (empty names are rejected and the old name
  returns) and removed.
- **BEH-45** Every known amplifier can be moved up or down in the list. There is one list: the
  Amplifiers page and the Mix & EQ amp picker show the amps in that order, and the order is saved with
  the list (`BEH-40`). New amps are added at the end. The amp picked on Mix & EQ stays picked.

## Mute

- **BEH-24** Mute exists per output, per amplifier (both outputs) and globally (MUTE ALL, which
  becomes UNMUTE ALL when everything is muted).
- **BEH-25** MUTE ALL and UNMUTE ALL act on the amps that are ONLINE, and on their two outputs only:
  inputs and the mixer are never touched.
- **BEH-26** An output whose mute the amp has not told yet (an IPD takes seconds after ONLINE) shows
  "mute unknown", not unmuted. A muted output shows MUTED and an empty meter. Mute state is the same on the
  Amplifiers page and in the Levels tab.

## Mixer, levels and limiter

- **BEH-27** Mixer: each of the 4 sources has a gain and a mute into IN 1 and into IN 2, with a
  level meter. The IPD stores it as ON (1 = the source plays), the app shows it as a mute. The meters
  are the IPD's source meters and, on the IPX, its two input signals (ANALOG 1 and 2; AES has none).
- **BEH-28** Levels: the routing is assumed fixed, IN n → OUT n (true on the IPX; an IPD2400 was seen
  feeding both outputs from IN 1, whatever IN 2 does, and the app has no setting for it). IN n has a fader (input gain), a meter, a
  dB value and a mute; OUT n the same (output gain). Each output has a limiter with its own gain
  reduction meter and linked unit values (Vpk, Ohm, Watt, dBu).

## EQ, filters and delay

- **BEH-29** 10 bands per channel, each with on/off, type, frequency, gain and Q.
- **BEH-30** Outputs add a high-pass filter (HPF) and a low-pass filter (LPF), each with on/off,
  type and frequency, and output delay. Inputs include input delay.

## Link

- **BEH-31** Inputs and outputs are linked separately. A linked pair shares its **level** only: the
  gain of both channels and, for outputs, the limiter (load, V peak, power, level). Channel 1 is the
  master: channel 2 follows it. The EQ, the output filters, the delay, the mixer and the mutes stay
  per channel (`SEC-7`: a link never unmutes): the EQs are never merged. A shared setting of channel 2
  of a linked pair cannot be edited; the app refuses it.
- **BEH-32** Linking makes channel 2 equal to channel 1 in what the pair shares. The amps differ, and
  the app hides it:
  - Both: the app reads the shared settings of channel 1, checks them (ranges, ceilings: if one fails
    nothing is written), mutes the outputs (`SEC-6`), writes them to channel 2, then makes the link
    effective; the mutes end as they were. It does not rely on what the amp copies by itself.
  - IPX: the amp keeps the link flag, copies the gain and the limiter dBu once when it is switched
    on, then keeps the channels independent. The app writes every change of a shared setting to both
    channels. The app shows what is stored in the amp: a pair that the amp flags linked while the
    shared settings of its channels differ (a change made on the amp itself, or found so when the state
    is read) is shown as two unlinked channels with their stored values, with a notice that it is
    linked in the amp. The app writes nothing for it (`SEC-1`). If the channels become equal on the
    amp, the pair is shown linked again. LINK on such a pair makes channel 2 equal to channel 1
    (checked, outputs muted around it); UNLINK clears the flag in the amp.
  - IPD: the amp's own link flag copies **everything** of channel 1 to channel 2 (EQ, filters, delay
    included) and ignores what is written to channel 2, so the app never switches it on. The link is
    the app's: it is kept in the config of the amp and the app writes both channels. The load of the
    limiter, which the app keeps, is shared as well. A link kept by the app whose channels differ when
    the state is read is dropped (the pair is shown as two channels). A pair that the IPD itself has
    linked (front panel, another tool) is shown linked with channel 1's values; nothing of channel 2
    can then be edited, because the amp keeps it equal, and UNLINK switches the flag off in the amp.
- **BEH-33** Each EQ tab has a COPY button that takes the EQ of the other channel of the pair (input 1
  and 2, output 1 and 2): the ten bands and, on outputs, the HPF and LPF. The delay is not copied. It
  asks first, mutes the outputs while it is done and puts them back as they were (`SEC-6`, `SEC-7`).
  The channels are otherwise never merged.
- **BEH-34** In Levels, linked inputs show one IN 1 + 2 card with one fader (channel 1's) and stereo
  meters, and linked outputs show one OUT 1 + 2 card and one LIMITER 1 + 2 card with channel 1's
  figures and two gain reduction bars. Each card keeps a MUTE per channel (MUTE 1, MUTE 2), as mutes
  stay per channel. The sides are independent: with only one side linked, the second row shows the
  other side's channel 2 alone.
- **BEH-35** Unlinking restores separate channels and keeps the current values.
- **BEH-44** Device mode. An IPD and an IPX each have a mode, `stereo` or `two_way` ("2 ways" on its
  front panel), that decides which inputs feed which outputs: in stereo input n feeds output n, in two
  ways an IPD2400 feeds both outputs from input A (seen on hardware). On an IPD the mode is bit 0x1000
  of a number the amp also uses for other things, so a change keeps the other bits. The app reads it with the state and
  asks for it again about once a second, so a change made on the front panel shows. Changing it from
  the app asks first, mutes the outputs while it is done and puts them back as they were (`SEC-6`,
  `SEC-7`). A settings file never sets it. The IPX keeps it as a number of its own (MODEAES, field 0: 1 stereo,
  0 2 ways); it is asked for about once a second too.

## Presets

- **BEH-36** 50 presets per amplifier, identified by number and name.
- **BEH-37** Actions (recall, store and load leave both outputs muted, `SEC-6`): recall a stored preset, store the current settings to a slot under a name,
  save the settings to a JSON file on the computer, load such a file into an amp. The file format
  is the library's snapshot (`labgruppen dump`). A file is checked as a whole before anything is
  written: if one value is outside the range of the amp or above a user ceiling, the load is
  refused, except a limiter value a hair beyond the range (the amp itself stores 36.02 dBu for its
  35.99 dBu maximum), which is taken as the nearest value in range. Entries that cannot be set from a
  file are skipped and listed: output mutes (`SEC-6`), the controls that only have their own change yet
  (the link, names, polarity, compressors, input limiters, device mode), and the output limiter of a
  file from the other family (the threshold is dBV on an IPD and Vpk on an IPX).
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
