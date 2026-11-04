# Safety and security

What must be protected and how. Every rule here should have a test or a
verifiable check. Requirements use `SEC-<n>` IDs.

The app controls the volume of a sound system. A wrong value can damage speakers or hearing.

- **SEC-1** **Never write on connect.** On connect, reconnect and app start the app only reads the
  amp's state. It never pushes its cached or displayed state, and it has no write queue for amps
  that are not ONLINE.
- **SEC-2** **Hard limits.** Every value is checked against the model's range from the library
  before it is sent; values outside it, NaN and infinity are never sent. A value of the wrong kind
  (a bool for a gain) is refused, not converted. The IPX reports no ranges, so for the basic
  controls (gain, delay, EQ, crossover frequency, mixer level) it takes the IPD's.
- **SEC-3** **User ceilings.** Each amp can have user ceilings (maximum output gain, maximum
  limiter threshold) that the UI cannot exceed. A value above a ceiling is refused, not clamped,
  and the range a control shows ends at the ceiling. Lowering a ceiling writes nothing: a control
  that is above it stays until the user moves it.
- **SEC-4** **Small steps.** Faders follow `UX-22` and `UX-23`. While a fader moves, only the
  latest value is sent, at most every 50 ms per parameter.
- **SEC-5** **Confirm dangerous actions:** UNMUTE ALL, preset recall, loading a file (see
  `UX-24`).
- **SEC-6** **Bulk changes mute first.** Preset recall, file load and link copy mute the affected
  outputs first, write, then put the mutes right (the library's restore order), and no output ends
  unmuted that was muted before:
  - recall: the amp swaps its whole configuration, so an output ends muted if it was muted before
    or the preset mutes it, even if the stored preset says unmuted;
  - file load: output mutes in the file are never applied, and each output goes back to what it
    was;
  - what still waits to be written when a bulk change starts is dropped, not written over the new
    settings;
  - if anything fails after the outputs were muted, they stay muted and the user is told. A muted
    output is never the dangerous state.
  The window between the amp applying a preset that unmutes an output and the app muting it again
  is one round trip; the amps offer no way to avoid it.
- **SEC-7** **Never unmute automatically:** not after a reconnect, a preset recall, a file load or
  an app start.
- **SEC-8** **Network.** Neither firmware has real authentication, and both hand out passwords. The
  app assumes an isolated, trusted network, reads no passwords and stores none.
