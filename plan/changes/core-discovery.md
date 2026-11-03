# core: discovery

- Source: roadmap Phase 1

## Why

Most amps should appear on their own, without sending traffic for a protocol the user switched
off.

## Requirements

No spec change; implements:

- `BEH-17`: IPD / IPX switches; nothing is sent or listened for on a protocol that is off.
- `BEH-18`: switching a protocol on starts a scan; SCAN NOW runs a short scan.
- `BEH-19`: found amps are added as AUTO; a known IP is never added twice.

## Tasks

- [ ] Scan service using `amps::ipx::discover` and the IPD listen / probe functions
- [ ] Merge results into the amp list (dedupe by IP, AUTO source, default names)
- [ ] Switch state persisted in the config; scan disabled when both are off
- [ ] Tests: merging, no duplicates, AUTO vs MANUAL, no scan when off

## Done when

- [ ] Tests for BEH-17 to BEH-19 pass

## Notes

- Depends on `core-amp-list`.
- IPD discovery: the library has an announcement listener and a /24 scan; decide which SCAN NOW
  uses and how the subnet is chosen.
- How long is "a short time"? Pick a duration.
