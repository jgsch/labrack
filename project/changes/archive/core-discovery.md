# core: discovery

- Source: roadmap Phase 1

## Why

Most amps should appear on their own, without sending traffic for a protocol the user switched
off.

## Requirements

Edits `BEH-18` and `BEH-19` (scan duration, what is searched and how, default names); implements:

- `BEH-17`: IPD / IPX switches; nothing is sent or listened for on a protocol that is off.
- `BEH-18`: switching a protocol on starts a scan; SCAN NOW runs a short scan.
- `BEH-19`: found amps are added as AUTO; a known IP is never added twice.

## Tasks

- [x] Scan service using `amps::ipx::discover` and the IPD listen / probe functions
- [x] Merge results into the amp list (dedupe by IP, AUTO source, default names)
- [x] Switch state persisted in the config; scan disabled when both are off
- [x] Tests: merging, no duplicates, AUTO vs MANUAL, no scan when off

## Done when

- [x] Tests for BEH-17 to BEH-19 pass

## Notes

- Depends on `core-amp-list`.
- Decided with the product owner: scan at app start; only the default-route /24 is probed for the
  IPD (no interface listing); the scan window stays 5 s.
- `Finder` (`crates/core/src/discovery.rs`) searches one protocol for a window; `Lan` is the real
  one, tests use a fake that records what was searched (as `Connector` does for connections). The
  `Hub` owns the switches, the scans and the merge: `set_switches`, `scan_now`, `scanning`,
  `switches`, and `Update::Scan(Started | Finished { added, errors })`.
- SCAN NOW and the IPD switch use both IPD ways: listen for the UDP announcements and probe the
  /24 (the vendor app probes; the announcements were never seen on the test network). The subnet is
  that of the computer's default route, found by connecting a UDP socket, which sends nothing. An
  amp on another interface is not found; manual add is the fallback (BEH-20). Not
  planned.
- IPX: the address in the broadcast is used, the sender's address if that is not IPv4.
- A scan lasts 5 s (`Timing::scan_window`): the library's CLI probes for 3 s, and the IPD can need
  a few seconds over Wi-Fi.
- The app scans the protocols that are on when it starts (`Hub::new`), so a first start finds amps
  without a click. A subscriber created after `new` misses that scan's `Started` / `Finished`; it
  reads `scanning()` and the list.
- Two scans can run together when a switch goes on during one; the merge makes that safe. A scan
  does not keep the hub alive.
- Not covered by tests: the real IPD side of `Lan` (probing a subnet needs port 50675 and a real or
  simulated amp on a /24; ARCH-8 manual check) and a real IPX broadcast (the test sends one over
  loopback).
