# zexe

**Self-extracting executable compressor** • Inspired by OpenBSD `gzexe`

zexe compresses executable files using **Zopfli** (maximum gzip-compatible compression) and wraps them in a small shell header that decompresses and executes the original program on-the-fly. The result is a standalone, self-extracting binary that runs exactly like the original but takes up significantly less space.

---

## Features

- **Maximum compression** – Uses the Zopfli algorithm (15 iterations, dynamic blocks) for 3–8% better ratios than `gzip -9`
- **Self-extracting** – Compressed files are still directly executable; they decompress themselves to a temporary location and run
- **Portable** – Works on Linux, macOS, and BSD (POSIX‑compliant shell + standard `gzip` required for decompression)
- **Safe** – Performs sanity checks (executable, no setuid/setgid, avoids compressing critical system tools)
- **Detailed stats** – Shows original size, compressed size, and compression ratio
- **Restore** – Use `-d` to revert a compressed file back to its original state

---

### Compress an executable
zexe /path/to/program

### Decompress back to original
zexe -d /path/to/program

### Show help
zexe -h

### Show version
zexe -V
