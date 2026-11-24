/*
 * Integration test verifying playback seek functionality and timestamp accuracy.
 * Tests forward seeking, backward seeking, and seeking to track start (0.0s).
 * Validates that actual position matches target within a tight tolerance and
 * confirms that seeking does not produce spurious END_FILE events.
 */

#include <stdio.h>
#include <stdint.h>
#include <math.h>
#include "test_common.h"
#include "ansimuz_mpv.h"

#define POSITION_TOLERANCE 0.75

/*
 * Polls playback position until it settles near the target within tolerance.
 */
static int wait_for_position(
    ansimuz_mpv *player,
    double target,
    double *actual
)
{
    for (int i = 0; i < 50; i++) {
        if (ansimuz_mpv_get_time_pos(player, actual) == 0 &&
            fabs(*actual - target) <= POSITION_TOLERANCE) {
            return 1;
        }

        sleep_ms(100);
    }

    return 0;
}

/*
 * Dispatches a seek request and verifies that the reported position converges.
 */
static int check_seek(
    ansimuz_mpv *player,
    double target,
    const char *label
)
{
    printf("\n--- %s ---\n", label);
    printf("[SEEK] Target: %.2f seconds\n", target);

    if (ansimuz_mpv_seek(player, target) != 0) {
        printf("[FAIL] Seek command failed\n");
        return 0;
    }

    double actual = 0.0;

    if (!wait_for_position(player, target, &actual)) {
        printf("[FAIL] Position did not reach target\n");
        printf("[POSITION] Last observed: %.3f seconds\n", actual);
        return 0;
    }

    printf("[POSITION] Actual: %.3f seconds\n", actual);
    printf("[DIFF] %.3f seconds\n", fabs(actual - target));
    printf("[PASS] %s\n", label);

    return 1;
}

int main(void)
{
    ansimuz_mpv *player = NULL;
    char error[256] = {0};

    /* Initialize player backend */
    if (ansimuz_mpv_create(&player, error, sizeof(error)) != 0) {
        fprintf(stderr, "[FAIL] Init: %s\n", error);
        return 1;
    }

    printf("[OK] Backend initialized\n");

    /* Step 1: Load test audio file */
    printf("\n--- Test 1: Load music1.mp3 ---\n");
    ensure_test_audio_file("native/mpv/music1.mp3", 30);

    if (ansimuz_mpv_loadfile(
            player, "native/mpv/music1.mp3") != 0) {
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
    double duration = 0.0;

    /* Step 2: Wait for stream demuxing and valid track duration */
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

        if (file_loaded && playback_restarted &&
            ansimuz_mpv_get_duration(player, &duration) == 0 &&
            duration > 10.0) {
            break;
        }

        sleep_ms(100);
    }

    if (!file_loaded || !playback_restarted || duration <= 10.0) {
        fprintf(stderr,
                "[FAIL] Playback or duration unavailable "
                "(loaded=%d restarted=%d duration=%.2f)\n",
                file_loaded, playback_restarted, duration);
        ansimuz_mpv_destroy(player);
        return 1;
    }

    printf("[OK] Duration: %.2f seconds\n", duration);

    /* Step 3: Test forward seeking into the track */
    int forward_passed = check_seek(
        player, 20.0, "Test 3: Forward seek"
    );

    /* Step 4: Test backward seeking to earlier timestamp */
    int backward_passed = check_seek(
        player, 5.0, "Test 4: Backward seek"
    );

    /* Step 5: Test seeking back to track origin */
    int zero_passed = check_seek(
        player, 0.0, "Test 5: Seek to beginning"
    );

    /* Step 6: Ensure no false END_FILE events were emitted during seek operations */
    printf("\n--- Test 6: Check for false END_FILE ---\n");

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

    /* Step 7: Output test results and clean up */
    printf("\n--- Results ---\n");
    printf("FILE_LOADED: %s\n", file_loaded ? "YES" : "NO");
    printf("PLAYBACK_RESTART: %s\n",
           playback_restarted ? "YES" : "NO");
    printf("Forward seek: %s\n", forward_passed ? "PASS" : "FAIL");
    printf("Backward seek: %s\n", backward_passed ? "PASS" : "FAIL");
    printf("Seek to zero: %s\n", zero_passed ? "PASS" : "FAIL");
    printf("No false END_FILE: %s\n", !end_file ? "PASS" : "FAIL");

    ansimuz_mpv_destroy(player);

    if (forward_passed && backward_passed &&
        zero_passed && !end_file) {
        printf("\n[PASS] Seek test passed!\n");
        return 0;
    }

    printf("\n[FAIL] Seek test failed.\n");
    return 1;
}
