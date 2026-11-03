# Python 2.7 harness: loads the ORIGINAL IntelliDrive Controller protocol code (decompiled
# d_protocol.py / protocol.py, see ../NOTES.md) and exposes build_command / parse_packet.
# Reads a JSON command on stdin: {"op":"build","mk2":bool,"items":[[[s,m,ch,num],value|null],...],
# "hw_id":n,"my_id":n} -> hex, or {"op":"parse","hex":"..."} -> JSON fragments.
# $IPD_SRC = directory with the decompiled sources.
import os
SRC_DIR = os.environ['IPD_SRC']
import re, struct, sys, types, collections, traceback, math

SRC = open(SRC_DIR + '/d_protocol.py').read()
PSRC = open(SRC_DIR + '/protocol.py').read()


def grab(src, start, end_marker):
    i = src.index(start)
    j = src.index(end_marker, i + len(start))
    return src[i:j]


# protocol constants (top of protocol.py, before the first import-dependent class)
consts = PSRC[PSRC.index('fBELL = 128'):PSRC.index('def get_user_text')]
fmt = grab(PSRC, 'def get_member_format', 'def get_member_scale')
protocol = types.ModuleType('protocol')
exec consts + fmt in protocol.__dict__

mytime = types.ModuleType('mytime')
mytime.displayTime = lambda: ''

g = {'protocol': protocol, 'mytime': mytime, 'struct': struct, 'sys': sys, 'traceback': traceback,
     'math': math, 'collections': collections}
exec SRC[SRC.index('CodeTable = ('):SRC.index('def readableHEX')] in g
code = ''.join([
    grab(SRC, 'class Data_packet():', 'class NACK_packet():'),
    grab(SRC, 'class NACK_packet():', 'class Checksum():'),
    grab(SRC, 'class Checksum():', 'def parse_announce(data):'),
    grab(SRC, 'def parse_NACK(data):', 'def parse_data_packet'),
    grab(SRC, 'def parse_data_packet', 'def setkeepalives'),
    SRC[SRC.index('def parse_packet(buf, length=32):'):],
])
# uncompyle6 mis-indents the checksum compare under the DDW branch; the bytecode (offset 741)
# runs it for DDA and DDW.
bad = """                for i in range(10, pos - 1):
                    checksum = checksum ^ ord(data[i])

                result = checksum - ord(data[pos])
                pos += 1
                if result != 0:
                    res.fragments = []
                    res.invalid_packet_counter = res.invalid_packet_counter + 1
                    print mytime.displayTime() + ' Checksum error, packet discarded'
                    print mytime.displayTime() + (' ').join([_[2] for c in data[:len(data)]])
                    return res
"""
good = """                for i in range(10, pos - 1):
                    checksum = checksum ^ ord(data[i])

            result = checksum - ord(data[pos])
            pos += 1
            if result != 0:
                res.fragments = []
                res.invalid_packet_counter = res.invalid_packet_counter + 1
                return res
"""
assert bad in code
code = code.replace(bad, good)
# Same for the tail of parse_packet: the bytecode jumps from the DDA branch to `return pkt`,
# so DDC/DDM/else continue the elif chain.
tail = "        elif buf[:3] == 'DDN':\n            return\n        if buf[:3] == 'DDC':"
assert tail in code
code = code.replace(tail, tail.replace("        if buf[:3] == 'DDC':", "        elif buf[:3] == 'DDC':"))
code = re.sub(r"\[_\[\d\] for c in ([^\]]+\])\]", r"[hex(ord(c)) for c in \1]", code)
exec code in g

bc = grab(SRC, '    def build_command(self, HW_id, my_id):', 'def print_hex')
bc = bc.replace('[_[1] for i in', '[chr(i) for i in').replace('[_[2] for i in', '[chr(i) for i in')
bc = '\n'.join(l[4:] for l in bc.splitlines())
exec bc in g
g['CHOOSE_VAR_TO_UPDATE_SKIP_CYCLES'] = 5000
g['Key'] = collections.namedtuple('Key', 'struct_id, member_id, channel, num')


class FakeConn(object):
    def __init__(self, mk2, items):
        self.MK2 = mk2
        self.factory = False
        self.check_vars_delay = 0
        self.items = list(items)

    def getVarsForCommand(self, packetLen):
        if not self.items:
            return (None, None)
        k, v = self.items.pop(0)
        return (g['Key'](*k), v)


def build(mk2, items, hw_id=7, my_id=1):
    return g['build_command'](FakeConn(mk2, items), hw_id, my_id)


def parse(buf):
    pkt = g['parse_packet'](buf, length=len(buf) - 10)
    if pkt is None:
        return None
    return [(op, frag.encode('hex')) for op, frag in pkt.fragments]


if __name__ == '__main__':
    import json
    cmd = json.loads(sys.stdin.read())
    if cmd['op'] == 'build':
        items = [(tuple(k), str(v) if isinstance(v, unicode) else v) for k, v in cmd['items']]
        print build(cmd['mk2'], items, cmd.get('hw_id', 7), cmd.get('my_id', 1)).encode('hex')
    else:
        print json.dumps(parse(cmd['hex'].decode('hex')))
