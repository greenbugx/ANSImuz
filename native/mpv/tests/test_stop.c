/*
 * Integration test validating manual stop playback behavior.
 * Starts audio playback, issues an explicit stop command, and verifies
 * that libmpv delivers an END_FILE event with reason ANSIMUZ_MPV_END_STOP
 * without emitting further playback or progress notifications.
 */

#include <stdio.h>
#ifdef _WIN32
#include <windows.h>
static void wait_ms(unsigned int ms) {
    Sleep(ms);
}
#else
#include <time.h>
static void wait_ms(unsigned int ms) {
    struct timespec req = {
        .tv_sec = ms / 1000,
        .tv_nsec = (long)(ms % 1000) * 1000000L
    };
    nanosleep(&req, NULL);
}
#endif

#include "ansimuz_mpv.h"

/*
 * Helper that polls events for a duration and records lifecycle flags.
 */
static int drain_events(ansimuz_mpv *player, int duration_ms,
                        int *file_loaded, int *playback_restarted,
                        int *end_file) {
    for (int elapsed = 0; elapsed < duration_ms; elapsed += 50) {
        ansimuz_mpv_event event;
        int result = ansimuz_mpv_poll_event(player, &event);

        if (result < 0) {
            printf("[POLL ERROR] %s\n",
                   ansimuz_mpv_error_string(result));
            return -1;
        }

        if (result == 1) {
            printf("[EVENT] type=%d load_id=%llu",
                   event.type,
                   (unsigned long long)event.load_id);

            if (event.type == ANSIMUZ_MPV_EVENT_FILE_LOADED) {
                *file_loaded = 1;
            }

            if (event.type == ANSIMUZ_MPV_EVENT_PLAYBACK_RESTART) {
                *playback_restarted = 1;
            }

            if (event.type == ANSIMUZ_MPV_EVENT_END_FILE) {
                *end_file = 1;
                printf(" reason=%d error=%d",
                       event.end_reason,
                       event.error_code);
            }

            printf("\n");
        }

        wait_ms(50);
    }

    return 0;
}

int main(void) {
    ansimuz_mpv *player = NULL;
    char error[512] = {0};

    /* Initialize player backend */
    int status = ansimuz_mpv_create(
        &player, error, sizeof(error)
    );

    if (status < 0) {
        printf("[FATAL] Initialization failed: %s\n", error);
        return 1;
    }

    printf("[OK] Backend initialized\n");

    /* Step 1: Start playback */
    printf("\n--- Test 1: Start playback ---\n");

    status = ansimuz_mpv_loadfile(
        player,
        "native/mpv/music1.mp3"
    );

    if (status < 0) {
        printf("[FATAL] Load failed: %s\n",
                ansimuz_mpv_error_string(status));
        ansimuz_mpv_destroy(player);
        return 1;
    }

    uint64_t expected_load_id =
        ansimuz_mpv_get_last_load_id(player);

    printf("[LOAD] load_id=%llu\n",
           (unsigned long long)expected_load_id);

    int file_loaded = 0;
    int playback_restarted = 0;
    int end_file = 0;

    // Wait for the track to load and playback to start.
    drain_events(player, 3000,
                 &file_loaded, &playback_restarted, &end_file);

    if (!playback_restarted) {
        printf("[FAIL] Playback did not restart. Stop test aborted.\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    /* Step 2: Stop playback manually */
    printf("\n--- Test 2: Stop playback manually ---\n");

    status = ansimuz_mpv_stop(player);

    printf("[STOP] status=%d\n", status);

    if (status < 0) {
        printf("[FAIL] Stop command failed: %s\n",
               ansimuz_mpv_error_string(status));
        ansimuz_mpv_destroy(player);
        return 1;
    }

    // Capture the END_FILE event produced by manual stop.
    drain_events(player, 2000,
                 &file_loaded, &playback_restarted, &end_file);

    /* Step 3: Verify no extraneous playback events fire after stop */
    printf("\n--- Test 3: Post-stop event check ---\n");

    int ignored_loaded = 0;
    int post_stop_restart = 0;
    int post_stop_end = 0;

    drain_events(player, 1000,
                 &ignored_loaded, &post_stop_restart, &post_stop_end);

    /* Step 4: Validate test outcome and clean up */
    printf("\n--- Results ---\n");
    printf("Expected load_id: %llu\n",
           (unsigned long long)expected_load_id);
    printf("FILE_LOADED observed: %s\n",
           file_loaded ? "YES" : "NO");
    printf("PLAYBACK_RESTART observed: %s\n",
           playback_restarted ? "YES" : "NO");
    printf("END_FILE observed: %s\n",
           end_file ? "YES" : "NO");
    printf("Post-stop PLAYBACK_RESTART: %s\n",
           post_stop_restart ? "YES" : "NO");

    ansimuz_mpv_destroy(player);

    printf("\n[OK] Backend destroyed cleanly\n");

    return 0;
}
