/* Décompresseur xz minimal pour rust-gzexe
 * Compilation: gcc -Os -static -s -o decompress_xz.bin decompress_xz.c -llzma
 */

#include <stdio.h>
#include <stdlib.h>
#include <lzma.h>

#define CHUNK 16384

int main() {
    lzma_stream strm = LZMA_STREAM_INIT;
    lzma_ret ret;
    
    ret = lzma_stream_decoder(&strm, UINT64_MAX, LZMA_CONCATENATED);
    if (ret != LZMA_OK) {
        fprintf(stderr, "Erreur initialisation décodeur xz\n");
        return 1;
    }
    
    unsigned char in[CHUNK];
    unsigned char out[CHUNK];
    lzma_action action = LZMA_RUN;
    
    while (1) {
        if (strm.avail_in == 0) {
            strm.next_in = in;
            strm.avail_in = fread(in, 1, CHUNK, stdin);
            if (ferror(stdin)) {
                lzma_end(&strm);
                fprintf(stderr, "Erreur lecture entrée\n");
                return 1;
            }
            if (strm.avail_in == 0) action = LZMA_FINISH;
        }
        
        strm.next_out = out;
        strm.avail_out = CHUNK;
        ret = lzma_code(&strm, action);
        
        if (ret != LZMA_OK && ret != LZMA_STREAM_END) {
            lzma_end(&strm);
            fprintf(stderr, "Erreur décompression xz: %d\n", ret);
            return 1;
        }
        
        fwrite(out, 1, CHUNK - strm.avail_out, stdout);
        if (ferror(stdout)) {
            lzma_end(&strm);
            fprintf(stderr, "Erreur écriture sortie\n");
            return 1;
        }
        
        if (ret == LZMA_STREAM_END) break;
    }
    
    lzma_end(&strm);
    return 0;
}

