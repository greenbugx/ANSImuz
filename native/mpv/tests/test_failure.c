/*
 * Integration test exercising error handling and engine recovery.
 * Intentionally requests a missing file to verify that libmpv delivers
 * a load failure END_FILE event with proper load ID attribution, and then
 * loads a valid track to ensure the player instance recovers cleanly.
 */

#include <stdio.h>
#include <string.h>
#include "test_common.h"
#include "ansimuz_mpv.h"

/*
 * Helper to poll and log events across a fixed duration window.
 */
static void drain_events(ansimuz_mpv *player, int duration_ms) {
    for (int elapsed = 0; elapsed < duration_ms; elapsed += 50) {
        ansimuz_mpv_event event;
        int result = ansimuz_mpv_poll_event(player, &event);

        if (result < 0) {
            printf("[POLL ERROR] %s\n",
                   ansimuz_mpv_error_string(result));
            return;
        }

        if (result == 1) {
            printf("[EVENT] type=%d load_id=%llu",
               event.type,
               (unsigned long long)event.load_id);

            /* Log detailed status for track end notifications */
            if (event.type == ANSIMUZ_MPV_EVENT_END_FILE) {
                printf(" reason=%d error=%d",
                       event.end_reason,
                       event.error_code);
            }

            /* Log native asynchronous engine errors */
            if (event.type == ANSIMUZ_MPV_EVENT_ERROR) {
                printf(" error=%d", event.error_code);
            }

            printf("\n");
        }

        wait_ms(50);
    }
}

int main(void) {
    ansimuz_mpv *player = NULL;
    char error[512] = {0};

    /* Initialize a fresh player instance */
    int status = ansimuz_mpv_create(
        &player, error, sizeof(error)
    );

    if (status < 0) {
        printf("[FATAL] Initialization failed: %s\n", error);
        return 1;
    }

    printf("[OK] Backend initialized\n");

    /* Test 1: Intentionally load a nonexistent file to test error path */
    printf("\n--- Test 1: Failed file load ---\n");

    status = ansimuz_mpv_loadfile(
        player,
        "native/mpv/THIS_FILE_DOES_NOT_EXIST.mp3"
    );

    printf("[LOAD] status=%d load_id=%llu\n",
           status,
           (unsigned long long)ansimuz_mpv_get_last_load_id(player));

    /* Drain events to observe the failure END_FILE event */
    drain_events(player, 3000);

    /* Test 2: Load a valid file after failure to verify recovery */
    printf("\n--- Test 2: Recovery with valid file ---\n");
    ensure_test_audio_file("native/mpv/music1.mp3", 10);

    status = ansimuz_mpv_loadfile(
        player,
        "native/mpv/music1.mp3"
    );

    printf("[LOAD] status=%d load_id=%llu\n",
           status,
           (unsigned long long)ansimuz_mpv_get_last_load_id(player));

    /* Drain events to observe successful file loading and playback start */
    drain_events(player, 5000);

    /* Clean shutdown */
    ansimuz_mpv_destroy(player);

    printf("\n[OK] Backend destroyed cleanly\n");
    return 0;
}
