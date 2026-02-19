CC = egcc
CFLAGS = -Os -s -static -Wall -Wextra -I/usr/local/include
LDFLAGS = -L/usr/local/lib

all: decompress_gzip.bin decompress_zstd.bin decompress_xz.bin

decompress_gzip.bin: decompress_gzip.c
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $< -lz
	@echo "  Taille: $$(stat -f %z $@ 2>/dev/null || stat -c %s $@) octets"

decompress_zstd.bin: decompress_zstd.c
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $< -lzstd
	@echo "  Taille: $$(stat -f %z $@ 2>/dev/null || stat -c %s $@) octets"

decompress_xz.bin: decompress_xz.c
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $< -llzma
	@echo "  Taille: $$(stat -f %z $@ 2>/dev/null || stat -c %s $@) octets"

clean:
	rm -f *.bin

.PHONY: all clean

