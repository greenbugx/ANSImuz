/*
 * Integration test exercising player teardown and resource lifecycle management.
 * Validates repeated creation and destruction of idle player instances,
 * destruction while active audio decoding and rendering are in progress,
 * and clean reinitialization following teardown cycles.
 */

#include <stdio.h>
#include <windows.h>

#include "ansimuz_mpv.h"

#define CYCLES 5
#define MAX_WAIT_MS 10000

/*
 * Helper that polls events until a FILE_LOADED notification arrives.
 */
static int wait_for_file_loaded(ansimuz_mpv *player)
{
    DWORD start = GetTickCount();
    ansimuz_mpv_event event;

    while (GetTickCount() - start < MAX_WAIT_MS) {
        int status = ansimuz_mpv_poll_event(player, &event);

        if (status < 0) {
            printf("[FAIL] Event polling error: %d\n", status);
            return 0;
        }

        if (status > 0 &&
            event.type == ANSIMUZ_MPV_EVENT_FILE_LOADED) {
            printf("[INFO] FILE_LOADED load_id=%llu\n",
                   (unsigned long long)event.load_id);
            return 1;
        }

        Sleep(10);
    }

    return 0;
}

int main(void)
{
    char error[256];

    printf("=== Test 10: Shutdown and Resource Cleanup ===\n\n");

    /*
     * Test A: Repeated idle initialization and destruction.
     * Ensures clean creation and immediate teardown without resource leaks.
     */
    printf("--- Test A: Idle create/destroy cycles ---\n");

    for (int i = 0; i < CYCLES; i++) {
        ansimuz_mpv *player = NULL;
        error[0] = '\0';

        if (ansimuz_mpv_create(&player, error, sizeof(error)) != 0) {
            printf("[FAIL] Cycle %d initialization: %s\n", i + 1, error);
            return 1;
        }

        printf("[PASS] Cycle %d initialized\n", i + 1);

        ansimuz_mpv_destroy(player);

        printf("[PASS] Cycle %d destroyed\n", i + 1);
        Sleep(100);
    }

    /*
     * Test B: Destroy while playback is active.
     * Ensures background worker threads and audio streams terminate cleanly.
     */
    printf("\n--- Test B: Destroy during playback ---\n");

    for (int i = 0; i < CYCLES; i++) {
        ansimuz_mpv *player = NULL;
        error[0] = '\0';

        if (ansimuz_mpv_create(&player, error, sizeof(error)) != 0) {
            printf("[FAIL] Playback cycle %d initialization: %s\n",
                   i + 1, error);
            return 1;
        }

        printf("[INFO] Playback cycle %d initialized\n", i + 1);

        if (ansimuz_mpv_loadfile(player, "native/mpv/music1.mp3") != 0) {
            printf("[FAIL] Playback cycle %d load command failed\n", i + 1);
            ansimuz_mpv_destroy(player);
            return 1;
        }

        if (!wait_for_file_loaded(player)) {
            printf("[FAIL] Playback cycle %d did not load\n", i + 1);
            ansimuz_mpv_destroy(player);
            return 1;
        }

        Sleep(500);

        printf("[INFO] Destroying player during playback...\n");
        ansimuz_mpv_destroy(player);

        printf("[PASS] Playback cycle %d destroyed cleanly\n", i + 1);
        Sleep(250);
    }

    /*
     * Test C: Verify a fresh player works after repeated destruction.
     * Confirms no residual global state blocks new playback sessions.
     */
    printf("\n--- Test C: Reinitialize after shutdown ---\n");

    ansimuz_mpv *final_player = NULL;
    error[0] = '\0';

    if (ansimuz_mpv_create(&final_player, error, sizeof(error)) != 0) {
        printf("[FAIL] Final initialization: %s\n", error);
        return 1;
    }

    printf("[PASS] Fresh player initialized\n");

    if (ansimuz_mpv_loadfile(final_player, "native/mpv/music2.mp3") != 0) {
        printf("[FAIL] Final playback load command failed\n");
        ansimuz_mpv_destroy(final_player);
        return 1;
    }

    if (!wait_for_file_loaded(final_player)) {
        printf("[FAIL] Final player could not load music2.mp3\n");
        ansimuz_mpv_destroy(final_player);
        return 1;
    }

    printf("[PASS] Fresh player successfully loaded another track\n");

    ansimuz_mpv_destroy(final_player);

    printf("[PASS] Final player destroyed cleanly\n");

    printf("\n[PASS] Test 10 completed\n");
    return 0;
}
