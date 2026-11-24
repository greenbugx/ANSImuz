/*
 * Integration test verifying volume control and mute functionality.
 * Tests volume levels (0, 50, 100 percent) and mute state toggling.
 * Verifies that getters accurately reflect property modifications and
 * confirms that asynchronous PROPERTY_CHANGE notifications are delivered.
 */

#include <stdio.h>
#include <stdint.h>
#include <math.h>
#include "test_common.h"
#include "ansimuz_mpv.h"

#define VALUE_TOLERANCE 0.5

/*
 * Sets volume and verifies that the getter reflects the requested level.
 */
static int check_volume(
    ansimuz_mpv *player,
    double expected,
    const char *label
) {
    printf("\n--- %s ---\n", label);

    int status = ansimuz_mpv_set_volume(player, expected);

    if (status < 0) {
        printf("[FAIL] set_volume: %s\n",
               ansimuz_mpv_error_string(status));
        return 0;
    }

    double actual = -1.0;

    status = ansimuz_mpv_get_volume(player, &actual);

    if (status < 0) {
        printf("[FAIL] get_volume: %s\n",
               ansimuz_mpv_error_string(status));
        return 0;
    }

    printf("[VOLUME] Expected: %.2f\n", expected);
    printf("[VOLUME] Actual: %.2f\n", actual);

    int passed = fabs(actual - expected) <= VALUE_TOLERANCE;

    printf("[%s] %s\n", passed ? "PASS" : "FAIL", label);

    return passed;
}

/*
 * Sets mute state and verifies that the getter reflects the requested flag.
 */
static int check_mute(
    ansimuz_mpv *player,
    int expected,
    const char *label
) {
    printf("\n--- %s ---\n", label);

    int status = ansimuz_mpv_set_mute(player, expected);

    if (status < 0) {
        printf("[FAIL] set_mute: %s\n",
               ansimuz_mpv_error_string(status));
        return 0;
    }

    int actual = -1;

    status = ansimuz_mpv_get_mute(player, &actual);

    if (status < 0) {
        printf("[FAIL] get_mute: %s\n",
               ansimuz_mpv_error_string(status));
        return 0;
    }

    printf("[MUTE] Expected: %s\n", expected ? "ON" : "OFF");
    printf("[MUTE] Actual: %s\n", actual ? "ON" : "OFF");

    int passed = actual == (expected ? 1 : 0);

    printf("[%s] %s\n", passed ? "PASS" : "FAIL", label);

    return passed;
}

/*
 * Polls events until a specific PROPERTY_CHANGE notification is received.
 */
static int wait_for_property_event(
    ansimuz_mpv *player,
    uint64_t expected_id,
    ansimuz_mpv_property expected_property,
    const char *label
) {
    for (int i = 0; i < 30; i++) {
        ansimuz_mpv_event event;

        while (ansimuz_mpv_poll_event(player, &event) > 0) {
            printf("[EVENT] type=%d load_id=%llu property=%d value=%.2f\n",
                   event.type,
                   (unsigned long long)event.load_id,
                   event.property,
                   event.value);

            if (event.load_id == expected_id &&
                event.type == ANSIMUZ_MPV_EVENT_PROPERTY_CHANGE &&
                event.property == expected_property) {
                printf("[EVENT PASS] %s\n", label);
                return 1;
            }

            if (event.load_id == expected_id &&
                event.type == ANSIMUZ_MPV_EVENT_END_FILE) {
                printf("[FAIL] Track ended unexpectedly\n");
                return 0;
            }
        }

        sleep_ms(100);
    }

    printf("[EVENT FAIL] %s: property event not received\n", label);
    return 0;
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

    /* Step 1: Load test track */
    printf("\n--- Test 1: Load music1.mp3 ---\n");
    ensure_test_audio_file("native/mpv/music1.mp3", 10);

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

    /* Wait for playback to begin */
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

    printf("[OK] Playback started\n");

    /* Step 2: Validate volume modifications and property event emission */
    int volume_50 = check_volume(player, 50.0, "Volume 50 percent");
    int event_50 = wait_for_property_event(
        player, expected_id,
        ANSIMUZ_MPV_PROPERTY_VOLUME,
        "Volume 50 property event"
    );

    int volume_100 = check_volume(player, 100.0, "Volume 100 percent");
    int event_100 = wait_for_property_event(
        player, expected_id,
        ANSIMUZ_MPV_PROPERTY_VOLUME,
        "Volume 100 property event"
    );

    int volume_0 = check_volume(player, 0.0, "Volume zero");
    int event_0 = wait_for_property_event(
        player, expected_id,
        ANSIMUZ_MPV_PROPERTY_VOLUME,
        "Volume zero property event"
    );

    /* Step 3: Validate mute toggling and property event emission */
    int mute_on = check_mute(player, 1, "Enable mute");
    int mute_on_event = wait_for_property_event(
        player, expected_id,
        ANSIMUZ_MPV_PROPERTY_MUTE,
        "Mute enabled property event"
    );

    int mute_off = check_mute(player, 0, "Disable mute");
    int mute_off_event = wait_for_property_event(
        player, expected_id,
        ANSIMUZ_MPV_PROPERTY_MUTE,
        "Mute disabled property event"
    );

    /* Restore normal volume before destroying the player. */
    ansimuz_mpv_set_mute(player, 0);
    ansimuz_mpv_set_volume(player, 100.0);

    /* Step 4: Report results and clean up */
    printf("\n--- Results ---\n");
    printf("Volume 50: %s\n", volume_50 ? "PASS" : "FAIL");
    printf("Volume 50 event: %s\n", event_50 ? "PASS" : "FAIL");
    printf("Volume 100: %s\n", volume_100 ? "PASS" : "FAIL");
    printf("Volume 100 event: %s\n", event_100 ? "PASS" : "FAIL");
    printf("Volume 0: %s\n", volume_0 ? "PASS" : "FAIL");
    printf("Volume 0 event: %s\n", event_0 ? "PASS" : "FAIL");
    printf("Mute ON: %s\n", mute_on ? "PASS" : "FAIL");
    printf("Mute ON event: %s\n", mute_on_event ? "PASS" : "FAIL");
    printf("Mute OFF: %s\n", mute_off ? "PASS" : "FAIL");
    printf("Mute OFF event: %s\n", mute_off_event ? "PASS" : "FAIL");

    ansimuz_mpv_destroy(player);

    if (volume_50 && event_50 &&
        volume_100 && event_100 &&
        volume_0 && event_0 &&
        mute_on && mute_on_event &&
        mute_off && mute_off_event) {
        printf("\n[PASS] Volume and mute test passed!\n");
        return 0;
    }

    printf("\n[FAIL] Volume and mute test failed.\n");
    return 1;
}
