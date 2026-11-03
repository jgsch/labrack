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
- **BEH-5** Limiter per output, with linked units: Vpk, Ohm, Watt, dBu, the same on both families
  (as in the vendor apps):
  - IPX: the amp stores Vpk and Ohm; Watt and dBu are calculated.
  - IPD: the amp stores only the threshold in dBu. The app keeps the load in Ohm per output
    (default 4 Ω) in its config and converts.
- **BEH-6** Up to 50 presets per amplifier, numbered as on the amp: IPD slots 1–50 (51–100 are not
  used), IPX slots 1–49 (the amp has no slot 50; slot 0 is never used).

## Amplifier list and status

- **BEH-7** Every amplifier has an index, a name, a protocol, an IP address, a source (AUTO or
  MANUAL) and a loaded preset.
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
- **BEH-18** Switching a protocol on starts a scan. A scan can also be started manually (SCAN NOW)
  and runs for a short time.
- **BEH-19** Amplifiers found are added with source AUTO. An IP that is already known is never
  added twice.

## Manual add, rename and remove

- **BEH-20** Manual add takes a name (optional, defaults to Amp n), a protocol and an IP address.
  It is the fallback when discovery fails.
- **BEH-21** Validation: the IP must be a valid IPv4 address and must not already be in the list.
  Each error is shown next to the add button.
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
  every setting of channel 2 follows channel 1, faders and limiter included.
- **BEH-32** Linking copies channel 1's settings to channel 2. The amps differ, and the app hides
  it:
  - IPD: the amp mirrors channel 1 to channel 2 itself. The app writes only channel 1 and never
    writes channel 2 while linked (the amp ignores it and may stop answering).
  - IPX: the amp copies once when the link is switched on, then keeps the channels independent.
    The app writes every channel 1 change to channel 2 as well.
- **BEH-33** Linked inputs share one Inputs EQ; linked outputs share one Outputs EQ.
- **BEH-34** In Levels, linked outputs show one fader and one limiter, with stereo meters (two
  meters and two gain reduction bars).
- **BEH-35** Unlinking restores separate channels and keeps the current values.

## Presets

- **BEH-36** Up to 50 presets per amplifier (IPX: 49), identified by number and name.
- **BEH-37** Actions: recall a stored preset, store the current settings to a slot under a name,
  save the settings to a JSON file on the computer, load such a file into an amp. The file format
  is the library's snapshot (`labgruppen dump`).
- **BEH-38** MODIFIED means "changed since this app recalled or loaded the preset": any change made
  in the app, and any change from elsewhere (front panel, another controller) that the app sees.
  The app does not compare with the stored preset, so:
  - after the app starts or reconnects, the flag is cleared (the app cannot know);
  - changes made elsewhere that the app does not see are missed.

## Shared amplifier list and persistence

- **BEH-39** The Amplifiers and Search pages share one amplifier list. Renaming, adding and removing
  are reflected everywhere, including Mix & EQ (which shows a fixed list of three amps in the
  design). Mute state is shared as in `BEH-26`.
- **BEH-40** The app keeps a TOML config file: the amplifier list (name, protocol, IP, source,
  connected or not), the IPD / IPX switches, the IPD load per output, and the user safety ceilings.
  No passwords and no amp settings.

## Error and empty states

- **BEH-41** No amplifiers: an empty-state message pointing to the Search tab.

Other error states are covered above: not ONLINE (`BEH-8`, `BEH-10`), both protocols off
(`BEH-17`), duplicate and invalid IP (`BEH-21`).
