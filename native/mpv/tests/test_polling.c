/*
 * Integration test exercising non blocking event polling and queue mechanics.
 * Validates that an empty queue returns immediately with 0, event ordering
 * proceeds through FILE_LOADED and PLAYBACK_RESTART, and polling an idle
 * core continues to return 0 without blocking.
 */

#include <stdio.h>
#include <stdint.h>
#include <windows.h>

#include "ansimuz_mpv.h"

#define MAX_WAIT_MS 10000
#define POLL_INTERVAL_MS 10

/*
 * Helper that drains pending events, validates expected load ID attribution,
 * and sets flags for required playback lifecycle events.
 */
static int drain_events(ansimuz_mpv *player, uint64_t expected_load_id,
                        int *file_loaded, int *playback_restart,
                        int *event_count)
{
    ansimuz_mpv_event event;
    int status;

    while ((status = ansimuz_mpv_poll_event(player, &event)) > 0) {
        (*event_count)++;

        printf("[EVENT] type=%d load_id=%llu property=%d value=%.2f\n",
               event.type,
               (unsigned long long)event.load_id,
               event.property,
               event.value);

        /* Ensure events are attributed to the active load ID */
        if (event.load_id != 0 && event.load_id != expected_load_id) {
            printf("[FAIL] Unexpected load_id: %llu\n",
                   (unsigned long long)event.load_id);
            return -1;
        }

        if (event.type == ANSIMUZ_MPV_EVENT_FILE_LOADED &&
            event.load_id == expected_load_id) {
            *file_loaded = 1;
        }

        if (event.type == ANSIMUZ_MPV_EVENT_PLAYBACK_RESTART &&
            event.load_id == expected_load_id) {
            *playback_restart = 1;
        }
    }

    if (status < 0) {
        printf("[FAIL] poll_event returned error: %d\n", status);
        return -1;
    }

    return 0;
}

int main(void)
{
    ansimuz_mpv *player = NULL;
    ansimuz_mpv_event event;
    char error[256] = {0};

    int file_loaded = 0;
    int playback_restart = 0;
    int event_count = 0;

    printf("=== Test 9: Event Polling ===\n\n");

    /* Step 1: Initialize player instance */
    if (ansimuz_mpv_create(&player, error, sizeof(error)) != 0) {
        printf("[FAIL] Initialization: %s\n", error);
        return 1;
    }

    printf("[PASS] Player initialized\n");

    /*
     * Drain any initial property events generated during initialization.
     */
    while (ansimuz_mpv_poll_event(player, &event) > 0) {
        printf("[INIT EVENT] type=%d property=%d\n",
               event.type, event.property);
    }

    /*
     * Verify empty queue behavior.
     */
    int empty_status = ansimuz_mpv_poll_event(player, &event);

    if (empty_status != 0) {
        printf("[FAIL] Expected empty queue (0), got %d\n", empty_status);
        ansimuz_mpv_destroy(player);
        return 1;
    }

    printf("[PASS] Empty queue returns 0\n");

    /*
     * Load a track.
     */
    if (ansimuz_mpv_loadfile(player, "native/mpv/music1.mp3") != 0) {
        printf("[FAIL] loadfile command failed\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    uint64_t load_id = ansimuz_mpv_get_last_load_id(player);

    printf("[INFO] Loading track with load_id=%llu\n\n",
           (unsigned long long)load_id);

    /*
     * Collect events until both important events arrive.
     */
    DWORD start = GetTickCount();

    while (GetTickCount() - start < MAX_WAIT_MS) {
        if (drain_events(player, load_id, &file_loaded,
                         &playback_restart, &event_count) != 0) {
            ansimuz_mpv_destroy(player);
            return 1;
        }

        if (file_loaded && playback_restart) {
            break;
        }

        Sleep(POLL_INTERVAL_MS);
    }

    printf("\n=== Results ===\n");

    if (!file_loaded) {
        printf("[FAIL] FILE_LOADED was not received\n");
    } else {
        printf("[PASS] FILE_LOADED received\n");
    }

    if (!playback_restart) {
        printf("[FAIL] PLAYBACK_RESTART was not received\n");
    } else {
        printf("[PASS] PLAYBACK_RESTART received\n");
    }

    if (!file_loaded || !playback_restart) {
        ansimuz_mpv_destroy(player);
        return 1;
    }

    /*
     * Let any remaining events arrive, then drain them.
     */
    Sleep(500);

    if (drain_events(player, load_id, &file_loaded,
                     &playback_restart, &event_count) != 0) {
        ansimuz_mpv_destroy(player);
        return 1;
    }

    /*
     * Repeatedly poll an idle queue.
     */
    int idle_ok = 1;

    for (int i = 0; i < 20; i++) {
        int status = ansimuz_mpv_poll_event(player, &event);

        if (status != 0) {
            printf("[FAIL] Unexpected event/status during idle poll: %d\n",
                   status);
            idle_ok = 0;
            break;
        }

        Sleep(10);
    }

    if (idle_ok) {
        printf("[PASS] Repeated idle polling returns 0\n");
    }

    printf("[INFO] Total events received: %d\n", event_count);

    /* Clean up playback before terminating instance */
    ansimuz_mpv_stop(player);
    Sleep(200);

    while (ansimuz_mpv_poll_event(player, &event) > 0) {
        printf("[CLEANUP EVENT] type=%d load_id=%llu\n",
               event.type,
               (unsigned long long)event.load_id);
    }

    ansimuz_mpv_destroy(player);

    printf("[PASS] Player destroyed cleanly\n");

    if (!idle_ok) {
        return 1;
    }

    printf("\n[PASS] Test 9 completed\n");
    return 0;
}
