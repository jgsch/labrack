# Architecture and data flow

How the system is built. Requirements use `ARCH-<n>` IDs.

## Components and data flow

- **ARCH-1** Language: Rust. GUI: Dioxus desktop (0.7, a WebKitGTK window). The mockup's CSS is
  reused; its markup is converted to RSX; all logic is Rust.
- **ARCH-2** Cargo workspace with three crates, each in `crates/<name>/`:
  - `amps`: the protocol library and test CLI (`Amp` trait, IPX and IPD drivers, IPD
    simulator).
  - `core`: everything that is not drawing: amplifier list, connection state machine,
    discovery, safety rules, link mirroring, MODIFIED tracking, TOML config. No UI dependency, so
    it is tested directly.
  - `app`: the Dioxus app; turns `core` state into views and user input into `core`
    intents.
- **ARCH-3** One tokio task per amplifier owns its connection. Amp events (meters, changes,
  disconnects) go to `core`; user intents go through `core`, which validates and limits
  them before anything is sent.

## Testing

Test-first: a behaviour gets its failing test before the code.

- **ARCH-4** `amps`: golden vectors from the vendor's own IPD code, IPX captures, a fake IPX
  amp and the IPD simulator. New protocol findings get a test that reproduces them (the fake amps
  behave like the real ones).
- **ARCH-5** `core` unit tests: status transitions, IP validation and duplicates, discovery
  merging (no duplicates, AUTO vs MANUAL), link mirroring per family, MODIFIED tracking, hard
  limits and user ceilings, write rate limiting, mute ordering of bulk changes, config
  round-trips.
- **ARCH-6** `core` integration tests against the IPD simulator and the fake IPX: no write on
  connect, BUSY, UNREACHABLE, reconnect, never unmute after reconnect or recall.
- **ARCH-7** UI: screenshots of the app compared with the mockup boards (`mise run mockup`,
  ImageMagick).
- **ARCH-8** Live hardware (IPD2400, IPX 1200): manual, outputs muted, amp state dumped before and
  restored after.

## Development environment

- **ARCH-9** A Nix flake (`flake.nix`, with `flake.lock` committed) defines the development shell,
  entered with `nix develop`. It provides every tool the mise tasks need:
  - the Rust toolchain from rust-overlay (compiler, `cargo`, `clippy`, `rustfmt`,
    `rust-analyzer`, `rust-src`); the Rust version is set in `flake.nix` only;
  - `mise`, which runs the tasks;
  - `dx` (dioxus-cli) and the WebKitGTK / GTK libraries with `pkg-config`;
  - Chromium (headless) to screenshot the mockup, ImageMagick to compare screenshots, Python 3
    for `scripts/mockup.py`.
- **ARCH-10** Tasks (`.mise.toml`): `fmt`, `lint` (format check and clippy for both drivers and
  each alone, warnings are errors), `test`, `build`, `app`, `sim`, `mockup`, `app-shot`,
  `mockup-unpack`, `mockup-pack`.
- **ARCH-11** Formatting settings live in the `fmt` / `lint` tasks (no `rustfmt.toml`). Editors:
  pass the same `--config max_width=110,use_small_heuristics=Max` to rustfmt (rust-analyzer:
  `rust-analyzer.rustfmt.extraArgs`).
- **ARCH-12** The reverse-engineering environment is separate: `re/flake.nix` (`nix develop ./re`).
