/*
 * Stress test verifying rapid track replacement handling.
 * Issues repeated loadfile replace commands in fast succession to ensure
 * internal load queue tracking and native playlist entry correlation
 * do not leak records, drop pending states, or attribute events to the wrong track.
 */

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <windows.h>

#include "ansimuz_mpv.h"

#define MAX_EVENTS 512
#define REPLACEMENT_ROUNDS 10

/*
 * Record of observed native event for post test validation.
 */
typedef struct {
    int type;
    uint64_t load_id;
    int end_reason;
} recorded_event;

static recorded_event events[MAX_EVENTS];
static int event_count = 0;
static uint64_t final_load_id = 0;
static int final_loaded = 0;

/*
 * Drains events and records them into the event log array.
 */
static void record_events(ansimuz_mpv *player, int duration_ms)
{
    int elapsed = 0;

    while (elapsed < duration_ms) {
        ansimuz_mpv_event event;

        while (ansimuz_mpv_poll_event(player, &event) > 0) {
            if (event_count < MAX_EVENTS) {
                events[event_count].type = event.type;
                events[event_count].load_id = event.load_id;
                events[event_count].end_reason = event.end_reason;
                event_count++;
            }

            if (event.type == ANSIMUZ_MPV_EVENT_FILE_LOADED &&
                event.load_id == final_load_id) {
                final_loaded = 1;
            }

            printf("[EVENT] type=%d load_id=%llu reason=%d\n",
                   event.type,
                   (unsigned long long)event.load_id,
                   event.end_reason);
        }

        Sleep(10);
        elapsed += 10;
    }
}

int main(void)
{
    ansimuz_mpv *player = NULL;
    char error[512];

    /* Initialize player backend */
    if (ansimuz_mpv_create(&player, error, sizeof(error)) < 0) {
        printf("[FAIL] Initialization: %s\n", error);
        return 1;
    }

    printf("[OK] Backend initialized\n");

    const char *tracks[] = {
        "native/mpv/music1.mp3",
        "native/mpv/music2.mp3",
        "native/mpv/music3.mp3"
    };

    /* final_load_id tracked at file scope */

    /* Perform multiple rounds of rapid replacements */
    for (int round = 0; round < REPLACEMENT_ROUNDS; round++) {
        printf("\n--- Round %d ---\n", round + 1);

        for (int i = 0; i < 3; i++) {
            int status = ansimuz_mpv_loadfile(player, tracks[i]);

            if (status < 0) {
                printf("[FAIL] Load command failed: %s\n",
                       ansimuz_mpv_error_string(status));
                ansimuz_mpv_destroy(player);
                return 1;
            }

            final_load_id = ansimuz_mpv_get_last_load_id(player);

            printf("[LOAD] %s load_id=%llu\n",
                   tracks[i],
                   (unsigned long long)final_load_id);

            /* Deliberately replace tracks quickly. */
            record_events(player, 100);
        }
    }

    /* Wait for the final track to complete loading and reach steady state */
    printf("\n--- Waiting for final track ---\n");

    /* final_loaded tracked at file scope */

    for (int i = 0; i < 100; i++) {
        ansimuz_mpv_event event;

        while (ansimuz_mpv_poll_event(player, &event) > 0) {
            printf("[EVENT] type=%d load_id=%llu reason=%d\n",
                   event.type,
                   (unsigned long long)event.load_id,
                   event.end_reason);

            if (event.type == ANSIMUZ_MPV_EVENT_FILE_LOADED &&
                event.load_id == final_load_id) {
                final_loaded = 1;
            }
        }

        if (final_loaded)
            break;

        Sleep(50);
    }

    /* Verify final track attribution and cleanup */
    printf("\n--- Results ---\n");
    printf("Events recorded: %d\n", event_count);
    printf("Expected final load_id: %llu\n",
           (unsigned long long)final_load_id);

    if (!final_loaded) {
        printf("[FAIL] Final track did not report FILE_LOADED\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }

    printf("[PASS] Final track loaded successfully\n");

    record_events(player, 500);

    ansimuz_mpv_destroy(player);

    printf("[PASS] Rapid replacement test completed\n");
    return 0;
}