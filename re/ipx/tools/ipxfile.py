"""Decode IPX Controller container files (.ipxpreset, .factory, ...)."""

import hashlib
import struct
import sys
import zlib

KEY = 0x0102030405060708
KEY_PARTS = [(KEY >> (8 * i)) & 0xFF for i in range(8)]


def simplecrypt_decrypt(data: bytes) -> bytes:
    if len(data) < 3 or data[0] != 3:
        raise ValueError("not SimpleCrypt v3")
    flags = data[1]
    ba = bytearray(data[2:])
    last = 0
    for i, c in enumerate(ba):
        ba[i] = c ^ last ^ KEY_PARTS[i % 8]
        last = c
    ba = bytes(ba[1:])  # drop random prefix byte
    if flags & 0x02:  # checksum
        ba = ba[2:]
    elif flags & 0x04:  # sha1
        stored, ba = ba[:20], ba[20:]
        if hashlib.sha1(ba).digest() != stored:
            raise ValueError("SHA-1 mismatch")
    if flags & 0x01:  # qCompress: 4-byte BE length + zlib
        ba = zlib.decompress(ba[4:])
    return ba


def parse(raw: bytes):
    magic, version, ftype, extra = struct.unpack_from("<IIII", raw, 0)
    if magic != 0x585049:
        raise ValueError("bad magic")
    if sum(raw[:-4]) & 0xFF != struct.unpack_from("<I", raw, len(raw) - 4)[0]:
        raise ValueError("bad file checksum")
    (blen,) = struct.unpack_from("<I", raw, 0x10)
    body = raw[0x14 : 0x14 + blen]
    try:
        body = simplecrypt_decrypt(body)
        enc = True
    except ValueError:
        enc = False
    return dict(version=version, type=ftype, extra=extra, encrypted=enc), body


if __name__ == "__main__":
    hdr, body = parse(open(sys.argv[1], "rb").read())
    print(hdr, len(body), file=sys.stderr)
    if len(sys.argv) > 2:
        open(sys.argv[2], "wb").write(body)


# --- decoded preset body: sequence of (QString name, QString type, QVariant value), big-endian ---
QT_TYPES = {1: ">?", 2: ">i", 3: ">I", 4: ">q", 5: ">Q", 6: ">d", 38: ">d", 32: ">b", 33: ">h", 34: ">b", 36: ">H", 37: ">B"}


def read_qstring(b, o):
    (n,) = struct.unpack_from(">I", b, o)
    o += 4
    if n == 0xFFFFFFFF:
        return None, o
    raw = b[o : o + n]
    try:
        return raw.decode("utf-16-be"), o + n
    except UnicodeDecodeError:
        return raw, o + n  # binary blob stored in a QString variant (e.g. FIR data)


def read_qbytearray(b, o):
    (n,) = struct.unpack_from(">I", b, o)
    o += 4
    if n == 0xFFFFFFFF:
        return None, o
    return b[o : o + n], o + n


def read_qvariant(b, o):
    tid, null = struct.unpack_from(">IB", b, o)
    o += 5
    if tid == 10:
        return read_qstring(b, o)
    if tid == 12:
        return read_qbytearray(b, o)
    if tid in QT_TYPES:
        fmt = QT_TYPES[tid]
        return struct.unpack_from(fmt, b, o)[0], o + struct.calcsize(fmt)
    raise ValueError(f"unsupported QVariant type {tid} at {o - 5:#x}")


def parse_preset_body(b):
    o, out = 0, []
    while o < len(b):
        name, o = read_qstring(b, o)
        tname, o = read_qstring(b, o)
        val, o = read_qvariant(b, o)
        out.append((name, tname, val))
    return out


def parse_preset(raw: bytes):
    """Return (header, fir_blob_or_None, records)."""
    hdr, body = parse(raw)
    fir, o = None, 0
    if hdr["type"] in (8, 9):  # FiR-X presets: leading DSP coefficient blob
        fir, o = read_qbytearray(body, 0)
    return hdr, fir, parse_preset_body(body[o:])


def parse_factory(raw: bytes):
    """Factory file: QByteArray info block, u32 preset count, then per-preset QByteArray bodies."""
    hdr, body = parse(raw)
    info, o = read_qbytearray(body, 0)
    (count,) = struct.unpack_from(">I", body, o)
    o += 4
    presets = []
    for _ in range(count):
        blob, o = read_qbytearray(body, o)
        fir, p = (read_qbytearray(blob, 0) if hdr["type"] in (10, 11) else (None, 0))
        presets.append((fir, parse_preset_body(blob[p:])))
    model = info[:24].split(b"\0")[0].decode()
    return hdr, info, model, presets, len(body) - o
