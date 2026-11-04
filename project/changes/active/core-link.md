# core: channel link

- Source: roadmap Phase 1

## Why

Linking behaves differently on the two families; the app hides the difference.

## Requirements

No spec change; implements:

- `BEH-31`: channel 1 is the master of a linked pair.
- `BEH-32`: IPD writes channel 1 only and never channel 2 while linked; IPX mirrors every
  channel 1 write to channel 2.
- `BEH-33`, `BEH-34`: one shared EQ and one fader / limiter in the views when linked.
- `BEH-35`: unlinking keeps the current values.
- `SEC-6`: the link copy mutes first.

## Tasks

- [ ] Link intents for inputs and outputs; the copy runs through the bulk change runner
- [ ] Write routing per family; test that no channel 2 write reaches a linked IPD simulator
- [ ] Fake IPX test: every channel 1 change also lands on channel 2
- [ ] View model exposes a linked pair as one channel

## Done when

- [ ] Tests for BEH-31 to BEH-35 pass

## Notes

- Depends on `core-dsp-control` and `core-mute-presets`.
