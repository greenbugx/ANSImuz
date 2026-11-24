/*
 * Integration test verifying natural EOF (end of file) playback completion.
 * Loads an audio track, seeks near the end, and ensures libmpv completes
 * playback with an ANSIMUZ_MPV_END_EOF reason instead of an unexpected error.
 */

#include <stdio.h>
#include <stdint.h>
#include "test_common.h"
#include "ansimuz_mpv.h"

int main(void)
{
    ansimuz_mpv *player = NULL;
    char error[256] = {0};

    /* Initialize a fresh headless player instance */
    if (ansimuz_mpv_create(&player, error, sizeof(error)) != 0) {
        fprintf(stderr, "[FAIL] Init: %s\n", error);
        return 1;
    }

    printf("[OK] Backend initialized\n");

    /* Load test audio file */
    printf("\n--- Test 1: Load music2.mp3 ---\n");
    ensure_test_audio_file("native/mpv/music2.mp3", 10);

    if (ansimuz_mpv_loadfile(
            player, "native/mpv/music2.mp3") != 0) {
        fprintf(stderr, "[FAIL] loadfile failed\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    /* Track the expected load ID assigned on dispatch */
    uint64_t expected_id = ansimuz_mpv_get_last_load_id(player);
    printf("[LOAD] expected load_id=%llu\n",
           (unsigned long long)expected_id);

    int file_loaded = 0;
    int playback_restarted = 0;
    int seek_requested = 0;
    int end_file = 0;
    int eof_received = 0;

    double duration = 0.0;

    /* Wait for stream demuxing, start notification, and duration */
    printf("\n--- Test 2: Wait for playback and duration ---\n");

    for (int i = 0; i < 100; i++) {
        ansimuz_mpv_event event;

        while (ansimuz_mpv_poll_event(player, &event) > 0) {
            printf("[EVENT] type=%d load_id=%llu\n",
                   event.type,
                   (unsigned long long)event.load_id);

            /* Ignore events belonging to other load sequences */
            if (event.load_id != expected_id)
                continue;

            if (event.type == ANSIMUZ_MPV_EVENT_FILE_LOADED)
                file_loaded = 1;

            if (event.type == ANSIMUZ_MPV_EVENT_PLAYBACK_RESTART)
                playback_restarted = 1;

            if (event.type == ANSIMUZ_MPV_EVENT_END_FILE) {
                end_file = 1;
                printf("[END_FILE] reason=%d error=%d\n",
                       event.end_reason, event.error_code);
                break;
            }
        }

        if (end_file)
            break;

        /* Break early once demuxer resolves a valid non trivial duration */
        if (file_loaded && playback_restarted &&
            ansimuz_mpv_get_duration(player, &duration) == 0 &&
            duration > 5.0) {
            break;
        }

        sleep_ms(100);
    }

    if (!file_loaded || !playback_restarted || duration <= 5.0) {
        fprintf(stderr,
                "[FAIL] Playback or duration was not ready "
                "(loaded=%d restarted=%d duration=%.2f)\n",
                file_loaded, playback_restarted, duration);
        ansimuz_mpv_destroy(player);
        return 1;
    }

    printf("[OK] Duration: %.2f seconds\n", duration);

    /* Seek near the end of the track to observe natural completion */
    printf("\n--- Test 3: Seek near the end ---\n");

    double target = duration - 2.0;

    if (ansimuz_mpv_seek(player, target) != 0) {
        fprintf(stderr, "[FAIL] Seek failed\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    printf("[SEEK] Requested position: %.2f seconds\n", target);
    seek_requested = 1;

    /* Wait for playback to reach natural EOF */
    printf("\n--- Test 4: Wait for natural EOF ---\n");

    for (int i = 0; i < 150; i++) {
        ansimuz_mpv_event event;

        while (ansimuz_mpv_poll_event(player, &event) > 0) {
            printf("[EVENT] type=%d load_id=%llu\n",
                   event.type,
                   (unsigned long long)event.load_id);

            if (event.load_id != expected_id)
                continue;

            /* Check that END_FILE carries the natural EOF termination reason */
            if (event.type == ANSIMUZ_MPV_EVENT_END_FILE) {
                end_file = 1;

                printf("[END_FILE] reason=%d error=%d\n",
                       event.end_reason, event.error_code);

                if (event.end_reason == ANSIMUZ_MPV_END_EOF)
                    eof_received = 1;

                break;
            }
        }

        if (end_file)
            break;

        sleep_ms(100);
    }

    /* Report results and clean up player instance */
    printf("\n--- Results ---\n");
    printf("FILE_LOADED: %s\n", file_loaded ? "YES" : "NO");
    printf("PLAYBACK_RESTART: %s\n",
           playback_restarted ? "YES" : "NO");
    printf("SEEK requested: %s\n", seek_requested ? "YES" : "NO");
    printf("END_FILE received: %s\n", end_file ? "YES" : "NO");
    printf("Natural EOF received: %s\n",
           eof_received ? "YES" : "NO");

    ansimuz_mpv_destroy(player);

    if (eof_received) {
        printf("\n[PASS] Natural EOF test passed!\n");
        return 0;
    }

    printf("\n[FAIL] Natural EOF test failed.\n");
    return 1;
}
