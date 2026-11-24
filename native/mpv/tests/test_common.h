#ifndef ANSIMUZ_TEST_COMMON_H
#define ANSIMUZ_TEST_COMMON_H

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

#ifdef _WIN32
#include <windows.h>
#define sleep_ms(ms) Sleep(ms)
#define wait_ms(ms) Sleep(ms)

static inline uint32_t get_tick_ms(void) {
    return (uint32_t)GetTickCount();
}
#else
#include <time.h>
#include <unistd.h>

static inline void sleep_ms(unsigned int ms) {
    struct timespec ts;
    ts.tv_sec = ms / 1000;
    ts.tv_nsec = (long)(ms % 1000) * 1000000L;
    nanosleep(&ts, NULL);
}

#define wait_ms(ms) sleep_ms(ms)

static inline uint32_t get_tick_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}
#endif

static inline void ensure_test_audio_file(const char *path, uint32_t secs) {
    FILE *check = fopen(path, "rb");
    if (check) {
        fseek(check, 0, SEEK_END);
        long sz = ftell(check);
        fclose(check);
        if (sz > 44) {
            return;
        }
    }

    FILE *f = fopen(path, "wb");
    if (!f) return;

    uint32_t rate = 44100;
    uint32_t data_len = rate * secs * 2;
    uint32_t riff_len = 36 + data_len;
    uint16_t channels = 1;
    uint16_t bits = 16;
    uint32_t byte_rate = rate * 2;
    uint16_t block_align = 2;
    uint32_t fmt_len = 16;
    uint16_t audio_fmt = 1;

    fwrite("RIFF", 1, 4, f);
    fwrite(&riff_len, 4, 1, f);
    fwrite("WAVEfmt ", 1, 8, f);
    fwrite(&fmt_len, 4, 1, f);
    fwrite(&audio_fmt, 2, 1, f);
    fwrite(&channels, 2, 1, f);
    fwrite(&rate, 4, 1, f);
    fwrite(&byte_rate, 4, 1, f);
    fwrite(&block_align, 2, 1, f);
    fwrite(&bits, 2, 1, f);
    fwrite("data", 1, 4, f);
    fwrite(&data_len, 4, 1, f);

    uint8_t zero[1024] = {0};
    uint32_t remaining = data_len;
    while (remaining > 0) {
        uint32_t chunk = remaining > sizeof(zero) ? (uint32_t)sizeof(zero) : remaining;
        fwrite(zero, 1, chunk, f);
        remaining -= chunk;
    }
    fclose(f);
}

#endif

