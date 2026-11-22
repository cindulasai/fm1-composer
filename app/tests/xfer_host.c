/* SPDX-License-Identifier: GPL-3.0-only */
/* xfer_host FLASHFILE: COMPOSER's transfer code and library on this computer, for fm1c's tests
 * (`fm1c push --port exec:...`). The flash lives in FLASHFILE between runs. Reads lines on stdin:
 *   F0 .. F7     a SysEx frame in hex (spaces optional) -> the replies, one hex line each
 *   play 0|1     playback stopped / running (a push while running is staged)
 *   wait MS      the clock moves on */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "piece.h"
#include "store.h"

static uint8_t fl[LIB_SECT * 4096u];
static int dirty;
static int sim_read(uint32_t off, void *p, uint32_t n) { if (off < LIB_AT || off - LIB_AT > sizeof fl || n > sizeof fl - (off - LIB_AT)) return -1; memcpy(p, fl + (off - LIB_AT), n); return 0; }
static int sim_erase(uint32_t off) { if (off < LIB_AT || off - LIB_AT >= sizeof fl || (off & 4095u)) return -1; memset(fl + (off - LIB_AT), 0xFF, 4096); dirty = 1; return 0; }
static int sim_write(uint32_t off, const void *p, uint32_t n)
{
    uint32_t i;
    if (off < LIB_AT || off - LIB_AT > sizeof fl || n > sizeof fl - (off - LIB_AT)) return -1;
    for (i = 0; i < n; i++) fl[(off - LIB_AT) + i] &= ((const uint8_t *)p)[i];
    dirty = 1;
    return 0;
}
#define LIB_READ(off, p, n) sim_read(off, p, n)
#define LIB_ERASE(off) sim_erase(off)
#define LIB_WRITE(off, p, n) sim_write(off, p, n)
#include "lib.c"
static void out_frame(const uint8_t *p, uint32_t n) { uint32_t i; for (i = 0; i < n; i++) printf("%02X", p[i]); printf("\n"); fflush(stdout); }
#define XFER_SEND(p, n) out_frame(p, n)
#define XF_POOL
#include "xfer.c"

int main(int argc, char **argv)
{
    static char line[4096];
    static uint8_t f[2048];
    uint32_t now = 1, playing = 0;
    FILE *fp;
    if (argc != 2) { fprintf(stderr, "usage: xfer_host FLASHFILE\n"); return 2; }
    memset(fl, 0xFF, sizeof fl);
    if ((fp = fopen(argv[1], "rb"))) { if (fread(fl, 1, sizeof fl, fp) != sizeof fl) memset(fl, 0xFF, sizeof fl); fclose(fp); }
    lib_mount();
    while (fgets(line, sizeof line, stdin)) {
        uint32_t v, n = 0;
        const char *s = line;
        if (sscanf(line, "play %u", &v) == 1) playing = v;
        else if (sscanf(line, "wait %u", &v) == 1) now += v;
        else {
            unsigned b;
            int k;
            while (n < sizeof f && sscanf(s, " %2x%n", &b, &k) == 1) { f[n++] = (uint8_t)b; s += k; }
            if (n >= 2 && f[0] == 0xF0 && f[n - 1] == 0xF7) xfer_frame(f + 1, n - 2, now, (int)playing);
        }
        xfer_poll(now, (int)playing);
        if (dirty && (fp = fopen(argv[1], "wb"))) { fwrite(fl, 1, sizeof fl, fp); fclose(fp); dirty = 0; }
        printf(".\n");                          /* (this line is done) */
        fflush(stdout);
    }
    return 0;
}
