# core: channel link

- Source: roadmap Phase 1

## Why

Linking behaves differently on the two families; the app hides the difference.

## Requirements

Edits `BEH-31` (what follows, what does not, channel 2 cannot be edited) and `BEH-32` (how linking
copies, the same on both families); implements:

- `BEH-31`: channel 1 is the master of a linked pair.
- `BEH-32`: IPD writes channel 1 only and never channel 2 while linked; IPX mirrors every
  channel 1 write to channel 2.
- `BEH-33`, `BEH-34`: one shared EQ and one fader / limiter in the views when linked.
- `BEH-35`: unlinking keeps the current values.
- `SEC-6`: the link copy mutes first.

## Tasks

- [x] Link intents for inputs and outputs; the copy runs through the bulk change runner
- [x] Write routing per family; test that no channel 2 write reaches a linked IPD simulator
- [x] Fake IPX test: every channel 1 change also lands on channel 2
- [x] View model exposes a linked pair as one channel
- [x] `amps`: the IPD simulator mirrors a linked pair and counts channel 2 writes; the fake IPX copies
      on link once, as documented

## Done when

- [x] Tests for BEH-31 to BEH-35 pass

## Notes

- Depends on `core-dsp-control` and `core-mute-presets`.
- API (`Hub`): `set_link(id, Side, bool)` (async), `linked(id, Side)`, `channels(id, Side)` (the view
  model: one channel when linked, two otherwise). `Error::Linked` for a channel 2 edit of a linked
  pair. `Hub::set` and `set_limiter` route the writes; `Hub::set` still refuses the link flag.
- **What follows** (`crate::link::follows`): gain, delay, EQ, output filters, output limiter. The
  mockup's `mirror()` mirrors the same (EQ, processing, levels) and not the mixer. **The mute is not
  part of the link**, so that linking can never unmute (SEC-7); a user's unmute of output 1 does not
  unmute output 2. **Not verified on hardware: whether the IPD firmware mirrors a mute by itself.** If
  it does, the app's 100 ms mute poll shows it, but the families would then differ.
- **Linking is the same procedure on both families**: read channel 1 fresh, check every value
  (ranges, ceilings: a copy cannot raise channel 2 past a ceiling; one failure and nothing is
  written), then under the bulk runner (`Effect::Keeps`, outputs muted and put back) write channel 2
  with the link still *off*, and only then switch the link on. So the app does not depend on what the
  amp copies by itself (the IPX copies gain and limiter dBu only; whether the IPD copies at link time
  is unknown), and a linked IPD is never written on channel 2. Mutation-tested.
- **Linked edits.** IPD: channel 1 only; the amp mirrors, and the app's state follows (`observed`
  mirrors the cache of a linked IPD pair, and tells the views). IPX: channel 1 and channel 2 in one
  batch. The IPD limiter load, which is the app's, is set for both outputs while linked.
- **Files.** A file loaded into a linked pair never writes channel 2: its channel 2 entries are
  skipped, and on the IPX channel 2 gets channel 1's value from the file, so the pair stays equal.
- **The fixture is a linked amp**: the real IPX 1200 dump has its inputs linked. The DSP test that
  writes `in2.*` now unlinks first, as a user would have to.
- Link flag: on the first channel of the pair for both families (`in1.link`, `out1.link`). The IPX has
  a flag per channel; whether the vendor app sets both is not known, and only the first is used.
- Not covered: a change made on the IPX front panel to channel 1 of a linked pair is not copied to
  channel 2 (the app only copies what it writes); said in `BEH-32`. Reacting to such an event would
  mean the app writing by itself on an amp's say-so, which `SEC-1` is wary of.
- Unlinking is a single flag write and changes no value; the cache keeps both channels' values.

## Revised after the first review

- Read on a real IPX 1200: the link flag is set on channel 1 only (`in1.link`, `out1.link` on; the
  channel 2 flags off), as implemented. Its inputs and outputs were linked and equal.
- Linking a pair that is already linked does nothing on an IPD (it used to copy over channel 2).
  On an IPX it syncs a drifted pair.
- `Hub::link_drift(id, side)` lists the settings of channel 1 that channel 2 does not have (within
  0.2 %), for the UI to warn about. An IPD cannot drift.
- **A drift is followed, not synced.** When the amp reports a change to channel 1 of a linked IPX
  pair (front panel), the app writes the same value to channel 2 and emits `Update::LinkCorrected`.
  A pair found out of step when the state is read, or a correction refused by a range or a ceiling, is
  only reported (`Update::LinkOutOfSync`, `Hub::link_drift`): connecting writes nothing (SEC-1). There
  is no sync action; linking a linked pair is a no-op on both families, and unlinking then linking
  makes the channels equal.

## Second revision: show what is stored, correct nothing

The auto-correction is gone (`LinkCorrected`, `LinkOutOfSync`). A pair the amp flags linked while its
channels differ is **shown as two unlinked channels** with the stored values (`Hub::linked` false,
`Hub::link_diverged` true, `Update::LinkChanged`), whether found when the state is read or reported by
the amp; the app writes nothing. The state comes only from what the amp reports, never from the app's
own writes, so editing a linked pair cannot make it flicker (tested by event). It is shown linked again
if the channels become equal. LINK on such a pair copies channel 1 to channel 2; UNLINK clears the flag.
