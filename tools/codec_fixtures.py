#!/usr/bin/env python3
"""Writes tools/fixtures/: `sample(n)` (as codec.rs's tests make it) compressed by the reference libraries,
in the formats Distant Horizons' data compression modes write. codec.rs's tests decode them.

    python3 -m venv /tmp/venv && /tmp/venv/bin/pip install zstandard lz4
    /tmp/venv/bin/python tools/codec_fixtures.py

With Java 11+ on PATH (or JAVA_HOME), also the dhs-unfinished-*.xz fixtures: tools/DhsFixtures.java with xz-java
1.12 (downloaded to target/), as Distant Horizons Support's Lod.compress writes them: flushed, never finished.

zstd at level 3 (zstd-jni's `Zstd.compress(data, 3)`), lz4 frames of 64 KiB independent blocks without
checksums (lz4-java's `LZ4FrameOutputStream` as DH opens it), xz at preset 3 with CRC-64 (xz-java's
`XZOutputStream(LZMA2Options(3), CHECK_CRC64)`).
"""
import lzma
import os
import shutil
import subprocess
import tempfile
import urllib.request
from pathlib import Path

import lz4.frame
import zstandard

TOOLS = Path(__file__).resolve().parent
OUT = TOOLS / "fixtures"
SIZES = [0, 1, 1000, 70000]
XZ_JAR = "https://repo1.maven.org/maven2/org/tukaani/xz/1.12/xz-1.12.jar"
M32 = 0xFFFFFFFF


def sample(length):
    out = bytearray()
    x = 12345
    while len(out) < length:
        x = (x * 1103515245 + 12345) & M32
        kind = (x >> 16) % 5
        if kind == 0:
            out += bytes([(x >> 8) & 255]) * (x % 40)
        elif kind == 1:
            out += b"minecraft:grass_block_STATE_{snowy:false}"
        elif kind == 2 and len(out) > 300:
            start = len(out) - 1 - (x % 300)
            n = (x >> 3) % 64
            for k in range(n):
                out.append(out[start + k % (len(out) - start)])
        elif kind == 3:
            out += bytes(((x >> (k % 24)) & 255) for k in range(x % 20))
        else:
            out.append((x >> 24) & 255)
    return bytes(out[:length])


def main():
    OUT.mkdir(exist_ok=True)
    for n in SIZES:
        data = sample(n)
        (OUT / f"sample-{n}.zst").write_bytes(zstandard.ZstdCompressor(level=3).compress(data))
        (OUT / f"sample-{n}.lz4").write_bytes(lz4.frame.compress(
            data, block_size=lz4.frame.BLOCKSIZE_MAX64KB, block_linked=False, content_checksum=False,
            store_size=False))
        (OUT / f"sample-{n}.xz").write_bytes(lzma.compress(
            data, format=lzma.FORMAT_XZ, check=lzma.CHECK_CRC64, preset=3))
    data = sample(70000)
    (OUT / "linked-70000.lz4").write_bytes(lz4.frame.compress(
        data, block_size=lz4.frame.BLOCKSIZE_MAX64KB, block_linked=True, content_checksum=True))
    (OUT / "ultra-70000.zst").write_bytes(zstandard.ZstdCompressor(level=19, write_checksum=True).compress(data))
    (OUT / "fast-70000.zst").write_bytes(zstandard.ZstdCompressor(level=-5).compress(data))
    dhs()
    print("wrote", len(list(OUT.iterdir())), "fixtures to", OUT)


def dhs():
    home = os.environ.get("JAVA_HOME")
    java = str(Path(home) / "bin" / "java") if home else shutil.which("java")
    if not java:
        print("no java: dhs-unfinished-*.xz left as they are")
        return
    jar = TOOLS.parent / "target" / "xz-1.12.jar"
    if not jar.exists():
        jar.parent.mkdir(exist_ok=True)
        urllib.request.urlretrieve(XZ_JAR, jar)
    with tempfile.TemporaryDirectory() as tmp:
        args = []
        for n in SIZES[1:]:
            src = Path(tmp) / f"in-{n}"
            src.write_bytes(sample(n))
            args += [str(src), str(OUT / f"dhs-unfinished-{n}.xz")]
        subprocess.run([java, "-cp", str(jar), str(TOOLS / "DhsFixtures.java"), *args], check=True)


if __name__ == "__main__":
    main()
