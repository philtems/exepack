CC = gcc
CFLAGS = -Os -s -static -Wall -Wextra
LDFLAGS = 

all: decompress_gzip.bin decompress_zstd.bin decompress_xz.bin

decompress_gzip.bin: decompress_gzip.c
	$(CC) $(CFLAGS) -o $@ $< -lz
	upx --ultra-brute decompress_gzip.bin
	@echo "  Taille: $$(stat -c %s $@ 2>/dev/null || stat -f %z $@) octets"

decompress_zstd.bin: decompress_zstd.c
	$(CC) $(CFLAGS) -o $@ $< -lzstd
	upx --ultra-brute decompress_zstd.bin
	@echo "  Taille: $$(stat -c %s $@ 2>/dev/null || stat -f %z $@) octets"

decompress_xz.bin: decompress_xz.c
	$(CC) $(CFLAGS) -o $@ $< -llzma
	upx --ultra-brute decompress_xz.bin
	@echo "  Taille: $$(stat -c %s $@ 2>/dev/null || stat -f %z $@) octets"

clean:
	rm -f *.bin

.PHONY: all clean

