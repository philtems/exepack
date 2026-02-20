# Makefile for temsxz - TEMS XZ Compressor/Decompressor
# (c) 2026 Philippe TEMESI - https://www.tems.be

PROGRAM = temsxz
VERSION = 1.0
AUTHOR = "Philippe TEMESI"
YEAR = 2026

# Compiler and flags
CC = gcc
CFLAGS = -O2 -s -Os -ffunction-sections -fdata-sections -Wl,--gc-sections
CFLAGS += -Wall -Wextra -DVERSION=\"$(VERSION)\" -DAUTHOR=\"$(AUTHOR)\" -DYEAR=$(YEAR)
LDFLAGS = -Wl,-s -Wl,--as-needed

# Libraries
LIBS = -llzma

# Installation directories
PREFIX ?= /usr/local
BINDIR = $(PREFIX)/bin
MANDIR = $(PREFIX)/share/man/man1

# Source files
SRC = temsxz.c
OBJ = $(SRC:.c=.o)

# Default target
all: $(PROGRAM)

# Create a smaller executable with various optimizations
$(PROGRAM): $(OBJ)
	$(CC) $(CFLAGS) -o $@ $^ $(LDFLAGS) $(LIBS)
	@echo "✅ Build complete: $(PROGRAM)"
	@echo "   File size: $$(stat -c%s $(PROGRAM) 2>/dev/null || stat -f%z $(PROGRAM) 2>/dev/null) bytes"
	@strip --strip-all $(PROGRAM) 2>/dev/null || true
	@echo "   Stripped size: $$(stat -c%s $(PROGRAM) 2>/dev/null || stat -f%z $(PROGRAM) 2>/dev/null) bytes"

# Alternative: Ultra small with musl libc (static, very small)
musl:
	@echo "Building with musl-libc for maximum portability..."
	musl-gcc -static -Os -s -ffunction-sections -fdata-sections \
		-Wl,--gc-sections -Wl,-s -DVERSION=\"$(VERSION)\" \
		-DAUTHOR=\"$(AUTHOR)\" -DYEAR=$(YEAR) \
		-o $(PROGRAM)-musl $(SRC) -llzma
	@echo "✅ Musl build complete: $(PROGRAM)-musl"
	@echo "   File size: $$(stat -c%s $(PROGRAM)-musl 2>/dev/null || stat -f%z $(PROGRAM)-musl 2>/dev/null) bytes"

# Alternative: Dynamic linking with specific optimizations
dynamic: CFLAGS += -fomit-frame-pointer -mfpmath=sse -msse2
dynamic: $(PROGRAM)

# Compile with UPX compression (if available)
upx: $(PROGRAM)
	@if command -v upx >/dev/null 2>&1; then \
		echo "Compressing with UPX..."; \
		upx --ultra-brute $(PROGRAM); \
	else \
		echo "UPX not found. Install upx for additional compression."; \
	fi

# Build all variants
all-variants: $(PROGRAM) musl upx

# Object file compilation
%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

# Clean build files
clean:
	rm -f $(OBJ) $(PROGRAM) $(PROGRAM)-musl $(PROGRAM)-*.o
	@echo "✅ Clean complete"

# Install the program
install: $(PROGRAM)
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 $(PROGRAM) $(DESTDIR)$(BINDIR)/
	@echo "✅ Installed $(PROGRAM) to $(DESTDIR)$(BINDIR)/"

# Uninstall the program
uninstall:
	rm -f $(DESTDIR)$(BINDIR)/$(PROGRAM)
	@echo "✅ Uninstalled $(PROGRAM)"

# Create a man page (optional)
man:
	@echo "Creating man page..."
	@mkdir -p man
	@echo '.TH TEMSXZ 1 "$(YEAR)" "Version $(VERSION)" "User Commands"' > man/$(PROGRAM).1
	@echo '.SH NAME' >> man/$(PROGRAM).1
	@echo 'temsxz \- TEMS XZ compressor/decompressor' >> man/$(PROGRAM).1
	@echo '.SH SYNOPSIS' >> man/$(PROGRAM).1
	@echo '.B temsxz [OPTIONS]' >> man/$(PROGRAM).1
	@echo '.SH DESCRIPTION' >> man/$(PROGRAM).1
	@echo 'Compress or decompress data using LZMA algorithm.' >> man/$(PROGRAM).1
	@echo '.SH OPTIONS' >> man/$(PROGRAM).1
	@echo '.TP' >> man/$(PROGRAM).1
	@echo '\-c' >> man/$(PROGRAM).1
	@echo 'Compress input stream.' >> man/$(PROGRAM).1
	@echo '.TP' >> man/$(PROGRAM).1
	@echo '\-d' >> man/$(PROGRAM).1
	@echo 'Decompress input stream (default).' >> man/$(PROGRAM).1
	@echo '.TP' >> man/$(PROGRAM).1
	@echo '\-h' >> man/$(PROGRAM).1
	@echo 'Display help.' >> man/$(PROGRAM).1
	@echo '.TP' >> man/$(PROGRAM).1
	@echo '\-v' >> man/$(PROGRAM).1
	@echo 'Display version.' >> man/$(PROGRAM).1
	@echo '.TP' >> man/$(PROGRAM).1
	@echo '\-e' >> man/$(PROGRAM).1
	@echo 'Extreme compression mode.' >> man/$(PROGRAM).1
	@echo '.TP' >> man/$(PROGRAM).1
	@echo '0-9' >> man/$(PROGRAM).1
	@echo 'Compression level (default: 6).' >> man/$(PROGRAM).1
	@echo '.SH SEE ALSO' >> man/$(PROGRAM).1
	@echo '.BR xz (1), gzip (1), bzip2 (1)' >> man/$(PROGRAM).1
	@echo '.SH AUTHOR' >> man/$(PROGRAM).1
	@echo 'Philippe TEMESI - $(WEBSITE)' >> man/$(PROGRAM).1
	@echo "✅ Man page created: man/$(PROGRAM).1"

install-man: man
	install -d $(DESTDIR)$(MANDIR)
	install -m 644 man/$(PROGRAM).1 $(DESTDIR)$(MANDIR)/
	@echo "✅ Man page installed"

# Show help
help:
	@echo "Available targets:"
	@echo "  all          : Build $(PROGRAM) (default)"
	@echo "  musl         : Build with musl-libc (static)"
	@echo "  dynamic      : Build with dynamic optimizations"
	@echo "  upx          : Build and compress with UPX"
	@echo "  all-variants : Build all variants"
	@echo "  clean        : Remove build files"
	@echo "  install      : Install $(PROGRAM)"
	@echo "  uninstall    : Remove $(PROGRAM)"
	@echo "  man          : Create man page"
	@echo "  install-man  : Install man page"
	@echo "  help         : Show this help"

# Show version
version:
	@echo "$(PROGRAM) version $(VERSION)"
	@echo "Copyright (c) $(YEAR) $(AUTHOR)"
	@echo "$(WEBSITE)"

# Test the program
test: $(PROGRAM)
	@echo "Testing compression/decompression..."
	@echo "Hello, world! This is a test." > testfile.txt
	@./$(PROGRAM) -c < testfile.txt > testfile.txt.xz
	@./$(PROGRAM) -d < testfile.txt.xz > testfile.txt.out
	@if cmp -s testfile.txt testfile.txt.out; then \
		echo "✅ Test passed: compression/decompression works!"; \
	else \
		echo "❌ Test failed: output differs"; \
	fi
	@rm -f testfile.txt testfile.txt.xz testfile.txt.out

.PHONY: all clean install uninstall man install-man help version test musl dynamic upx all-variants

# Default target
.DEFAULT_GOAL = all

