/* Décompresseur zstd minimal pour rust-gzexe
 * Compilation: gcc -Os -static -s -o decompress_zstd.bin decompress_zstd.c -lzstd
 */

#include <stdio.h>
#include <stdlib.h>
#include <zstd.h>

#define CHUNK 16384

int main() {
    ZSTD_DCtx* ctx = ZSTD_createDCtx();
    if (!ctx) {
        fprintf(stderr, "Erreur création contexte zstd\n");
        return 1;
    }
    
    unsigned char in[CHUNK];
    unsigned char out[CHUNK];
    ZSTD_inBuffer input = {in, 0, 0};
    size_t last_ret = 0;
    
    while (1) {
        if (input.pos >= input.size) {
            input.size = fread(in, 1, CHUNK, stdin);
            if (ferror(stdin)) {
                ZSTD_freeDCtx(ctx);
                fprintf(stderr, "Erreur lecture entrée\n");
                return 1;
            }
            if (input.size == 0) break;
            input.pos = 0;
        }
        
        ZSTD_outBuffer output = {out, CHUNK, 0};
        last_ret = ZSTD_decompressStream(ctx, &output, &input);
        
        if (ZSTD_isError(last_ret)) {
            ZSTD_freeDCtx(ctx);
            fprintf(stderr, "Erreur décompression: %s\n", ZSTD_getErrorName(last_ret));
            return 1;
        }
        
        fwrite(out, 1, output.pos, stdout);
        if (ferror(stdout)) {
            ZSTD_freeDCtx(ctx);
            fprintf(stderr, "Erreur écriture sortie\n");
            return 1;
        }
        
        if (last_ret == 0) break;
    }
    
    ZSTD_freeDCtx(ctx);
    return 0;
}

