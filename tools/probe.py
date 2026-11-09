#!/usr/bin/env python3
"""Read-only probe of the Razer 90-byte feature-report protocol.

Sends only GET commands (command id with bit 0x80 set) and prints which
class/id pairs the device answers. Usage: probe.py /dev/hidrawN [class ...] [--args HEX]
"""
import fcntl
import os
import sys
import time

REPORT_LEN = 90
STATUS = {0x00: "new", 0x01: "busy", 0x02: "ok", 0x03: "fail", 0x04: "timeout", 0x05: "unsupported"}


def ioc(nr, size):
    # _IOC(_IOC_READ|_IOC_WRITE, 'H', nr, size)
    return (3 << 30) | (size << 16) | (ord("H") << 8) | nr


HIDIOCSFEATURE = ioc(0x06, REPORT_LEN + 1)
HIDIOCGFEATURE = ioc(0x07, REPORT_LEN + 1)


def build(tid, cls, cid, args=b""):
    r = bytearray(REPORT_LEN)
    r[1] = tid
    r[5] = 0x50
    r[6] = cls
    r[7] = cid
    r[8:8 + len(args)] = args
    crc = 0
    for b in r[2:88]:
        crc ^= b
    r[88] = crc
    return r


def send(fd, report):
    fcntl.ioctl(fd, HIDIOCSFEATURE, bytes([0]) + report)
    for _ in range(20):
        time.sleep(0.005)
        buf = bytearray(REPORT_LEN + 1)
        fcntl.ioctl(fd, HIDIOCGFEATURE, buf)
        resp = buf[1:]
        if resp[0] != 0x01:
            return resp
    return resp


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    argv = sys.argv[2:]
    args = b""
    if "--args" in argv:
        i = argv.index("--args")
        args = bytes.fromhex(argv[i + 1])
        argv = argv[:i]
    classes = [int(c, 0) for c in argv] or range(0x00, 0x10)
    fd = os.open(sys.argv[1], os.O_RDWR)
    tid = 0x1F
    # sanity: get firmware version (class 0x00, id 0x81)
    resp = send(fd, build(tid, 0x00, 0x81))
    print(f"firmware: status={STATUS.get(resp[0], hex(resp[0]))} v{resp[8]}.{resp[9]}")
    for cls in classes:
        for cid in range(0x80, 0x100):
            resp = send(fd, build(tid, cls, cid, args=args))
            if resp[0] != 0x02:
                continue
            payload = bytes(resp[8:8 + resp[5]]).rstrip(b"\0")
            if (cls, cid) == (0x00, 0x82):
                payload = b""  # factory serial, keep it out of protocol files posted publicly
            print(f"class 0x{cls:02x} id 0x{cid:02x} size={resp[5]:3d} data={payload.hex()}")
    os.close(fd)


if __name__ == "__main__":
    main()
