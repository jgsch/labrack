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
  limiter threshold, in the dBu the amp stores) that the UI cannot exceed. The limiter ceiling holds in every unit and
  in settings files. A value above a ceiling is refused, not clamped,
  and the range a control shows ends at the ceiling. Lowering a ceiling writes nothing: a control
  that is above it stays until the user moves it. What the amp holds above a ceiling, because of a
  recalled preset or a change made elsewhere, is told in a toast naming the amp, output, value and
  ceiling whenever the state is read (connect, recall, file load), before the user unmutes.
  That toast stays until the user dismisses it and is not replaced by an error. An IPX limiter is judged by its
  dBu field, and a value outside the model's range is told as well.
- **SEC-4** **Small steps.** Faders follow `UX-22` and `UX-23`. While a fader moves, only the
  latest value is sent, at most every 50 ms per parameter. A gesture cannot jump a level: no key
  auto-repeat, at most 6 dB up per drag, one wheel raise per 200 ms.
- **SEC-5** **Confirm dangerous actions:** UNMUTE ALL, and every action that mutes the outputs of
  an amp: preset recall, storing a preset, loading a file, linking channels, copying an EQ, changing the device mode (see `UX-24`).
- **SEC-6** **Bulk changes mute first and leave the outputs muted.** Preset recall, file load and
  storing a preset mute both outputs of the amp first, change the amp, and leave both outputs
  muted, whatever they were and whatever the preset or file says. Unmuting is always the user's
  decision (`SEC-7`).
  - A preset the app stored carries muted outputs (storing mutes them first), so recalling it has no
    unmuted moment at all.
  - A preset stored by another tool may have unmuted outputs. The amp applies a recall at once,
    mutes included, so an output it unmutes passes audio with the preset's settings until the app has
    muted it again, one round trip (measured on an IPD 2400 over Wi-Fi: 0.4 to 0.6 s with a read of the mutes in between, so
    the app writes the two mutes at once after an IPD recall, without reading them first). The app
    cannot prevent it.
  - File load never applies output mutes from the file.
  - What still waits to be written when a recall or a load starts is dropped, not written over the
    new settings.
  - If anything fails after the outputs were muted, they stay muted and the user is told. If the amp
    refuses the change before sending anything (a bad name, a protected slot), the app takes back
    only the mute it added.
  - An unmute asked for while a bulk change runs on the amp is refused, not queued: it would be written
    after the change. Turning a mute on is never refused.
  - Link copy mutes first too, and puts the outputs back as they were: it changes no scene.
  A muted output is never the dangerous state.
- **SEC-7** **Never unmute automatically:** not after a reconnect, a preset recall, a file load, a
  store or an app start. Only an action of the user on that output (or UNMUTE ALL) unmutes.
- **SEC-8** **Network.** Neither firmware has real authentication, and both hand out passwords. The
  app assumes an isolated, trusted network, reads no passwords and stores none.
