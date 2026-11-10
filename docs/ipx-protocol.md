# Lab.gruppen IPX binary protocol

Derived from IPX Controller 2.3.4b (static analysis); verified on an IPX 1200 (MCU 2.3.4.0, DSP 2.3.2.0).

## Overview and transport

| Item | Value |
|---|---|
| Control | TCP, default port 50168 (0xC3F8), PC is client |
| Discovery | UDP 52350 (0xCC7E), amp -> PC broadcast, receive only |
| Byte order | little-endian everywhere in frames and payloads |
| Concurrency | one control client at a time; a second connection is accepted, then closed by the amp |
| Login | none needed for control or for reading the full state |

## Framing (identical on TCP and UDP)

```
off  size  field
0    4     sync     = 0xAA01AA02 (wire bytes 02 AA 01 AA)
4    4     len      = 4 + payload_len + 8, max 0x2800
8    2     main     command class (table below)
10   2     src      = 2 when sent by the PC
12   4     reserved = 0
16   4     sub      sub-header (meaning depends on main)
20   n     payload
20+n 4     checksum = (sum of sub + payload bytes) & 0xFF, stored as u32
24+n 4     trailer  = 0x11110123 (wire bytes 23 01 11 11)
```

Example, SETPRM to env 13, ch 0, item 0, field 1, value 1:
`02aa01aa 10000000 0100 0200 00000000 0d000001 01000000 0f000000 23011111`.

Receiver: scan the stream for the sync word, reject `len` < 12 or > 0x2800, require the trailer; a checksum field of 0 skips the checksum check.

| main | Name | Use |
|---|---|---|
| 1 | SETPRM | write one value, no reply |
| 2 | GETPRM | request (empty payload) or reply/push (4-byte or block payload) |
| 3 | SYSCTRL | control commands, sub = `u16 ctrlcmd, u16 0` |
| 4 | DEVBROADCAST | UDP discovery |
| 5 | DEVPRMCHAGE | parameter-changed notification (spelling as in the app) |

## Discovery (UDP 52350, main = 4)

Payload, little-endian:

| Offset | Type | Field |
|---|---|---|
| 0x00 | char[16] | id string |
| 0x10 | char[32] | device name |
| 0x30 | char[16] | IPv4 address as text (the app connects to this) |
| 0x40 | u16 | TCP control port |
| 0x42 | u16 | model/type (app maps 3 to 4, ignores values >= 4) |
| 0x44 | i32 | flag |

## Addressing

SETPRM/GETPRM/DEVPRMCHAGE sub-header: `u8 env, u8 channel, u8 item, u8 field`.
`field` is the dword index inside the block's struct; every 4-byte field of every block is addressable (verified). `channel` is the DSP channel 0/1 (A/B); `item` is the band, source or filter slot.
GETPRM with empty payload returns the same sub-header plus the value (4 bytes, or the whole block for block ids). SETPRM carries 4 bytes and is not acknowledged.

| env | Name | Notes |
|---|---|---|
| 1 | SYSATTR | char[16] model, char[32] serial |
| 2 | USER | |
| 3 | NETWORK | |
| 4 | AUDCFG | |
| 5 | MODEAES | |
| 6 | INPUT_MIXER | item = source: 0 analog1, 1 analog2, 2 aes1, 3 aes2 |
| 7 | INPUT_LEVEL | |
| 8 | INPUT_EQ | item = band 0-9 |
| 9 | INPUT_DELAY | |
| 10 | INPUT_COMPRESSOR | app name INPUT_RMSLIMITER |
| 11 | INPUT_RMS_LIMITER | app name INPUT_INLIMITER |
| 12 | OUTPUT_XOVER | item 0 highpass, 1 lowpass |
| 13 | OUTPUT_LEVEL | |
| 14 | OUTPUT_EQ | item = band 0-9 |
| 15 | OUTPUT_DELAY | |
| 16 | OUTPUT_COMPRESSOR | app name OUTPUT_RMSLIMITER |
| 17 | OUTPUT_LIMITER | peak limiter |
| 18 | ACK | password-change status (names only) |
| 19 | DEVCFG | device settings, see below |
| 20 | GAINSLEVEL | meters, 12 x f32, pushed |
| 21 | PRESET_PRMCFG | preset admin/developer passwords (XOR, see Security) |
| 22 | TEMPERATURE | i32, pushed |
| 23 | SWITCH_USER | |
| 24 | USERPASSWORD | set/check passwords (XOR) |
| 25 | DEV_PRM_SYNC | whole state, 0x6B0 bytes |
| 26 | PRESET_NOTIFY | |
| 27 | DEVICE_NAME | NUL-terminated string |
| 28 | ALL_TEMPERATURES | array of i32 |
| 29 | FIR_MODE | FiR-X models |

Env 0 NONE and 30 are unused; 31 STATUS is not used by the app.

## Value encoding

All values are 4 bytes little-endian. Booleans are i32 0/1. Strings are fixed char[16] (names, 32 for MAC/serial/ID), NUL-padded, Latin-1; a GETPRM of a name field returns the full 16 bytes.

f32 fields (everything else is i32): INPUT_MIXER f0, INPUT_LEVEL f0, OUTPUT_LEVEL f0, INPUT_EQ/OUTPUT_EQ f3 and f4, INPUT_DELAY/OUTPUT_DELAY f1, INPUT_COMPRESSOR/OUTPUT_COMPRESSOR f0 f4 f5, INPUT_RMS_LIMITER f0, OUTPUT_LIMITER f0-f3.
Example: -2.5 dB is `00 00 20 C0`.

## Parameter blocks (dword layout per struct)

| Block | Fields (dword index: name, type) | Units |
|---|---|---|
| INPUT_MIXER (per source) | 0 level f32, 1 mute, 2-5 name char[16] | dB |
| INPUT_LEVEL | 0 level f32, 1 mute, 2 link, 3-6 name | dB |
| OUTPUT_LEVEL | 0 level f32, 1 mute, 2 polarity inverted, 3 link, 4-7 name | dB |
| EQ band | 0 enabled, 1 type, 2 freq i32, 3 gain f32, 4 Q f32 | Hz, dB |
| DELAY | 0 unit i32, 1 delay f32 | unit enum not verified |
| COMPRESSOR | 0 threshold f32, 1 attack i32, 2 hold i32, 3 release i32, 4 ratio f32, 5 gain f32 | dB, ms |
| INPUT_RMS_LIMITER | 0 level f32, 1 release i32 | dB, ms |
| XOVER filter | 0 enabled, 1 type, 2 freq i32 | Hz (20001 = off) |
| OUTPUT_LIMITER | 0 Vpk f32 (setting), 1 load ohms f32, 2 watts f32, 3 dBu f32, 4 release i32 | V, ohm, W, dBu, ms |
| DEVCFG | 0 display mode, 1 lock unit, 2 power off, 3-4 unknown, 5 power-on preset (255 = last used), 6 read-only preset range, 7 auto power down, 8-10 access rights locked/user/admin | |

Watts is derived: Vpk^2 / (2 * ohms). Quirk (verified live): a single GETPRM of field 2 returns it as an i32 (595), the bulk blocks (DEV_PRM_SYNC, GETPRESET) carry it as f32 (595.0) (69 Vpk into 4 ohm = 595 W, matches the fixture). Watts and dBu are derived by the vendor app, not by the amp: all four fields are plain stored values (verified live on an IPX 1200, outputs muted, state restored afterwards). Writing Vpk (field 0) or ohms (field 1) leaves Watts and dBu unchanged; writing field 3 changes only field 3; linking copies out1's dBu over out2's. Field 2 accepts a float write (`ipx raw 17 1 0 2 --float 450`), reads back as the integer 450 in a single reply and as 450.0 in the bulk block.

dBu relation (matches every value seen, to 4 decimals): `dBu = 10 * log10(W * ohms / 0.6)` (0.7746 V squared is 0.6). The values written by the vendor app fit it from the watts figure the user typed: preset dBu 36.02 is 600 W into 4 ohm (while Vpk was stored as 69 and Watts as 595); live out1 35.9851 is 69 Vpk (595.125 W); live out2 34.7712 is 450 W (60 Vpk), although its Vpk and Watts now read 69 and 595, so something changed them without updating dBu.

The amp's front panel shows and sets dBu only (checked on an IPX 1200 and an IPD 2400; the IPD converts its stored dBV for the display), which suggests the dBu field is the one that counts. Which field the DSP limits on is still not known: it needs a signal above the threshold and the gain reduction meter. Writing all four fields consistently makes the question moot.
Amp-side value ranges were not determined; values outside the app's limits were not tried.

EQ type: 0 bell, 1 notch, 2 allpass, 3 low shelf, 4 high shelf, 5 band pass, 6 high pass, 7 low pass.

Crossover type: 0 Butterworth 6, 1 Bessel 6, 2 Butterworth 12, 3 Bessel 12, 4 Linkwitz-Riley 12, 5 Butterworth 18, 6 Bessel 18, 7 Butterworth 24, 8 Bessel 24, 9 Linkwitz-Riley 24 (dB/octave).

## DEV_PRM_SYNC (GETPRM env 25, 0x6B0 bytes)

| Offset | Content |
|---|---|
| 0x00 | char[16] current preset name |
| 0x10 | u32 preset index |
| 0x14-0x4B | not decoded |
| 0x4C | u32 current user mode |
| 0x50 + k*0x14 (k = 0..2) | password record: u32 level, char[16] password XORed with the fixed key (level of record 0 is not read; treated as 0) |
| 0x8C | network, 0x58 bytes: u32 dhcp, u32 port, char[16] ip, netmask, gateway, char[32] MAC |
| 0xE4 | audio config, 0x548 bytes (below) |
| 0x62C | device config, 11 x u32 (DEVCFG fields 0-10) |
| 0x65C | 3 x char[16]: build, MCU, DSP versions (MCU "2.3.4.0" on the verified unit) |
| 0x68C | char[32] device id |
| 0x6AC | u32 vendor locked |

### Audio config (dword indices from the start of the block; c = channel 0/1)

| Dword | Content | Size |
|---|---|---|
| 0x00 | operating mode (see below) | 1 |
| 0x01 | AES-to-analog | 1 |
| 0x02 + c*0x18 | mixer: 4 sources x {level, mute, name[16]} (6 dwords each) | 0x18 |
| 0x32 + c*7 | input level {level, mute, link, name[16]} | 7 |
| 0x40 + c*0x32 | input EQ: 10 bands x 5 dwords | 0x32 |
| 0xA4 + c*2 | input delay | 2 |
| 0xA8 + c*6 | input compressor | 6 |
| 0xB4 + c*2 | input RMS limiter | 2 |
| 0xB8 + c*6 | crossover: highpass (3 dwords) then lowpass (3) | 6 |
| 0xC4 + c*8 | output level {level, mute, polarity, link, name[16]} | 8 |
| 0xD4 + c*0x32 | output EQ: 10 bands x 5 | 0x32 |
| 0x138 + c*2 | output delay | 2 |
| 0x13C + c*6 | output compressor | 6 |
| 0x148 + c*5 | output limiter {Vpk, ohms, watts, dBu, release} | 5 |

Operating mode (IPX 1200, 2026-11-06): GETPRM/SETPRM env 5 (MODEAES), channel 0, item 0, field 0 is this dword. 1 = "stereo", 0 = "2 ways" (the front panel showed stereo when it read 1, and 2 ways after a write of 0, checked with the user at the amp). The write holds and changes nothing else in the audio config. Other numbers were not tried and may be other modes. No signal was available to check the routing.

Total 0x152 dwords = 0x548 bytes. The same audio config appears in a GETPRESET reply.

## Meters (GETPRM env 20, 48 bytes = 12 x f32, dB, about 10 Hz, pushed unsolicited)

| Index | Meter | Status |
|---|---|---|
| 0-1 | input signal ch A/B, before input gain | verified with live audio; shown as the ANALOG 1 and ANALOG 2 meters of the mixer (inferred) |
| 2-3 | "output signal" ch A/B (-150 when muted) | read -150 dB constantly on an IPX 1200 with the outputs unmuted and the amp playing (2026-11-05): not used for the output level |
| 4-5 | post input (after mixer and input gain): the levels of IN 1 and IN 2 | verified on an IPX 1200 (2026-11-05): analog 2 at -25.1 dB into both mixers with input gain -8 dB reads -33.1 dB on both |
| 6-7 | amp / output-stage meter: the output level | follows the signal and the output gain of each channel on a live IPX 1200 (-45 to -56 dB with input 2 at -20 dB; out1 gain -10 dB lower moved A by 10 dB, 2026-11-05) |
| 8-9 | gain reduction (compressor/limiter) set 1 | inferred |
| 10-11 | gain reduction set 2 | inferred |

TEMPERATURE (env 22) is pushed as one i32; ALL_TEMPERATURES (env 28) returns an i32 array (unit/sensor map not verified).

## Presets and control commands (main 3, sub = `u16 cmd, u16 0`)

| Cmd | Name | Payload | Reply | Status |
|---|---|---|---|---|
| 1 | HEARTBEAT | none | none awaited | sent every 2 s by the client |
| 3 | GETPRESET | u32 index | sub echoed; u32 ?, char[16] name @4, u32 index @0x14, audio config @0x48; an empty slot returns 0xFF bytes; index 50 gets no reply | verified |
| 2 | SETPRESET | char[16] name (NUL padded) + u32 index (wire), 20 bytes | none | verified: stores the current settings in the slot; the amp then reports that slot as its current preset (status shows its name and index) |
| 4 | RECALLPRESET | u32 index (wire) | none | verified (index 1 loads "base"; changes live audio incl. mutes) |
| 18 | GETALLPRESETNAME | none | N x char[16] (N = 50 on the test amp; the array index is the wire preset index 0-49, the other commands use the same index) | verified |
| 19 | GETVALIDPRESETS | none | N x u32, non-zero = slot used | verified |

Preset slots are indexed from the same list (index 1 = "base" on the test amp).
Defined in the app but not used here: 5 DELETEPRESET (tried on a stored slot with a u32 index and with the 20-byte store payload: no reply, preset stays; the vendor app only offers 14 DEL_ALLUSERPRESET, which erases everything and was not tried), 6 QUERYSTATUS, 7 DEVICE_REBOOT, 8 SETDEV_FACTORYINFO, 9-12 UPGRADE_*, 13 USER_LOGIN, 14 DEL_ALLUSERPRESET, 15 DEL_ALLFACTPRESET, 16 ACK, 17 FPLEDFLASH, 20 STOREONEPRESET_PKG, 21 PC_STORE_PRESET, 22 PC_RECALL_PRESET, 23/24 SET/GET_HARDWARE_CFG, 25 FORCE_FAN_ON, 26-28 INSTALL/GET/DEL_KEY, 29 PC_RECALL_PRESET_VLOCK, 30-33 FIR data, 34 GET_LOG.
The firmware upgrade and key commands should not be sent without understanding them.

## Session behaviour and quirks

- No handshake. After TCP connect the client may read an unsolicited frame; the app first requests DEV_PRM_SYNC.
- A busy amp accepts the connection and closes it immediately (EOF on first read, within about 0.5 s).
- A new connection opened within roughly 20 s of closing the previous one often gets no reply to its first GETPRM (timeout), even though nothing else is connected; the next attempt after a pause works. Seen when running one CLI command after another.
- SETPRM is never acknowledged; read back with GETPRM to confirm.
- Replies are matched by (main, sub). Meters and temperature arrive interleaved with replies and must be filtered out.
- Changes made elsewhere (front panel) arrive as GETPRM or DEVPRMCHAGE frames carrying the address and the new value.
- Timeouts used by the client: 2 s per request, 5 s for DEV_PRM_SYNC and GETPRESET.
- Writing a crossover or EQ band `type` does not switch that filter on (IPX 1200, firmware 2.3.4.0, 2026-11-10: tried on a disabled EQ band and on the disabled low-pass of output 2; both stayed off and only the type changed). An older note said it did. Writing `enabled` last when restoring settings is harmless and keeps the filter off until it is complete.
- Link (verified on the IPX 1200): setting the output link flag copies channel 1's settings to channel 2 once (gain, limiter dBu). After that the channels stay independent: writes to either channel are not mirrored, for inputs and outputs alike. A client that wants linked channels to follow each other must write both.
- An occasional GETPRM gets no reply (seen over Wi-Fi during a restore); retry before reporting a failure.
- TCP is a byte stream; frames can split or coalesce.
- FiR-X models use FIR_MODE and extra FIR commands; not covered.

## Verified on hardware vs inferred

| Verified on the IPX 1200 | Inferred from the vendor app only |
|---|---|
| framing, checksum, trailer; GETPRM/SETPRM on all block fields (re-verified with the Rust client: 288-parameter bulk dump, writes of float/int/enum/bool fields, full restore) | UDP discovery payload (broadcasts were never seen on the test network) |
| DEV_PRM_SYNC layout, password XOR decoding | meter indices 6-11 |
| single-client behaviour, no login needed | DELAY unit enum, ALL_TEMPERATURES layout |
| meters 0-5, temperature push | USERPASSWORD item/field usage, SWITCH_USER |
| GETPRESET, GETALLPRESETNAME, GETVALIDPRESETS, SETPRESET (store), RECALLPRESET | DEVPRMCHAGE behaviour, all other SYSCTRL commands |

## Security notes

- The control port has no authentication; any host that can reach TCP 50168 can change levels, mutes, EQ, limiters and presets.
- Passwords are readable: DEV_PRM_SYNC returns every user level's password XORed with a fixed key (same operation encodes and decodes). USERPASSWORD and PRESET_PRMCFG payloads use the same key.
  Key: `3a 7f 92 4b 1d e8 6c 55 a3 0f 78 9b 2e c1 4d 8a`. The password is NUL-padded ASCII before XOR (so the padding bytes on the wire equal the key bytes). Factory defaults: see device.
- Preset and factory files carry passwords in plaintext in their body.
- The single-client limit is a weak guard only: whoever connects first holds the amp, and a stray connection locks out IPX Controller.
- Keep the amp on an isolated management network.
