/* temsxz_decomp_only.c - Version ultra-compacte pour embarquement */
#include "xz.h"
#include <stdio.h>
#include <unistd.h>

#define CHUNK_SIZE 16384

int main(void) {
    struct xz_buf buf;
    struct xz_dec *s;
    enum xz_ret ret;
    uint8_t inbuf[CHUNK_SIZE];
    uint8_t outbuf[CHUNK_SIZE];
    
    /* Initialisation du décodeur - mode single-call ? Non, multi-call avec allocation dynamique */
    /* Pour décompresser des fichiers de taille arbitraire, on utilise le mode multi-call */
    s = xz_dec_init(XZ_DYNALLOC, 1U << 20); /* Max 1 MiB de mémoire */
    if (s == NULL) {
        fprintf(stderr, "Memory allocation error\n");
        return 1;
    }
    
    buf.in = inbuf;
    buf.in_pos = 0;
    buf.in_size = 0;
    buf.out = outbuf;
    buf.out_pos = 0;
    buf.out_size = sizeof(outbuf);
    
    do {
        /* Lire plus de données si nécessaire */
        if (buf.in_pos == buf.in_size && !feof(stdin)) {
            buf.in_size = fread(inbuf, 1, sizeof(inbuf), stdin);
            buf.in_pos = 0;
            if (ferror(stdin)) {
                fprintf(stderr, "Read error\n");
                xz_dec_end(s);
                return 1;
            }
        }
        
        ret = xz_dec_run(s, &buf);
        
        /* Écrire la sortie */
        if (buf.out_pos > 0) {
            if (fwrite(outbuf, 1, buf.out_pos, stdout) != buf.out_pos) {
                fprintf(stderr, "Write error\n");
                xz_dec_end(s);
                return 1;
            }
            buf.out_pos = 0;
        }
        
        if (ret == XZ_STREAM_END) {
            break;
        }
        
        if (ret != XZ_OK) {
            fprintf(stderr, "Decompression error: %d\n", ret);
            xz_dec_end(s);
            return 1;
        }
        
    } while (buf.in_size > 0 || !feof(stdin));
    
    xz_dec_end(s);
    return 0;
}

