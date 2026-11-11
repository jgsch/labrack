# Review fixes

- Source: full review of 2026-11-11 (findings 1 to 3)

## Why

- A confirmation (SWITCH OFF band or HPF, RAISE, CONFIRM RESET / ALL OFF) stayed open or armed when the
  user switched tab or amp, and then acted on the new target.
- A MUTE waited for every write queued before it (0.5 s on an IPX, 2 s on an IPD behind an EQ RESET).
- With the picked amp offline or unread the shield covered the amp picker, so no other amp could be picked.

## Requirements

- Modify `UX-24`: a question or an armed button belongs to one amp and one tab.
- Modify `UX-29`: the amp picker is outside the shield.
- Modify `SEC-4`: queued writes go out in chunks of 8, a mute first, giving the amp up between chunks.

## Tasks

- [x] Tests: chunked `Coalescer`, mute order behind a big batch on the IPD simulator, picker outside the
  shield, panel keys
- [x] `Coalescer::take_some`, `Writer::take_chunk`; panels keyed by amp and tab; shield moved under the picker

## Done when

- [x] Tests for `SEC-4`, `UX-24`, `UX-29` pass
- [x] `mise run lint` and `mise run test` pass

## Notes

- Not changed: the IPD idle MUTE latency (0.35 to 1.1 s measured). The mute polling holds the write gate,
  but releasing it did not help on the simulator: the IPD client handles one exchange at a time, so a MUTE
  waits for the one in flight. Left as it was.
