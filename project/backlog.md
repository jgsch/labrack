# Backlog

Bugs, features, and chores small enough for one change, highest priority first.
Larger initiatives go to `roadmap.md`. Remove an item when its work is
committed.

## Bugs

From the full review of 2026-11-10, in order of priority. Each starts with a failing test.

Bugs:

- The app drops the refusals of the core (`let _ =` on `set`, `set_limiter`, `set_ip`, `rename`,
  `remove`): a duplicate IP, a linked channel 2 or a missing right shows nothing. Show them as
  toasts. `app/src/app.rs`. Spec: UX-26, UX-29
- An unreadable config file is reported on stderr only, and what the user adds is lost at exit.
  Show it in the window. `app/src/app.rs`. Spec: BEH-40

Unverified (test on the hardware first):

- IPX recall: the mutes are read straight after the recall, which has no reply. If the amp applies
  it late, an output may be unmuted while shown MUTED. Spec: SEC-6

Minor:

- IPD client: a write that times out removes a newer pending write of the same key; `get_many`
  returns cached values for keys that did not answer, and `Ok` after a disconnect.
- Config: ceilings and loads from the file are not validated (a `nan` ceiling disables it).
- The preset name box allows 24 characters, the amps 15 or 16: a long name mutes the outputs, is
  refused, and unmutes them.
- The preset list is read twice at once after a store (50 slow reads each on an IPD).
- `DeviceState::parse` decodes the IPX passwords into memory (SEC-8); nothing uses them.
- Docs: `docs/ipx-protocol.md:205` (a filter type write does not switch it on, IPX 1200 firmware
  2.3.4.0, seen 2026-11-10); BEH-37 says the limiter is skipped from files but it is restored; the
  last Chores item (IPD discovery on the default route only) looks fixed; the `Header` doc comment
  says UNMUTE ALL is disabled.

## Features

## Chores

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
- IPX gain reduction: the meters have two sets (`gain_reduction_a` / `_b`), both "inferred" in
  docs/ipx-protocol.md; which one is the output limiter is unknown, so the Levels limiter meter has
  nothing to show for the IPX yet. The IPD has `gain_reduction_out`. Spec: BEH-28
- IPX limiter: which of the four stored figures does the DSP limit on? An amp's out2 held a dBu that
  its Vpk does not give. The amps' front panels show and set dBu only, so the app takes the dBu as the
  threshold and derives the other figures from it; a threshold edit writes all four. A test with a
  signal above the threshold and the gain reduction meter would settle it. Spec: BEH-5
- IPX discovery address: a live IPX 1200 reports its own IP as 192.168.5.200 in its status while it
  answers on 192.168.1.87. Discovery uses the IP text inside the IPX broadcast; if that is the
  amp's configured address, it can be unreachable from where the amp is heard. Prefer the sender
  address of the datagram, once a real broadcast has been seen (none was on this network). Spec: BEH-19
- Discovery of the IPD only probes the /24 of the default route. On a machine with Wi-Fi to the amps
  and a wired default route (like the development machine) it finds nothing. Spec: BEH-18
