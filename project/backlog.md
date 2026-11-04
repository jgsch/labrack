# Backlog

Bugs, features, and chores small enough for one change, highest priority first.
Larger initiatives go to `roadmap.md`. Remove an item when its work is
committed.

## Bugs

## Features

## Chores

Open questions to resolve; each blocks or shapes the requirements listed.

- IPD login: does the firmware block writes at User or Locked level, and does the level survive a
  reconnect or power cycle? Spec: BEH-13 to BEH-15
- How does the app see IPD changes made on the front panel? Polling the full state takes about
  16 s. Spec: BEH-38
- IPD mixer gain tops out at 0 dB while the mockup faders go to +12: show the model's range on the
  fader. Spec: BEH-27, UX-18, SEC-2
- Choose defaults for the user ceilings. Spec: SEC-3, BEH-40
- IPD MK2 units (different framing) have not been tried on hardware. Spec: BEH-1
- **IPD limiter display may be about 2.2 dB off, before any UI shows Vpk or Watt for an IPD.**
  Read on a live IPD 2400 (MK1, fw 1.2.6): it stores 32.56 dBu on both outputs, while the Windows
  app shows 34.78 dBu / 60 Vpk for them. The Windows app also allows 19.22 to 39.04 dBu (10 to 98
  Vpk) on the IPD 2400, where the library (vendor config) says 16.78 to 36.78: 2.44 and 2.26 dB
  lower. The IPX shows no such offset (69 Vpk is 35.99 dBu). If the app converts the stored value
  with the formula alone, it shows 47 Vpk for what the vendor app calls 60 Vpk, and a threshold typed
  in Vpk would be stored about 2.2 dB higher than meant. Needed: a few more pairs (set a threshold in
  the Windows app, read `labgruppen get out1.limiter.threshold`), at the floor, the ceiling and
  between, to tell an offset from a different formula. Also the IPD 1200 and 4800. Spec: BEH-5,
  SEC-3
- When there is access to an IPX 4800: read its limiter range in the Windows app (lowest and highest
  threshold in dBu and Vpk) and add it to `LIMITER_DBU` in `amps` (`ipx/params.rs`), which now holds
  the IPX 1200's range, 19.22 to 35.99 dBu (10 to 69 Vpk), for every IPX. Do the same for the IPX 2400
  (guess from the IPD 2400: about 98 Vpk). The range depends on the amp model, and the app has to
  know the model to use it. Spec: BEH-5, SEC-3
- IPX gain reduction: the meters have two sets (`gain_reduction_a` / `_b`), both "inferred" in
  docs/ipx-protocol.md; which one is the output limiter is unknown, so the Levels limiter meter has
  nothing to show for the IPX yet. The IPD has `gain_reduction_out`. Spec: BEH-28
- IPX limiter: which of the four stored figures does the DSP limit on? The out2 of the live amp held
  a dBu that its Vpk does not give. Until known, the app shows the figures derived from Vpk and
  load, and a threshold edit writes all four. Spec: BEH-5
- IPX discovery address: a live IPX 1200 reports its own IP as 192.168.5.200 in its status while it
  answers on 192.168.1.87. Discovery uses the IP text inside the IPX broadcast; if that is the
  amp's configured address, it can be unreachable from where the amp is heard. Prefer the sender
  address of the datagram, once a real broadcast has been seen (none was on this network). Spec: BEH-19
- Discovery of the IPD only probes the /24 of the default route. On a machine with Wi-Fi to the amps
  and a wired default route (like the development machine) it finds nothing. Spec: BEH-18
