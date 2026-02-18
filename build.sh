#!/bin/sh
# build.sh - Compile tout le projet

set -e

echo "=== Compilation du décompresseur C minimal ==="
cd decompress

# Compiler le décompresseur unique qui gère tous les formats
echo "Compilation du décompresseur C..."
gcc -Os -static -s -o decompress decompress.c -lz -lzstd -llzma
upx --ultra-brute decompress

# Vérifier la taille
SIZE=$(stat -c %s decompress 2>/dev/null || stat -f %z decompress)
echo "Taille du décompresseur: $SIZE octets"

cd ..

echo "=== Compilation de l'outil Rust ==="
cargo clean
cargo build --release

echo "=== Résultat final ==="
ls -la target/release/tems-exepack
ls -la decompress/decompress
