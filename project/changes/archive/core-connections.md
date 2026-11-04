# core: connections and status

- Source: roadmap Phase 1

## Why

Each amp needs one owner for its connection that reports a clear status, reconnects on its own
and never writes when it connects.

## Requirements

Adds `BEH-42` (when and how often the app connects and retries) and edits `BEH-7` (the loaded
preset); implements:

- `ARCH-3`: one tokio task per amp; events into core, intents out through core.
- `BEH-8`: every status in the table, one at a time.
- `BEH-9`: DISCONNECT closes the connection and is remembered across restarts; CONNECT reopens.
- `BEH-10`: IP editable at any time (reconnects); no writes while not ONLINE.
- `BEH-12`: output level and gain reduction refreshed about every 100 ms (mute: see Notes).
- `SEC-1`: connect, reconnect and start only read; no write queue.
- `SEC-7`: nothing unmutes after a reconnect or an app start.
- `ARCH-6`: integration tests against the IPD simulator and the fake IPX.

## Tasks

- [x] Status state machine with a unit test per transition
- [x] Per-amp task: connect with the chosen driver, read state, forward meters and changes
- [x] Error mapping: `Error::Busy` → BUSY, timeout / refused → UNREACHABLE, bad IP → CHECK IP
- [x] Reconnect with backoff: RECONNECTING… after a loss, retries while UNREACHABLE / BUSY
- [x] Disconnect / connect intents, persisted in the config
- [x] Test harness that records every write the simulator and fake IPX receive; assert none on
      connect and reconnect
- [x] Meter stream to the UI at ~10 Hz

## Done when

- [x] Tests for ARCH-3, ARCH-6, BEH-8, BEH-9, BEH-10, BEH-12, SEC-1, SEC-7 pass

## Notes

- Depends on `core-amp-list`.
- `Hub` (`crates/core/src/hub.rs`) wraps the `Store` and owns the tasks. Intents go through it; the
  views read `amps()`, `status()`, `meters()` and `subscribe()` (`Update::{Status, Meters,
  Changed}`). A task is the only writer of its amp's status; `Hub::reconcile` makes the tasks match
  the list after every intent (start, restart on a changed IP or connected flag, end on remove).
- Retry timing is `Timing` (default 1 s, doubling to 15 s; meters every 100 ms), in BEH-42. The
  first attempt of a start, CONNECT or IP edit is immediate.
- The loaded preset is read with the state when the amp goes ONLINE and cleared when it is not
  (BEH-7). ONLINE means connected and `status()` read; a failed read counts as a failed attempt.
- Mute state is not part of this change: the IPD sends no mute changes on its own, so BEH-12's mute
  refresh needs polling and belongs to `core-mute-presets` (backlog: how the app sees front-panel
  changes on the IPD, BEH-38).
- Test doubles in `amps` (not feature-gated, like the IPD simulator): `SimServer::writes()`, and the
  fake IPX moved from `tests/ipx.rs` to `labgruppen::amps::ipx::fake` with `writes()` and a busy
  mode. A write is a SET on the IPD, and SETPRM, RECALLPRESET or SETPRESET on the IPX; the IPX
  heartbeat is not counted.
- `core` tests reach the doubles through a `Connector` that routes by the amp's IP; `Network` is the
  real one (`labgruppen::connect`).
- Not covered: an IPX that closes the connection because the vendor app holds it (BUSY) is only
  tested with the fake's busy mode, not against hardware (ARCH-8).
