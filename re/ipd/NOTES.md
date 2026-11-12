# IntelliDrive Controller 1.2.11 (IPD 1200 / 2400 / 4800): reverse-engineering notes

Source: `LAB.GRUPPEN_IntelliDrive_Controller_64_bit_v_1_2_11_125735.msi`
(sha256 f854b6f9…aaac2), WiX MSI, built 2020-01-09. The program is a **Python 2.6 / wxPython app
frozen with py2exe** (made by AllDSP for Lab.gruppen). Its own 39 modules were extracted from the exe's
zip archive and decompiled with uncompyle6 3.9.2 / xdis 6.1.1. All of them decompiled without errors.

uncompyle6 mis-indents two blocks in `d_protocol.py`; the bytecode was checked (`pydisasm`):
* `parse_data_packet`: the checksum compare + `pos += 1` apply to **both** `DDA` and `DDW`
  (bytecode offset 741), not only to `DDW`.
* `parse_packet` tail: after the `DDA` branch the code returns `pkt`; `DDC`/`DDM` are `elif`s.

Reproduce: `msiextract` the MSI, unzip `IntelliDrive Controller.exe`, decompile the app `.pyc`s.
`tools/orig_harness_py27.py` runs the original encoder/decoder (with the two fixes) under Python 2.7 and
`tools/gen_vectors.py` regenerates `crates/amps/tests/fixtures/ipd_vectors.json` from it.

## Models
Unit type (announcement bytes 33-35) `27_0_1`; Global MODEL_NUMBER: 0 = IPD2400, 1 = IPD1200,
2 = IPD4800. Each has skins `27_0_1_N` (user `_u`, developer `_d`); `default.cfg` in each skin lists
every control with `struct_id / member_id / channel(s) / num / min / max`. The three maps are identical
except output limiter/compressor threshold ceilings (dBu: 1200 = 33.8, 2400 = 36.78, 4800 = 39.83)
and meter ranges.

## Network
| Port | Use |
|---|---|
| UDP 50674 | amps broadcast 42-byte `DDB…Q` announcements (app listens) |
| TCP 50675 | control; also answers the `4DPP` probe the app sends to every IP of the subnet |
| UDP 57383 | app broadcasts TFTP-like `\0\2hgzd.dam\0netascii\0` (bootloader detection) |

Commands sent as plain ASCII to 50675: `DDRESETCONNECTIO` (before closing), and during firmware
updates `DDUPLOADFIRMWARE`, `DCRESARTFIRMWARE`, `DDReset_Interface`.

### Announcement (42 bytes)
`DDB` · [4] type2 · [5] type1 · [5:22] MAC text · [22:25] HW version · [25:28] PIC version ·
[28:31] serial (BE 24-bit) · [31] year-2000 · [32] month · [33:36] unit type · [36] day ·
[37] DSP state · [39] PC packet id · [40] HW packet id · [41] `Q`. MK2 units put `67890Q` in [36:42].

### Session (`d_protocol.Remote_Link.ethernet_process`)
Strict lockstep: one command, one reply, every 0.05–0.1 s (empty commands when idle; replies always
carry meters). A command is resent after 0.5 s; after 1 s the app reconnects (3 times), after 2 s it
gives up. No handshake: the first command can be sent right after connecting.

Command (`build_command`):
```
MK1: 05 'D' 'D' 'J'   | one fragment max (packet limit 8)
MK2: lenH lenL 'D' 'J'| fragments up to ~500 bytes; len = 4 + len(fragments)
fragments XOR CodeTable[i % 32]
FD hw_id FF my_id     | hw_id = id from the last reply, my_id = 1..254
checksum              | 0xAA ^ all previous bytes
GET: 03 07 struct member channel num 'j'
SET: 04 (6+n) struct member channel num value      (FACTORY = 0A instead of 04)
```
Reply:
```
MK1 'DDA': 'DDA' + 36 bytes XOR CodeTable[i % 32] + pc_id + hw_id + 'Q'
MK2 'DDW': 'DDW' ... [7] pc_id, [8:10] body length (BE) + body XOR CodeTable[i % 34]
body: fragments (opcode, size incl. 2-byte head, data...), then 09 checksum (0xD5 ^ body bytes)
CodeTable = 233 45 98 215 123 42 99 23 178 132 200 150 100 50 77 43 54 90 31 43 123 234 211 177
            65 133 3 64 23 83 251 1 54 34
```
Opcodes: 01 VU in, 02 VU out (signed dB per channel), 03 GET, 04 SET, 05 RSP (struct member channel
num value), 06 ERR, 07/08 gain reduction in/out, 09 checksum, 0A factory set, 0B raw input VU,
0C output voltage/current pairs (V dB = b/4 - 3.75, A dB = b/4 - 13.75), 0D/0E inclination,
0F activity log (513 bytes), 62/63 RS-485 slave list / id.

A SET is confirmed by the RSP echo of the same value; the app retries every 0.5 s up to 50 times.
Some GETs are never sent as such; the app asks for another key and the firmware answers both:
PIN/MODEL_NUMBER/MODEL_FIRMWARE → ACCESS_RIGHTS, BUILD_NUMBER/MODEL_NAME → SW_VERSION,
AMP_READINGS → CURRENT_USER.

### Addressing
`Key(struct_id, member_id, channel, num)`: inputs A/B = channels 0/1, outputs = 0x80/0x81.
Structs: 0 gain, 1 mixer, 2 delay, 3 LPF, 4 HPF, 5 PEQ, 6 limiter, 7 mute, 8 invert,
9 preset global, 10 link, 11 command, 12 compressor, 13 global, 14 channel, 15 FIR, 32 UI control.
Members, wire formats (big-endian) and scales (raw = value × scale) are in `crates/amps/src/amps/ipd/protocol.rs`;
e.g. gain `>h` ×100 (dB), Q `>H` ×10, delay `>L` ×1000 (ms → µs), threshold ×100, ratio ×10.
The parameter table is `crates/amps/src/amps/ipd/params.rs`.

Enums: PEQ type 128 bell, 129 notch, 130 allpass, 131 low shelf, 132 high shelf, 133 band pass,
134 high pass, 136 low pass. Crossover shape: low nibble order (0 = 6 dB … 3 = 24 dB), high nibble
family (0 Butterworth, 1 Bessel, 2 Linkwitz-Riley).

Commands: SET `(11, 26, 0, cmd) = 1`: 1 load preset (number in `(9, 18, 0, 0)`), 2 save preset,
4/5 enter/exit standby, 6 locate, 14 activate network settings, 25 load last preset, 26 load empty
preset, 27/28 clear user/factory presets.

`AMP_READINGS (13, 37)`: bytes 3/2/1 = load impedance ch1/ch2/ch3 ×0.1 Ω, byte 0 = temperature °C.
`HARDWARE_STATUS (13, 35)` bits: 1 init done, 2 memory test failed, 4 standby, 8 internal bus,
16 DSP not synced, 32 protect, 64 dim display, 128 display off, 0x8000 **telnet enabled**,
0x80000 amp protection/standby. (Like the IPX, the IPD firmware has a telnet option.)

### Users and passwords
Levels (`CURRENT_USER`): 0 AllDSP, 1 Factory, 2 Developer, 3 Admin, 4 User, 5 Locked.
Login: SET `CHECK_PASSWORD (13, 29)` to the 8-character password. The app reads every level's password
from the amp with GET `SET_PASSWORD (13, 28, level)`, so **passwords are readable over the network**.
App defaults: initial password `Password`; a hard-coded `dq8r0mzu` is the lock password: Lock Unit
(`child_unit.LockUnit`) sends it as a CHECK_PASSWORD and polls CURRENT_USER until 5, and preset loads send it
when the user is not at User level; factory presets carry the tag `of9juc8hklcndrsnoc48uxow5uhrjdajxos84mpxayie000<type>`.
A separate numeric PIN exists (`PIN (13, 51)` / `CHECK_PIN (13, 52)`).
Note: `network_customized.py` also contains the vendor's SMTP account credentials for support e-mail.

## Preset files (.icpreset)
zip → one member = urlsafe_b64(bytewise (b64(cPickle(dict)) + key) mod 256), key from `keyfile.py`
(fallback: `child_unit.default_encryption_key`), or plain b64(cPickle), or (old) a bare pickle.
The dict maps `data_model.Key` → raw wire values; `(255, 29, 0, 1)` holds the save restriction,
`(255, 29, 0, 0)` the factory tag. The Rust library does not read them (it saves JSON snapshots); a sample is in `samples/original.icpreset`. The
earlier Python library read them with a restricted unpickler and wrote files the original app could open.

## Observed on hardware (IPD2400 MK1, firmware 1.2.6, 2026-11-02, over Wi-Fi)
* Only TCP 50675 is open. No UDP announcements were received on 50674 (maybe Wi-Fi).
* The `4DPP` probe is answered with 42 bytes `DDP`/`DDC` + 36 constant non-decodable bytes +
  pc_id, hw_id, `Q`, **not** a `DDB` announcement. The original app would not find this unit by
  polling. The library treats any such reply as "unit present, MK1 unless the MK2 marker is there".
* MK1 framing works; MK2 commands are answered with `DDC` NACKs.
* About every other command gets a `DDN` NACK (busy). Resending the same command right away (10 ms,
  with the NACK's hw_id) works; waiting 0.5 s like the app cuts throughput to about 2 packets/s.
  The amp barely checks hw_id/pc_id.
* Replies are pipelined: a GET is answered in the 3rd data packet (about 70 ms over Wi-Fi). Data packets
  alternate between meters (VU in/out, GR in/out, raw VU) and up to 4 RSP fragments.
* After connecting, the amp also streams its whole state on its own (about 370 keys in 35 s when polled
  fast). A full snapshot of the 312 known keys takes about 16 s.
* Not answered directly: MODEL_NUMBER, BUILD_NUMBER, AMP_READINGS(_2), PIN (fw < 1.2.7 = 66055,
  `minModelFirmwareVersion`). MODEL_NAME `IPD2400` arrives with SW_VERSION, AMP_READINGS only now and
  then in the stream (seen once: 31 = 31 °C, impedance 0).
* SET confirmed by echo (output gain write/read-back/restore tested, about 1.7 s).
* Confirmed: GET `SET_PASSWORD(13, 28, level)` returns every level's password in clear text, without
  logging in (levels 1-4 on this unit were the factory defaults `ipd2fact`, `ipdopen`, `Ad_min`, `Password`).

## Firmware
`dfi/27_0_1_{0,1,2}.dfi` (98 KB each) and `dfi/0_0_0.dfe` are encrypted firmware images; not analysed.
