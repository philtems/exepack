#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <lzma.h>

#define CHUNK_SIZE 16384
#define VERSION "1.0"
#define AUTHOR "Philippe TEMESI"
#define YEAR "2026"
#define WEBSITE "https://www.tems.be"
#define PROGRAM_NAME "temsxz"

void print_usage(void) {
    fprintf(stderr, 
        "%s version %s - (c) %s %s\n"
        "Website: %s\n"
        "\n"
        "Usage: %s [OPTIONS]\n"
        "\n"
        "Options:\n"
        "  -c        Compress input stream (compression mode)\n"
        "  -d        Decompress input stream (default mode)\n"
        "  -h        Display this help\n"
        "  -v        Display version information\n"
        "\n"
        "Compression levels (with -c):\n"
        "  0-6       Standard levels (default: 6)\n"
        "  7-9       Higher levels (slower, better ratio)\n"
        "  -e        Extreme mode (adds LZMA_PRESET_EXTREME)\n"
        "\n"
        "Examples:\n"
        "  cat file | %s -c > file.xz      (compression level 6)\n"
        "  cat file | %s -c -9 > file.xz   (maximum compression)\n"
        "  cat file | %s -c -9e > file.xz  (extreme compression)\n"
        "  cat file.xz | %s -d > file      (decompression)\n"
        "\n",
        PROGRAM_NAME, VERSION, AUTHOR, YEAR, WEBSITE,
        PROGRAM_NAME, PROGRAM_NAME, PROGRAM_NAME, PROGRAM_NAME, PROGRAM_NAME);
}

void print_version(void) {
    fprintf(stderr, "%s version %s\n", PROGRAM_NAME, VERSION);
    fprintf(stderr, "© %s %s\n", YEAR, AUTHOR);
    fprintf(stderr, "%s\n", WEBSITE);
}

int main(int argc, char *argv[]) {
    lzma_stream strm = LZMA_STREAM_INIT;
    lzma_ret ret;
    uint8_t inbuf[CHUNK_SIZE];
    uint8_t outbuf[CHUNK_SIZE];
    lzma_action action = LZMA_RUN;
    int compress = -1;  // -1 = undefined, 0 = decompress, 1 = compress
    int level = 6;      // Default level
    int extreme = 0;    // Extreme flag
    int i;
    
    // Parse arguments
    for (i = 1; i < argc; i++) {
        if (strcmp(argv[i], "-h") == 0 || strcmp(argv[i], "--help") == 0) {
            print_usage();
            return 0;
        }
        else if (strcmp(argv[i], "-v") == 0 || strcmp(argv[i], "--version") == 0) {
            print_version();
            return 0;
        }
        else if (strcmp(argv[i], "-c") == 0) {
            compress = 1;
        }
        else if (strcmp(argv[i], "-d") == 0) {
            compress = 0;
        }
        else if (argv[i][0] == '-' && argv[i][1] >= '0' && argv[i][1] <= '9' && argv[i][2] == '\0') {
            level = argv[i][1] - '0';
            if (level < 0 || level > 9) {
                fprintf(stderr, "Error: compression level must be between 0 and 9\n");
                return 1;
            }
        }
        else if (strcmp(argv[i], "-e") == 0) {
            extreme = 1;
        }
        else {
            fprintf(stderr, "Unknown option: %s\n", argv[i]);
            fprintf(stderr, "Use -h for help.\n");
            return 1;
        }
    }
    
    // If no mode specified, default to decompression
    if (compress == -1) {
        compress = 0;
    }
    
    // Initialize stream
    if (compress) {
        // Compression mode
        uint32_t preset = level;
        if (extreme) {
            preset |= LZMA_PRESET_EXTREME;
        }
        
        ret = lzma_easy_encoder(&strm, preset, LZMA_CHECK_CRC64);
        
        if (ret == LZMA_OK) {
            fprintf(stderr, "Compression: level %d%s\n", 
                    level, extreme ? " (extreme)" : "");
        }
    } else {
        // Decompression mode
        ret = lzma_stream_decoder(&strm, UINT64_MAX, LZMA_CONCATENATED);
        fprintf(stderr, "Decompression\n");
    }
    
    if (ret != LZMA_OK) {
        fprintf(stderr, "Initialization error\n");
        return 1;
    }
    
    // Main processing loop
    strm.next_in = inbuf;
    strm.avail_in = 0;
    strm.next_out = outbuf;
    strm.avail_out = sizeof(outbuf);
    
    while (1) {
        // Read more data if needed
        if (strm.avail_in == 0 && !feof(stdin)) {
            strm.next_in = inbuf;
            strm.avail_in = fread(inbuf, 1, sizeof(inbuf), stdin);
            
            if (ferror(stdin)) {
                fprintf(stderr, "Read error\n");
                lzma_end(&strm);
                return 1;
            }
            
            if (feof(stdin)) {
                action = LZMA_FINISH;
            }
        }
        
        // Compress/decompress
        ret = lzma_code(&strm, action);
        
        // Write output
        if (strm.avail_out == 0 || ret != LZMA_OK) {
            size_t write_size = sizeof(outbuf) - strm.avail_out;
            if (write_size > 0) {
                if (fwrite(outbuf, 1, write_size, stdout) != write_size) {
                    fprintf(stderr, "Write error\n");
                    lzma_end(&strm);
                    return 1;
                }
            }
            strm.next_out = outbuf;
            strm.avail_out = sizeof(outbuf);
        }
        
        // Handle end of stream
        if (ret != LZMA_OK) {
            if (ret == LZMA_STREAM_END) {
                break;
            } else {
                fprintf(stderr, "%s error\n", compress ? "Compression" : "Decompression");
                lzma_end(&strm);
                return 1;
            }
        }
    }
    
    lzma_end(&strm);
    return 0;
}
