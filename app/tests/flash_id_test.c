/* SPDX-License-Identifier: GPL-3.0-only */
/* US-50: the flash chips the app (platform.c, recovery.c), its update loader too (loader.c), accept: FL_JEDEC_OK,
 * the FM-1's two parts, nothing else. (The build checks both ids are compared in the app's code, the loader's too.) */
#include <stdio.h>
#include <stdint.h>
#include "fm1_flash_map.h"

int main(void)
{
    static const uint32_t ok[] = {0x856014u, 0xB36014u};               /* Puya P25Q80; UC25HQ80 (newer units) */
    static const uint32_t bad[] = {0u, 0xFFFFFFu, 0xFFFFFFFFu, 0xEF4014u, 0xEB6014u, 0xC84014u, 0x856015u, 0xB36015u, 0x0B6014u};
    unsigned i, fails = 0;
    for (i = 0; i < sizeof ok / sizeof ok[0]; i++)
        if (!FL_JEDEC_OK(ok[i])) { printf("FAIL: FM-1 flash %06X refused\n", (unsigned)ok[i]); fails++; }
    for (i = 0; i < sizeof bad / sizeof bad[0]; i++)
        if (FL_JEDEC_OK(bad[i])) { printf("FAIL: unknown flash %06X accepted\n", (unsigned)bad[i]); fails++; }
    printf(fails ? "FAILED\n" : "ok\n");
    return fails ? 1 : 0;
}
