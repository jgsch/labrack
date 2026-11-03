# IPX Controller 2.3.4b — reverse-engineering notes

Source: `ipx-controller-setup-2.3.4b.exe` (sha256 3c2dfc12…fa2f), Inno Setup 6.4.0 installer,
Authenticode-signed by "Music Tribe Brands SE1 AB". Main program `app/IPXController.exe`
(Qt 6 / MinGW x86-64, stripped; RTTI class names remain). Analysed statically with REA + Ghidra 12.1.4.

## Network

| Purpose | Transport | Port | Evidence |
|---|---|---|---|
| Device discovery (amp -> PC) | UDP, receive only, bound with ShareAddress/ReuseAddressHint | **52350** (0xCC7E) | `FUN_14015d2f0`: `QAbstractSocket::bind(sock, 0xcc7e, 5)` |
| Control | TCP client | from discovery packet, default **50168** (0xC3F8) | `FUN_14015cde0` default, `FUN_140152f50` `connectToHost(ip, port)` |

The app never calls `writeDatagram`, so discovery is passive: the amps broadcast and the app listens.

### Frame format (both TCP and UDP), little-endian

```
off  size  field
0    4     sync   = 0xAA01AA02   (bytes 02 AA 01 AA)
4    4     len    = bytes after this 16-byte header = 4 + payload_len + 8   (max 0x2800)
8    2     main   (MG_NETMAINCMD_*)
10   2     src?   always 2 when sent by the PC
12   4     reserved (0)
16   4     sub-header (meaning depends on main, see below)
20   n     payload
20+n 4     checksum = (byte sum of sub-header + payload) & 0xFF, as uint32
24+n 4     trailer = 0x11110123
```
Parser: `FUN_140175400`, `FUN_14015e750`; builders: `FUN_1401768d0` (SET/GETPRM), `FUN_140176c60` (SYSCTRL).
Checksum: `FUN_140160080` (8-bit additive). A zero checksum field skips the check on receive.

### Enums (order of the name tables in .rdata, which agrees with the code)

MG_NETMAINCMD: 0 NONE, 1 SETPRM, 2 GETPRM, 3 SYSCTRL, 4 DEVBROADCAST, 5 DEVPRMCHAGE

MG_ENVCMD (sub-header byte 0 for SETPRM/GETPRM):
0 NONE, 1 SYSATTR, 2 USER, 3 NETWORK, 4 AUDCFG, 5 MODEAES, 6 INPUT_MIXER, 7 INPUT_LEVEL,
8 INPUT_EQ, 9 INPUT_DELAY, 10 INPUT_RMSLIMITER, 11 INPUT_INLIMITER, 12 OUTPUT_XOVER,
13 OUTPUT_LEVEL, 14 OUTPUT_EQ, 15 OUTPUT_DELAY, 16 OUTPUT_RMSLIMITER, 17 OUTPUT_LIMITER,
18 ACK, 19 DEVCFG, 20 GAINSLEVEL, 21 PRESET_PRMCFG, 22 TEMPERATURE, 23 SWITCH_USER,
24 USERPASSWORD, 25 DEV_PRM_SYNC, 26 PRESET_NOTIFY, 27 DEVICE_NAME, 28 ALL_TEMPERATURES, 29 FIR_MODE

MG_CTRLCMD (sub-header u16 for SYSCTRL):
0 NONE, 1 HEARTBEAT, 2 SETPRESET, 3 GETPRESET, 4 RECALLPRESET, 5 DELETEPRESET, 6 QUERYSTATUS,
7 DEVICE_REBOOT, 8 SETDEV_FACTORYINFO, 9 UPGRADE_REQUEST, 10 UPGRADE_READY, 11 UPGRADE_PACKAGE,
12 UPGRADE_SUCCESS, 13 USER_LOGIN, 14 DEL_ALLUSERPRESET, 15 DEL_ALLFACTPRESET, 16 ACK,
17 FPLEDFLASH, 18 GETALLPRESETNAME, 19 GETVALIDPRESETS, 20 STOREONEPRESET_PKG, 21 PC_STORE_PRESET,
22 PC_RECALL_PRESET, 23 SET_HARDWARE_CFG, 24 GET_HARDWARE_CFG, 25 FORCE_FAN_ON, 26 INSTALL_KEY,
27 GET_KEY, 28 DEL_KEY, 29 PC_RECALL_PRESET_VLOCK, 30 FIR_SET_DATA, 31 FIR_GET_DATA,
32 STORE_FIR_DATA, 33 LOAD_FIR_DATA, 34 GET_LOG

### Sub-headers
* SETPRM / GETPRM / DEVPRMCHAGE: `u8 envcmd, u8 channel, u8 index, u8 field`.
  The app maps parameter names to these 4 values with a table (`FUN_140155660` and similar, lookup in
  `FUN_140154290` / `FUN_140153fb0`); see `paramtable.txt`. Examples:

  | name | envcmd | ch | idx | field |
  |---|---|---|---|---|
  | Input1LevelControl / Input2LevelControl | 7 INPUT_LEVEL | 0 / 1 | 0 | 0 |
  | Input1Mute / Input2Mute | 7 | 0 / 1 | 0 | 1 |
  | InputLink | 7 | 0 | 0 | 2 |
  | Output1LevelControl / Output2LevelControl | 13 OUTPUT_LEVEL | 0 / 1 | 0 | 0 |
  | Output1Mute / Output2Mute | 13 | 0 / 1 | 0 | 1 |
  | Output1PhaseToggle / Output2PhaseToggle | 13 | 0 / 1 | 0 | 2 |
  | OutputLink | 13 | 0 | 0 | 3 |

  **Verified on an IPX 1200:** every 4-byte field of every block is addressable as
  `[env, channel, item, field]`, where `field` is the dword index inside the block's struct
  (see `docs/ipx-protocol.md` for the table). GETPRM with an empty payload returns the 4-byte value
  under the same sub-header; SETPRM with the 4-byte value writes it and sends no reply.
  Floats are float32 LE. The amp pushes GAINSLEVEL (12 floats, about 10 Hz) and TEMPERATURE.
  GETPRM DEV_PRM_SYNC returns the whole state (0x6B0 bytes). It allows one TCP client at a time
  (a second connection is accepted then closed) and requires no login.
* SYSCTRL: `u16 ctrlcmd, u16 0`.
* Field order in the bulk AUDCFG structs (`FUN_140171ee0` etc.): output = {level, mute, phase, link, name[]},
  input = {level, mute, link, name[]}.

### DEVBROADCAST payload (UDP, main = 4), little-endian, `FUN_14015d1a0` / `FUN_14015cde0`
```
0x00 char[16]  id string        -> device-info +0x18
0x10 char[32]  name             -> device-info +0x00
0x30 char[16]  IPv4 address (text) -> device-info +0x30 (used by connectToHost)
0x40 u16       TCP control port -> device-info +0x48
0x42 u16       model/type (3 is mapped to 4); packets with a value >= 4 are ignored
0x44 i32       flag (!= 0)
```

### Password obfuscation
Payloads for ENVCMD 24 (USERPASSWORD) and 21 (PRESET_PRMCFG with byte 8 == 0) are XORed in place
(`FUN_140153c80`) with the fixed 16-byte key
`3a 7f 92 4b 1d e8 6c 55 a3 0f 78 9b 2e c1 4d 8a`. It can be reversed trivially.

## Files (.ipxpreset / .factory / ...) — see tools/ipxfile.py

Container (little-endian), `FUN_140160810`:
```
0x00 u32 magic   0x00585049 ("IPX\0")
0x04 u32 version (bytes 00 00 00 01 / 00 00 00 02; must be <= 0x02000000)
0x08 u32 type    1 preset, 5 locked preset, 8 FiR-X preset, 9 locked FiR-X preset,
                 2/7/10/11 factory (10/11 carry FIR data; 7/11 = .factorylocked)
0x0C u32 models  bitmask 1 = IPX1200, 2 = IPX2400, 4 = IPX4800
0x10 QByteArray  body (u32 LE length + bytes)
end-4 u32        byte-sum of everything before it
```
The body is optionally encrypted with **SimpleCrypt** (Andre Somers' Qt class),
key `0x0102030405060708`, version byte 3, SHA-1 integrity flag. If decryption fails, the app uses the raw body.
"Locked" types (5/9/7/11) are additionally decrypted with the device vendor key (`IPXVendorLockKey`); not covered here.

Preset body (big-endian Qt `QDataStream`): repeated `QString name, QString typeName, QVariant value`
(306 records; 309 for FiR-X, which are preceded by a `QByteArray` DSP coefficient blob starting `03 'RIF'`).
Floats are serialized as doubles. Presets contain `PresetAdminPassword` / `PresetDeveloperPassword` in plaintext.

Factory body: `QByteArray info` (0x88 bytes: char[24] model, then small table with "Password", "admin", "ipxopen"),
`u32 count`, then `count` × `QByteArray` preset bodies.
