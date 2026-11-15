"""Writes the registry icons: the Java mod's 16x16 "WD" advancement icon scaled up, and a
16x16 browser-window drawing for HCEF, both as 256x256 PNG (nearest neighbour).

usage: python -I mkicons.py <advicon_wd.png> <webdisplays out.png> <hcef out.png>
"""
import struct
import sys
import zlib


def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    at, chunks, idat = 8, {}, b""
    while at < len(data):
        size, kind = struct.unpack(">I4s", data[at:at + 8])
        body = data[at + 8:at + 8 + size]
        if kind == b"IDAT":
            idat += body
        else:
            chunks[kind] = body
        at += 12 + size
    width, height, depth, colour = struct.unpack(">IIBB", chunks[b"IHDR"][:10])
    assert depth == 8 and colour == 6, "8-bit RGBA only"
    raw, stride, rows, prev = zlib.decompress(idat), width * 4, [], bytearray(width * 4)
    for y in range(height):
        line = raw[y * (stride + 1):(y + 1) * (stride + 1)]
        kind, cur = line[0], bytearray(line[1:])
        for i in range(stride):
            a = cur[i - 4] if i >= 4 else 0
            b = prev[i]
            c = prev[i - 4] if i >= 4 else 0
            if kind == 1:
                cur[i] = (cur[i] + a) & 255
            elif kind == 2:
                cur[i] = (cur[i] + b) & 255
            elif kind == 3:
                cur[i] = (cur[i] + (a + b) // 2) & 255
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                cur[i] = (cur[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append([tuple(cur[x * 4:x * 4 + 4]) for x in range(width)])
        prev = cur
    return rows


def write_png(path, rows, scale):
    height, width = len(rows) * scale, len(rows[0]) * scale
    raw = bytearray()
    for row in rows:
        line = b"\x00" + b"".join(bytes(pixel) * scale for pixel in row)
        raw += line * scale

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b""))


# HCEF: a browser window. One character per pixel.
HCEF = [
    "................",
    ".FFFFFFFFFFFFFF.",
    ".FTTTTTTTTTTTTF.",
    ".FTrTyTgTTTTTTF.",
    ".FTTTTTTTTTTTTF.",
    ".FBBBBBBBBBBBBF.",
    ".FBBBBccccBBBBF.",
    ".FBBBcCwCCcBBBF.",
    ".FBBcCwCCwCcBBF.",
    ".FBBcwwwwwwcBBF.",
    ".FBBcCwCCwCcBBF.",
    ".FBBBcCwCCcBBBF.",
    ".FBBBBccccBBBBF.",
    ".FBBBBBBBBBBBBF.",
    ".FFFFFFFFFFFFFF.",
    "................",
]
COLOURS = {
    ".": (0, 0, 0, 0),
    "F": (38, 42, 54, 255),      # frame
    "T": (70, 78, 98, 255),      # title bar
    "r": (236, 96, 86, 255),
    "y": (245, 190, 79, 255),
    "g": (98, 197, 84, 255),
    "B": (18, 22, 32, 255),      # page
    "c": (40, 110, 190, 255),    # globe's edge
    "C": (64, 156, 235, 255),    # globe
    "w": (214, 236, 255, 255),   # its lines
}

wd, out_wd, out_hcef = sys.argv[1:4]
write_png(out_wd, read_png(wd), 16)
write_png(out_hcef, [[COLOURS[c] for c in row] for row in HCEF], 16)
print("written")
