# Performance and compatibility

Measurable budgets and supported environments. Each budget states how it is
measured. Requirements use `PERF-<n>` IDs.

- **PERF-1** Linux (x86_64; aarch64 builds from the same flake), Wayland and X11. Dynamically
  linked; no static build.
- **PERF-2** Meters at about 10 Hz for 16 amps without visible lag. (Target to measure, not yet
  verified.)
- **PERF-3** Startup to a usable window under 2 s. (Target to measure, not yet verified.)
- **PERF-4** Works offline: fonts and assets are bundled.
