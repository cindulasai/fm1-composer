/* SPDX-License-Identifier: GPL-3.0-only */
/* render_piece IN.f1p OUT.wav [SECONDS]: play a piece through COMPOSER's player and synth (the firmware's own
 * code, on this computer) into a 44.1 kHz stereo WAV. Prints the peak, the RMS and the clipped share. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "synth.c"
#include "player.c"

static void put32le(FILE *f, uint32_t v) { fputc((int)(v & 255u), f); fputc((int)(v >> 8 & 255u), f); fputc((int)(v >> 16 & 255u), f); fputc((int)(v >> 24), f); }

int main(int argc, char **argv)
{
    static uint8_t buf[PIECE_MAX_BYTES + 1];
    FILE *f;
    size_t n;
    int e;
    uint32_t secs, frames, fr, i, ch, clipped = 0;
    int32_t peak = 0;
    double sq = 0;
    if (argc == 2 && !strcmp(argv[1], "--sounds")) {           /* the catalog, in PRESETS order */
        synth_init();
        for (i = 0; i < sound_count; i++) printf("%s\n", sound_name(i));
        return 0;
    }
    if (argc < 3) { fprintf(stderr, "usage: render_piece IN.f1p OUT.wav [SECONDS]\n"); return 2; }
    if (!(f = fopen(argv[1], "rb"))) { perror(argv[1]); return 2; }
    n = fread(buf, 1, sizeof buf, f);
    fclose(f);
    if ((e = piece_check(buf, (uint32_t)n))) { fprintf(stderr, "%s: not a valid piece (error %d)\n", argv[1], e); return 2; }
    synth_init();
    player_load(buf);
    for (ch = 0; ch < pl_v.nch; ch++) {
        uint32_t s = pl_v.ch[ch].sound;
        synth_role(ch, pl_v.ch[ch].role);
        synth_sound(ch, s < sound_count ? s : sound_default(pl_v.ch[ch].role));
        synth_level(ch, pl_v.ch[ch].vol, pl_v.ch[ch].pan);
    }
    pl_loop = 0;
    player_play();
    secs = argc > 3 ? (uint32_t)atoi(argv[3]) : (uint32_t)((uint64_t)pl_v.len_ticks * 600u / pl_v.tempo_x10 / 96u + 3u);
    frames = secs * 44100u / CTL * CTL;
    if (!(f = fopen(argv[2], "wb"))) { perror(argv[2]); return 2; }
    fwrite("RIFF", 1, 4, f); put32le(f, 36 + frames * 4); fwrite("WAVEfmt ", 1, 8, f);
    put32le(f, 16); put32le(f, 0x00020001u); put32le(f, 44100); put32le(f, 44100 * 4); put32le(f, 0x00100004u);
    fwrite("data", 1, 4, f); put32le(f, frames * 4);
    for (fr = 0; fr < frames; fr += CTL) {
        int32_t out[2 * CTL];
        player_tick(CTL);
        memset(out, 0, sizeof out);
        synth_apply();
        synth_render(out, CTL);
        for (i = 0; i < 2 * CTL; i++) {
            int32_t a = out[i] < 0 ? -out[i] : out[i];
            if (a > peak) peak = a;
            if (a >= 32767) clipped++;
            sq += (double)out[i] * out[i];
            fputc(out[i] & 255, f); fputc(out[i] >> 8 & 255, f);
        }
    }
    fclose(f);
    printf("%s: %u s, peak %d, rms %.0f, clipped %.4f%%, voices dropped %u\n", argv[2], secs, peak,
           __builtin_sqrt(sq / (frames * 2.0)), 100.0 * clipped / (frames * 2.0), pl_dropped);
    return 0;
}
