/*
 * Integration test validating playback pause and resume functionality.
 * Ensures that pausing holds the current position within strict tolerance,
 * resuming advances playback as expected, and no spurious END_FILE events
 * are produced during the pause lifecycle.
 */

#include <stdio.h>
#include <stdint.h>
#include <math.h>
#include "test_common.h"
#include "ansimuz_mpv.h"

/*
 * Helper to safely poll current playback position in seconds.
 */
static int get_position(ansimuz_mpv *player, double *position)
{
    return ansimuz_mpv_get_time_pos(player, position) == 0;
}

int main(void)
{
    ansimuz_mpv *player = NULL;
    char error[256] = {0};

    /* Initialize player instance */
    if (ansimuz_mpv_create(&player, error, sizeof(error)) != 0) {
        fprintf(stderr, "[FAIL] Init: %s\n", error);
        return 1;
    }

    printf("[OK] Backend initialized\n");

    /* Step 1: Load test track */
    printf("\n--- Test 1: Load music1.mp3 ---\n");
    ensure_test_audio_file("native/mpv/music1.mp3", 10);

    if (ansimuz_mpv_loadfile(player, "native/mpv/music1.mp3") != 0) {
        fprintf(stderr, "[FAIL] loadfile failed\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    uint64_t expected_id = ansimuz_mpv_get_last_load_id(player);

    printf("[LOAD] load_id=%llu\n",
           (unsigned long long)expected_id);

    int file_loaded = 0;
    int playback_restarted = 0;
    int end_file = 0;

    /* Step 2: Wait for playback to begin */
    printf("\n--- Test 2: Wait for playback ---\n");

    for (int i = 0; i < 100; i++) {
        ansimuz_mpv_event event;

        while (ansimuz_mpv_poll_event(player, &event) > 0) {
            printf("[EVENT] type=%d load_id=%llu\n",
                   event.type,
                   (unsigned long long)event.load_id);

            if (event.load_id != expected_id)
                continue;

            if (event.type == ANSIMUZ_MPV_EVENT_FILE_LOADED)
                file_loaded = 1;

            if (event.type == ANSIMUZ_MPV_EVENT_PLAYBACK_RESTART)
                playback_restarted = 1;

            if (event.type == ANSIMUZ_MPV_EVENT_END_FILE)
                end_file = 1;
        }

        if (file_loaded && playback_restarted)
            break;

        sleep_ms(100);
    }

    if (!file_loaded || !playback_restarted) {
        fprintf(stderr, "[FAIL] Playback did not start\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    /* Allow stream to establish steady playback before pausing */
    sleep_ms(1000);

    double before_pause = 0.0;
    double during_pause = 0.0;
    double after_resume = 0.0;

    if (!get_position(player, &before_pause)) {
        fprintf(stderr, "[FAIL] Could not read position\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    /* Step 3: Pause playback and verify position freeze */
    printf("\n--- Test 3: Pause playback ---\n");
    printf("[POSITION] Before pause: %.3f\n", before_pause);

    if (ansimuz_mpv_set_pause(player, 1) != 0) {
        fprintf(stderr, "[FAIL] Pause command failed\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    sleep_ms(1500);

    if (!get_position(player, &during_pause)) {
        fprintf(stderr, "[FAIL] Could not read paused position\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    printf("[POSITION] During pause: %.3f\n", during_pause);

    /* Verify position remained static within a small tolerance */
    double pause_difference = fabs(during_pause - before_pause);
    int pause_passed = pause_difference < 0.25;

    printf("[PAUSE] Position difference: %.3f seconds\n",
           pause_difference);
    printf("[PAUSE] %s\n", pause_passed ? "PASS" : "FAIL");

    /* Step 4: Resume playback and verify position advances */
    printf("\n--- Test 4: Resume playback ---\n");

    if (ansimuz_mpv_set_pause(player, 0) != 0) {
        fprintf(stderr, "[FAIL] Resume command failed\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    sleep_ms(1500);

    if (!get_position(player, &after_resume)) {
        fprintf(stderr, "[FAIL] Could not read resumed position\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    printf("[POSITION] After resume: %.3f\n", after_resume);

    /* Verify playback progressed forward */
    double resume_difference = after_resume - during_pause;
    int resume_passed = resume_difference > 0.5;

    printf("[RESUME] Position advanced: %.3f seconds\n",
           resume_difference);
    printf("[RESUME] %s\n", resume_passed ? "PASS" : "FAIL");

    /* Step 5: Check that pausing did not trigger an unintended END_FILE */
    printf("\n--- Test 5: Check for false END_FILE ---\n");

    for (int i = 0; i < 10; i++) {
        ansimuz_mpv_event event;

        while (ansimuz_mpv_poll_event(player, &event) > 0) {
            printf("[EVENT] type=%d load_id=%llu\n",
                   event.type,
                   (unsigned long long)event.load_id);

            if (event.load_id == expected_id &&
                event.type == ANSIMUZ_MPV_EVENT_END_FILE) {
                end_file = 1;
            }
        }

        sleep_ms(100);
    }

    printf("[END_FILE] Unexpected event: %s\n",
           end_file ? "YES" : "NO");

    /* Step 6: Print test outcome and clean up */
    printf("\n--- Results ---\n");
    printf("FILE_LOADED: %s\n", file_loaded ? "YES" : "NO");
    printf("PLAYBACK_RESTART: %s\n",
           playback_restarted ? "YES" : "NO");
    printf("Pause behavior: %s\n", pause_passed ? "PASS" : "FAIL");
    printf("Resume behavior: %s\n", resume_passed ? "PASS" : "FAIL");
    printf("No false END_FILE: %s\n", !end_file ? "PASS" : "FAIL");

    ansimuz_mpv_destroy(player);

    if (pause_passed && resume_passed && !end_file) {
        printf("\n[PASS] Pause/resume test passed!\n");
        return 0;
    }

    printf("\n[FAIL] Pause/resume test failed.\n");
    return 1;
}
