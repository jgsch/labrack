# Lab.gruppen IPD ("IntelliDrive") binary protocol

Derived from IntelliDrive Controller 1.2.11 (decompiled Python app); verified on an IPD2400 MK1, firmware 1.2.6, over Wi-Fi. Covers IPD 1200 / 2400 / 4800.

## Overview and transport

| Item | Value |
|---|---|
| Control | TCP 50675, PC is client; strict request/response, one reply per command |
| Announcements | UDP 50674, amp -> PC, 42-byte `DDB...Q` packets |
| Probe | the app sends `4DPP` (below) over TCP to every IP of the subnet |
| Bootloader | UDP 57383, app broadcasts a TFTP-like request; firmware-update path, not covered |
| Byte order | big-endian for values; ASCII tags in frames |
| Models | Global MODEL_NUMBER 0 = IPD2400, 1 = IPD1200, 2 = IPD4800; unit type `27_0_1` |
| Hardware generations | MK1 (36-byte packets) and MK2 (long packets) |

All three models share one parameter map; only output limiter/compressor ceilings and meter ranges differ.

## Discovery

Probe, sent on a TCP connection: `34 44 50 50 FD 00 FF 00 <cs>` where `cs` = 0xAA XOR all previous bytes.
Expected reply: 42-byte announcement:

| Offset | Content |
|---|---|
| 0-2 | `DDB` |
| 4 | type2 |
| 5-21 | MAC as 17 characters of text |
| 22-24 | hardware version (3 bytes) |
| 25-27 | PIC version (3 bytes) |
| 28-30 | serial, big-endian 24 bit |
| 31 | year - 2000 |
| 32 | month |
| 33-35 | unit type (27, 0, 1) |
| 36 | day (MK1) |
| 37 | DSP state |
| 39 | PC packet id |
| 40 | HW packet id |
| 41 | `Q` |

MK2 units carry the ASCII marker `67890Q` at bytes 36-41.
Hardware finding: the IPD2400 MK1 (fw 1.2.6) answers the probe with 42 bytes `DDP` or `DDC` + 36 constant undecodable bytes + pc_id + hw_id + `Q`, not a `DDB`. Treat any such reply as "unit present, MK1 unless the MK2 marker is there". No UDP announcements were received on 50674 (possibly because of Wi-Fi).

## Framing

### Checksum and CodeTable

CodeTable (34 entries): 233 45 98 215 123 42 99 23 178 132 200 150 100 50 77 43 54 90 31 43 123 234 211 177 65 133 3 64 23 83 251 1 54 34.
Command checksum: seed 0xAA, XOR with every preceding byte of the command. Reply checksum: seed 0xD5, XOR with every body byte before the `09` opcode.

### PC -> amp command ("J")

```
MK1: 05 'D' 'D' 'J' | fragments (XOR CodeTable[i % 32]) | FD hw_id FF my_id | cs
MK2: lenH lenL 'D' 'J' | fragments (XOR CodeTable[i % 32]) | FD hw_id FF my_id | cs
     len = 4 + total fragment bytes (big-endian)
```

`i` counts from 0 at the first fragment byte. `hw_id` = id from the last reply (or the NACK), `my_id` cycles 1..254. An empty command (no fragments) is valid and is used for polling.
Fragment budget: MK1 one fragment per packet (max 8 bytes of fragments), MK2 about 500 bytes.

| Fragment (before XOR) | Layout |
|---|---|
| GET | `03 07 struct member channel num 6A` ('j') |
| SET | `04 (6+n) struct member channel num value[n]` |
| FACTORY SET | as SET with opcode `0A` |

FIR keys (struct 15, member bit 7 set): on send `member += num / 256`, `num %= 256`; on receive `num += 256 * (member & 0x7F)`, `member = 128`.

### amp -> PC replies

| Kind | Layout |
|---|---|
| MK1 data `DDA` | `'DDA'` + 36 bytes (XOR CodeTable[i % 32]) + pc_id + hw_id + `Q` = 42 bytes. If the 36 bytes start with `00 00` the packet is empty |
| MK2 data `DDW` | `'DDW'`, 4 bytes (unused), [7] pc_id, [8..9] body length (big-endian), body XOR CodeTable[i % 34]; total 10 + length, no trailing `Q` |
| Announcement | `DDB`, see Discovery |
| NACK (busy/error) | `DDN`, `DDM` or `DDC`, 42 bytes with pc_id at [39], hw_id at [40]; short form of 10 bytes with an error code at [5] |

Body = fragments `opcode, size (including this 2-byte head), data...`, ending with `09 cs`. A packet without a valid checksum fragment is discarded. Opcode `00` ends the body; opcode `0F` has no size byte and 513 data bytes.
MK1 firmware quirk: if the first decoded body byte is 233 (wire byte 0), the opcode is 9 when body[1] = 213, otherwise 5 (RSP); for 5, if the opcode at body[body[1]] is 2 the fragment is 1 (VU in), if 8 it is 7 (GR in).
Like the vendor app, the library takes the MK2 hw_id from byte 8 (the length high byte); unverified on hardware.

### Opcodes

| Op | Name | Data |
|---|---|---|
| 01 | VU in | int8 dB per input |
| 02 | VU out | int8 dB per output |
| 03 | GET | `struct member channel num 'j'` |
| 04 | SET | `struct member channel num value` |
| 05 | RSP | `struct member channel num value` (reply and echo) |
| 06 | ERR | error fragment |
| 07 / 08 | GR in / GR out | int8 dB gain reduction |
| 09 | checksum | 1 byte |
| 0A | FACTORY SET | as SET |
| 0B | raw VU in | int8 |
| 0C | output V/I | pairs (V, I) per output; V dB = b/4 - 3.75, I dB = b/4 - 13.75 |
| 0D / 0E | inclination / orientation | sensor data |
| 0F | activity log | 513 bytes, no size byte |
| 62 / 63 | RS-485 slave list / id | not used |

## Addressing

Key = (struct, member, channel, num). Channel: inputs A/B = 0/1, outputs 1/2 = 0x80/0x81; elsewhere channel is a slot (preset 0 = current, 1..100; connector 0-3; user level 1-5). `num` is the band (0-9), mixer source (0-3) or link index.

| Struct | Name | Struct | Name |
|---|---|---|---|
| 0 | gain | 8 | invert |
| 1 | mixer | 9 | preset global |
| 2 | delay | 10 | link |
| 3 | LPF | 11 | command |
| 4 | HPF | 12 | compressor |
| 5 | PEQ | 13 | global |
| 6 | limiter | 14 | channel |
| 7 | mute | 15 / 32 | FIR / UI control |

## Value encoding (big-endian, raw = value x scale)

| Member | Name | Format | Scale |
|---|---|---|---|
| 0 | GAIN | i16 | 100 (dB) |
| 1 | FREQ | u16 | 1 (Hz) |
| 2 | Q | u16 | 10 |
| 3 | TYPE | u8 | |
| 4 | THRESHOLD | i16 | 100 (dB or dBu) |
| 5 / 6 / 7 | ATTACK / RELEASE / HOLD | i16 | 1 (ms) |
| 8 | DELAY | u32 | 1000 (ms to microseconds) |
| 9 | STATUS | u8 | |
| 14 / 15 / 24 / 25 | MUTE / INVERT / ON / LINK | u8 | 0/1 |
| 16 / 17 | SHORT_NAME / LONG_NAME | char[16], space padded | |
| 18 | PRESET_NUMBER | u32 | |
| 26 | COMMAND | u8 | |
| 27 | RATIO | i16 | 10 |
| 28 / 29 | SET_PASSWORD / CHECK_PASSWORD | char[8] | |
| 30 / 31 | ACCESS_RIGHTS / USER_ACCESS_RIGHTS | u32 bitmask | |
| 33 | SW_VERSION | u32 = major<<16 \| minor<<8 \| patch | |
| 34 | STANDBY_DELAY | u32 | 60000 (minutes to ms) |
| 35 | HARDWARE_STATUS | u32 flags | |
| 36 | CONFIGURATION | u32 | |
| 37 / 39 | AMP_READINGS / _2 | u32 | |
| 38 | CURRENT_USER | u32 | |
| 40 / 87 | SUBNET_MASK / IP | u32 | |
| 43 / 44 | STARTUP_PRESET / INPUT_SELECT | u32 | |
| 45 | AES_FS | u32 | 100 |
| 46 | CONNECTOR_NAME | char[16] | |
| 48 / 49 / 50 | MODEL_NUMBER / MODEL_FIRMWARE / MODEL_NAME | u32 / u32 / char[16] | |
| 51 / 52 | PIN / CHECK_PIN | u32 | |
| 53 | BUILD_NUMBER | u32 | |
| 59 | ACTIVITY_LOG | | |
| 64 | UI_CONTROL | char[8] | |
| 128 | FIR | i32 | |

Unlisted members default to i32.

## Parameters (struct / member / channels / num; ranges from the app skins)

| Parameter | Key | Range |
|---|---|---|
| gain | 0 / 0, all four channels | -48..12 dB |
| mute, invert | 7 / 14, 8 / 15 | 0/1 |
| delay | 2 / 8 | 0..2000 ms |
| mixer gain / on | 1 / 0, 1 / 24; input channels, num = source 0-3 | -48..0 dB |
| PEQ type / freq / gain / Q / on | 5 / 3, 1, 0, 2, 24; num = band | freq 20..20000 Hz, gain -12..12 dB, Q 0.2..25 |
| HPF freq / type / on | 4 / 1, 3, 24 | 19..20000 Hz (19 appears to mean off) |
| LPF freq / type / on | 3 / 1, 3, 24 | 20..20001 Hz (20001 appears to mean off) |
| limiter threshold / release | 6 / 4, 6 | release 10..1000 ms; outputs in dBu 16.78 up to ceiling 33.8 (1200), 36.78 (2400), 39.83 (4800); inputs from -24 dB |
| compressor threshold / ratio / attack / hold / release / gain | 12 / 4, 27, 5, 7, 6, 0 | ratio 1.2..25, times 1..10000 ms, gain -12..12 dB; threshold: the app allows up to 36.78 but an IPD2400 reports 37.82 (inputs) and 39.78 dBu (outputs) |
| link inputs / outputs | 10 / 25, channel 0 / 0x80, num 2 | 0/1 |
| channel name | 14 / 16 | 16 chars |
| unit name, connector names | 13 / 16, 13 / 46 (channel 0-3) | 16 chars |
| preset number / name | 9 / 18; 9 / 16 (channel 0 = current, 1..100) | 0..100 |
| standby delay | 13 / 34 | 0..30 min |

Enums:

| PEQ type | Value | Crossover shape (type byte: high nibble family, low nibble order) | Value |
|---|---|---|---|
| bell | 128 | Butterworth 6 / 12 / 18 / 24 | 0x00 / 0x01 / 0x02 / 0x03 |
| notch | 129 | Bessel 6 / 12 / 18 / 24 | 0x10 / 0x11 / 0x12 / 0x13 |
| allpass | 130 | Linkwitz-Riley 12 / 24 | 0x21 / 0x23 |
| low shelf | 131 | | |
| high shelf | 132 | | |
| band pass | 133 | | |
| high pass | 134 | | |
| low pass | 136 | | |

Users (CURRENT_USER): 0 AllDSP, 1 Factory, 2 Developer, 3 Admin, 4 User, 5 Locked.
AMP_READINGS: bits 31-24, 23-16, 15-8 = load impedance of outputs 1-3 in 0.1 ohm (255 = open/unknown), bits 7-0 = temperature in degrees C.
HARDWARE_STATUS bits: 0x1 init done, 0x2 memory test failed, 0x4 standby, 0x8 codec/bus failure, 0x10 DSP not synced, 0x20 protect, 0x40 dim display, 0x80 display off, 0x400 AES backup, 0x8000 telnet enabled, 0x80000 amplifier protection/standby.
ACCESS_RIGHTS bits: 0x1 input channel, 0x2 output channel, 0x4 load preset, 0x8 save preset, 0x10 front panel, 0x4000 display lock, 0x8000 language, 0x10000 user rights, 0x20000 locked rights, 0x40000 unit name, 0x80000 auto lock, 0x100000 unit configuration, 0x200000 load empty preset, 0x400000 admin rights, 0x800000 developer rights, 0x1000000 upgrade firmware, 0x2000000 full skin, 0x4000000 master volume, 0x8000000 tone control.

## Meters

Carried in every data reply as fragments 01, 02, 07, 08, 0B, 0C (see Opcodes); signed int8 dB per channel (inputs A/B, outputs 1/2). On MK1 hardware the data packets alternate between meter fragments and up to 4 RSP fragments.

## Presets and control commands

Slots 1..100, names via GET (9, 16, n). Load: SET (9, 18, 0, 0) = n, then command 1. Save: command 2.
Commands: SET Key(11, 26, 0, cmd) = 1:

| cmd | Action | cmd | Action |
|---|---|---|---|
| 1 | load preset | 14 | activate network settings |
| 2 | save preset | 20 | login (defined, not used by the client) |
| 3 | reset communication | 25 | load last preset |
| 4 / 5 | enter / exit standby | 26 | load empty preset |
| 6 | locate | 27 / 28 | clear user / factory presets |

A command is confirmed by any RSP echo (the firmware may reset the value to 0); resend at most 3 times.

## Session behaviour and quirks

- No handshake: the first command may follow the connect. The app polls every 0.05-0.1 s, with empty commands when idle.
- A SET is confirmed by an RSP echoing the same value; the app resends every 0.5 s up to 50 times.
- Resend after 0.5 s without reply; the app reconnects at 1 s (3 times) and gives up at 2 s.
- Some GETs are never sent as such; the firmware answers both keys: PIN / MODEL_NUMBER / MODEL_FIRMWARE via ACCESS_RIGHTS (13, 30), BUILD_NUMBER / MODEL_NAME via SW_VERSION (13, 33), AMP_READINGS(_2) via CURRENT_USER (13, 38).
- Login: SET CHECK_PASSWORD (13, 29) to 8 characters, then read CURRENT_USER. Not tried on hardware. A separate PIN exists (13, 51 / 52).
- The vendor app shows the output threshold as Vpk, W rms and dBu and asks for a load impedance (default 4 ohm). The amp stores only the dBu threshold; the impedance and the conversions are app-side.
- Before closing, send ASCII `DDRESETCONNECTIO`.
- Firmware update uses ASCII commands `DDUPLOADFIRMWARE`, `DCRESARTFIRMWARE`, `DDReset_Interface`; do not send.

## Verified on hardware (IPD2400 MK1, fw 1.2.6, 2026-11-02)

- Only TCP 50675 open; MK1 framing and CodeTable work; MK2 commands get `DDC` NACKs.
- About every other command gets a `DDN` NACK (busy). Resending immediately (10 ms) with the NACK's hw_id works; waiting 0.5 s drops throughput to about 2 packets/s. The amp barely checks hw_id/pc_id.
- Replies are pipelined: a GET is answered in the 3rd data packet (about 70 ms over Wi-Fi). After connecting, the amp streams its state on its own (about 370 keys in 35 s).
- Full snapshot of the 294 settings takes about 16 s. Writing all 294 back (queued, pipelined, each confirmed by its echo) takes about 20 s; a second snapshot was identical.
- Keys the firmware answers "together with" another key (MODEL_NAME with SW_VERSION, AMP_READINGS with CURRENT_USER) are sometimes skipped: ask again after about a second. AMP_READINGS then returned the temperature (48 C); the impedances read 0 with no load connected.
- Connections: opening several connections in quick succession occasionally times out on connect; keep retrying within the 3 s budget instead of giving up.
- Not answered directly: MODEL_NUMBER, BUILD_NUMBER, AMP_READINGS(_2), PIN (fw below 1.2.7 = 66055). MODEL_NAME (`IPD2400`) arrives with SW_VERSION; AMP_READINGS appears only occasionally in the stream.
- SET confirmed by echo (output gain write, read-back, restore). Preset name list read works.
- Link is master/slave inside the amp (2026-11-03): with inputs and outputs linked, writes to channel 1 (gain, EQ) are copied to channel 2 at once. A write to channel 2 is echoed, but channel 2 keeps channel 1's value, and twice such a write was followed by the amp not answering for 3 s. Write only channel 1 of a linked pair.
- Inferred only (vendor app): MK2 framing (`DDW`, 34-entry table), `DDB` announcements, login, preset load/save, standby, locate, FACTORY SET, FIR, activity log, sensor and RS-485 opcodes.

## Security notes

- Reads and a gain write worked without logging in on the test unit; whether user levels are enforced by the firmware for other writes was not tested.
- GET SET_PASSWORD (13, 28, level) returns every level's password in clear text, without logging in (confirmed for levels 1-4). Factory defaults: see device.
- Passwords are at most 8 characters; the PIN is a short number.
- The vendor app binary contains hard-coded credentials; they are deliberately not reproduced here.
- Keep the amp on an isolated management network.
