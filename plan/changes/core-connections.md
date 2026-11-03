# core: connections and status

- Source: roadmap Phase 1

## Why

Each amp needs one owner for its connection that reports a clear status, reconnects on its own
and never writes when it connects.

## Requirements

No spec change; implements:

- `ARCH-3`: one tokio task per amp; events into core, intents out through core.
- `BEH-8`: every status in the table, one at a time.
- `BEH-9`: DISCONNECT closes the connection and is remembered across restarts; CONNECT reopens.
- `BEH-10`: IP editable at any time (reconnects); no writes while not ONLINE.
- `BEH-12`: output level, gain reduction and mute refreshed about every 100 ms.
- `SEC-1`: connect, reconnect and start only read; no write queue.
- `SEC-7`: nothing unmutes after a reconnect or an app start.
- `ARCH-6`: integration tests against the IPD simulator and the fake IPX.

## Tasks

- [ ] Status state machine with a unit test per transition
- [ ] Per-amp task: connect with the chosen driver, read state, forward meters and changes
- [ ] Error mapping: `Error::Busy` → BUSY, timeout / refused → UNREACHABLE, bad IP → CHECK IP
- [ ] Reconnect with backoff: RECONNECTING… after a loss, retries while UNREACHABLE / BUSY
- [ ] Disconnect / connect intents, persisted in the config
- [ ] Test harness that records every write the simulator and fake IPX receive; assert none on
      connect and reconnect
- [ ] Meter stream to the UI at ~10 Hz

## Done when

- [ ] Tests for ARCH-3, ARCH-6, BEH-8, BEH-9, BEH-10, BEH-12, SEC-1, SEC-7 pass

## Notes

- Depends on `core-amp-list`.
- Retry intervals for UNREACHABLE and BUSY are not specified; pick and document them.
- The IPX heartbeat is a SYSCTRL frame, not a parameter write; the "no write" check counts
  SETPRM, preset and store commands.
- The fake IPX lives in `crates/amps/tests/ipx.rs`; core integration tests need it exported
  (e.g. a `test-util` feature or a shared test crate).
