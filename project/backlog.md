# Backlog

Bugs, features, and chores small enough for one change, highest priority first.
Larger initiatives go to `roadmap.md`. Remove an item when its work is
committed.

## Bugs

- Ceilings, the IPD load and the app's link are kept per list row, which is keyed by IP. An amp that gets a
  new address (DHCP) is a new row without ceilings, and no default ceilings exist. Key them to the device
  (IPX serial, IPD MAC) or warn when an amp has none. Spec: SEC-3, BEH-40
- A value finer than the amp stores (Q 0.74 on an IPD, which holds 0.7) is reported as confirmed and the
  box shows the requested value until the state is read again. Shown value should be the amp's echo.
  Spec: BEH-43
- `Shared::load` keeps whichever value arrived first and `read_state_again` does not wait for the previous
  read: a read begun before a recall can install old values and mark the state loaded. Also the shield is only
  an overlay, so the keyboard still reaches the controls under it. Spec: BEH-43, SEC-4, UX-29
- The IPD model name can arrive after connect (seen once: `IPD` instead of `IPD2400`); until then the limiter
  range is the IPD 1200's and a default-named amp is named `IPD`. Wait for the name, or re-evaluate. Spec: BEH-5, BEH-20
- The task started by `read_state_again` (presets.rs) is not ended with the connection; change events lost to
  a lagged channel (`pump`, hub.rs) are dropped without a re-read. Spec: BEH-43
- Do the amps refuse network writes while locked? The app holds back its own writes (`BEH-14`) but that is
  not known for the IPX (lock flag set, then a mute written) nor for the IPD (Locked level). Also: the IPD
  `SET_PASSWORD` of the User level (`BEH-48`) was only tried against the simulator; its echo on the real amp is
  unknown, so the app sends it and cannot confirm it. Try it on the IPD2400 with the front panel to unlock.
  Spec: BEH-13, BEH-14, BEH-47, BEH-48
- A scan probes IPX amps the user released with DISCONNECT, briefly taking their single controller slot.
  Skip addresses of known amps with `connected = false`. Spec: BEH-9, BEH-18

- `an_ipx_is_locked_and_unlocked_from_the_app` (core `access.rs`) counts the fake's writes right after
  queueing a mute that the write loop sends later; it failed once in a full run (2026-11-12). Wait for the
  mute to be written, or count only the lock. Spec: BEH-47

## Features

- IPX password: change the password an IPX unlocks with, as the IPD (`BEH-48`). The addresses are named
  (USERPASSWORD env 24, item 0 field = level; ACK env 18 item 1 for the status; XOR key in the docs) but the
  payload and the confirmation are not known: read them from IPX Controller, then try on the IPX 1200 (its
  passwords can be read back from DEV_PRM_SYNC to check). Spec: BEH-48, UX-33
- IPX network: change the address of an IPX from the Search page, as the IPD (`BEH-50`). DEV_PRM_SYNC shows
  the settings at 0x8C (DHCP flag, port, IP, mask, gateway as text), but how IPX Controller writes them (env 3
  NETWORK, SET_HARDWARE_CFG 23?) is not known. Spec: BEH-50, UX-34
- The display of an amp changed at its front panel shows on the Search page only after a reconnect (it is
  read once per connection). Spec: BEH-49

## Chores

- Spec decisions to reconsider (they match the spec today): ticking an EQ band on, or changing its type, can
  raise the sound by up to 12 dB without asking; lowering an HPF frequency or softening its slope does not ask
  while switching it off does; the output gain ceiling does not count input gain or EQ boosts; a device mode
  change re-routes the inputs and then puts the outputs back unmuted if they were. Spec: UX-24, SEC-3, SEC-5

Open questions to resolve; each blocks or shapes the requirements listed.

- IPD login: does the firmware block writes at User or Locked level, and does the level survive a
  reconnect or power cycle? The app holds back what the rights forbid either way. A real IPD 2400 reads
  as User with all five rights (read 2026-11-04); a Locked amp, a login and a level changed at the front
  panel while connected have not been tried on hardware (the app sees such a change at the next
  connect only). Spec: BEH-13 to BEH-15
- How does the app see IPD changes made on the front panel? Polling the full state takes about
  16 s. Spec: BEH-38
- IPD mixer gain tops out at 0 dB while the mockup faders go to +12: show the model's range on the
  fader. Spec: BEH-27, UX-18, SEC-2
- Choose defaults for the user ceilings. Spec: SEC-3, BEH-40
- IPD MK2 units (different framing) have not been tried on hardware. Spec: BEH-1
- Confirm the IPD's limiter unit on the IPD 1200 and 4800. Settled on an IPD 2400 (2026-11-04): the
  amp stores dBV, 29.04 for 40 Vpk, 32.56 for 60 Vpk; the Windows app and the amp's front panel both
  show 31.26 and 34.78 dBu. Spec: BEH-5
- When there is access to an IPX 4800: read its limiter range in the Windows app (lowest and highest
  threshold in dBu and Vpk) and add it to `LIMITER_DBU` in `amps` (`ipx/params.rs`), which now holds
  the IPX 1200's range, 19.22 to 35.99 dBu (10 to 69 Vpk), for every IPX. Expected from the IPD
  ceilings plus 2.22 dB: about 138 Vpk (42.05 dBu) for the 4800 and 98 Vpk (39.04 dBu) for the 2400;
  read it, do not trust it. The range depends on the amp model, and the app has to
  know the model to use it. Spec: BEH-5, SEC-3
- IPX gain reduction: show the Levels limiter meter for the IPX. `gain_reduction_b` (meter indices
  10-11) is the output limiter, verified on out1 of an IPX 1200 (docs/ipx-protocol.md); wire it in.
  Still unchecked: that the second value is out2, and what `gain_reduction_a` shows (the compressor
  or the input limiter). The IPD has `gain_reduction_out`. Spec: BEH-28
- IPX discovery address: a live IPX 1200 reports its own IP as 192.168.5.200 in its status while it
  answers on 192.168.1.87. Discovery uses the IP text inside the IPX broadcast; if that is the
  amp's configured address, it can be unreachable from where the amp is heard. Prefer the sender
  address of the datagram, once a real broadcast has been seen (none was on this network). Spec: BEH-19
