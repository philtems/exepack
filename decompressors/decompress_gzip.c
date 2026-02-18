/* Décompresseur gzip minimal pour rust-gzexe
 * Compilation: gcc -Os -static -s -o decompress_gzip.bin decompress_gzip.c -lz
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <zlib.h>

#define CHUNK 16384
#define GZIP_WINDOW_BITS 16 + MAX_WBITS

int main() {
    z_stream strm;
    unsigned char in[CHUNK];
    unsigned char out[CHUNK];
    int ret;
    
    /* Initialisation */
    memset(&strm, 0, sizeof(strm));
    ret = inflateInit2(&strm, GZIP_WINDOW_BITS);
    if (ret != Z_OK) {
        fprintf(stderr, "Erreur initialisation zlib\n");
        return 1;
    }
    
    /* Décompression */
    do {
        strm.avail_in = fread(in, 1, CHUNK, stdin);
        if (ferror(stdin)) {
            inflateEnd(&strm);
            fprintf(stderr, "Erreur lecture entrée\n");
            return 1;
        }
        strm.next_in = in;
        
        do {
            strm.avail_out = CHUNK;
            strm.next_out = out;
            ret = inflate(&strm, Z_NO_FLUSH);
            
            if (ret == Z_STREAM_ERROR || ret == Z_DATA_ERROR || ret == Z_MEM_ERROR) {
                inflateEnd(&strm);
                fprintf(stderr, "Erreur décompression: %d\n", ret);
                return 1;
            }
            
            fwrite(out, 1, CHUNK - strm.avail_out, stdout);
            if (ferror(stdout)) {
                inflateEnd(&strm);
                fprintf(stderr, "Erreur écriture sortie\n");
                return 1;
            }
        } while (strm.avail_out == 0);
        
    } while (ret != Z_STREAM_END);
    
    inflateEnd(&strm);
    return 0;
}

