#!/usr/bin/env python3
"""Install a PNG as the badge splash image.

Converts any PNG into the two checked-in splash artifacts for the
gameoflife app:

  apps-baosec/gameoflife/splash.png      128x128 1-bit grayscale (review
                                          artifact: exactly what the badge
                                          shows)
  apps-baosec/gameoflife/src/splash_img.rs  [u32; 512] bitboard embedded in
                                          the app (LSB of each word =
                                          leftmost pixel of that 32-cell
                                          row span, matching the app's
                                          field layout)

The image is scaled to 128x128 with nearest-neighbor and thresholded to
1-bit (luminance > 50% = ink). Transparent pixels are treated as off.
White pixels are ink; the badge renders live cells as white-on-black, so
a white-on-black PNG is what you want.

Usage:
  python3 png2splash.py /path/to/image.png

Stdlib only (zlib). Supports grayscale (1/2/4/8/16-bit), RGB, RGBA, and
palette PNGs, all filter types, non-interlaced.
"""
import struct
import sys
import zlib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
PNG_OUT = REPO / "apps-baosec/gameoflife/splash.png"
RS_OUT = REPO / "apps-baosec/gameoflife/src/splash_img.rs"

W = H = 128
COLOR_CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}


def decode_png(path):
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos = 8
    idat = b""
    w = h = bitd = ct = interlace = None
    palette = None
    while pos < len(data):
        ln = struct.unpack(">I", data[pos:pos + 4])[0]
        typ = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + ln]
        if typ == b"IHDR":
            w, h, bitd, ct, _c, _f, interlace = struct.unpack(">IIBBBBB", body)
        elif typ == b"IDAT":
            idat += body
        elif typ == b"PLTE":
            palette = body
        pos += 12 + ln
    assert interlace == 0, "interlaced PNGs not supported; re-export non-interlaced"
    ch = COLOR_CHANNELS[ct]
    raw = zlib.decompress(idat)
    stride = (w * bitd * ch + 7) // 8
    bpp = max(1, (bitd * ch) // 8)
    out = bytearray()
    prev = bytearray(stride)
    for y in range(h):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        if f == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 0xFF
        elif f == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif f == 3:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((a + prev[i]) // 2)) & 0xFF
        elif f == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        out += line
        prev = line
    return w, h, bitd, ct, out, stride, palette


def sample_pixel(px, stride, x, y, w, h, bitd, ct, palette=None):
    ch = COLOR_CHANNELS[ct]
    bits_per_px = bitd * ch
    maxv = (1 << bitd) - 1
    if bits_per_px == 8:
        i = y * stride + x * ch
        vals = px[i:i + ch]
    elif bits_per_px == 16:
        i = y * stride + x * ch * 2
        vals = [px[i + 2 * k] for k in range(ch)]
        maxv = 65535
    else:  # packed sub-byte gray
        i = y * stride + x // (8 // bitd)
        shift = 8 - bitd - (x % (8 // bitd)) * bitd
        vals = [(px[i] >> shift) & maxv]
    if ct == 3:  # palette
        r, g, b = palette[vals[0] * 3:vals[0] * 3 + 3]
        a = 255
    elif ct == 0:
        r = g = b = int(vals[0] * 255 / maxv)
        a = 255
    elif ct == 2:
        r, g, b = [int(v * 255 / maxv) for v in vals]
        a = 255
    elif ct == 4:
        g0 = int(vals[0] * 255 / maxv)
        r = g = b = g0
        a = int(vals[1] * 255 / maxv)
    else:  # ct == 6
        r, g, b = [int(v * 255 / maxv) for v in vals[:3]]
        a = int(vals[3] * 255 / maxv)
    return r, g, b, a


def ink(r, g, b, a):
    if a < 128:
        return 0
    return 1 if (0.299 * r + 0.587 * g + 0.114 * b) > 127.5 else 0


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    src = Path(sys.argv[1]).expanduser()
    assert src.is_file(), f"no such file: {src}"
    w, h, bitd, ct, px, stride, palette = decode_png(src)
    print(f"decoded {src.name}: {w}x{h} bitdepth={bitd} colortype={ct}")

    # sample + scale to 128x128 (nearest neighbor) + threshold to 1 bit
    bits = bytearray(128 * 16)
    on = 0
    for y in range(128):
        sy = y * h // 128
        for x in range(128):
            sx = x * w // 128
            r, g, b, a = sample_pixel(px, stride, sx, sy, w, h, bitd, ct, palette)
            if ink(r, g, b, a):
                on += 1
                bits[y * 16 + x // 8] |= 1 << (7 - x % 8)
    print(f"scaled to 128x128, 1-bit: {on} on-pixels")

    # write the 1-bit PNG (same format the app's artifact uses)
    raw = bytearray()
    for y in range(128):
        raw.append(0)
        raw += bits[y * 16:(y + 1) * 16]
    def chunk(typ, payload):
        c = struct.pack(">I", len(payload)) + typ + payload
        return c + struct.pack(">I", zlib.crc32(typ + payload) & 0xFFFFFFFF)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", 128, 128, 1, 0, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    PNG_OUT.write_bytes(png)

    # write the Rust bitboard: LSB of each word = leftmost pixel of the
    # 32-pixel row span (same layout the app's bitboard uses)
    words = []
    for y in range(128):
        for bx in range(4):
            word = 0
            for bit in range(32):
                x = bx * 32 + bit
                if bits[y * 16 + x // 8] & (1 << (7 - x % 8)):
                    word |= 1 << bit
            words.append(word)
    out = []
    out.append("//! Pre-rendered splash card for the gameoflife badge app:")
    out.append("//! 128x128, 1 bit per pixel packed 32 pixels per u32 in row-major")
    out.append("//! order, bit 0 (LSB) of each word = leftmost pixel of that 32-cell")
    out.append("//! span -- the same layout the app's bitboard uses. Every set bit is")
    out.append("//! a live cell, so this array IS the Game of Life initial population")
    out.append("//! shown at boot.")
    out.append("//!")
    out.append("//! GENERATED FILE - do not edit by hand. Regenerate from an image:")
    out.append("//!   python3 tools/splash-gen/png2splash.py <image.png>")
    out.append("//! or from the procedural renderer:")
    out.append("//!   cd tools/splash-gen && cargo run --release")
    out.append("//!")
    out.append(f"//! Generated from: {src.name}")
    out.append("")
    out.append("pub const SPLASH_IMG: [u32; 512] = [")
    for i in range(0, 512, 4):
        out.append("    " + "".join(f"0x{w:08x}, " for w in words[i:i + 4]))
    out.append("];")
    RS_OUT.write_text("\n".join(out) + "\n")

    print(f"wrote {PNG_OUT}")
    print(f"wrote {RS_OUT}")


if __name__ == "__main__":
    main()
