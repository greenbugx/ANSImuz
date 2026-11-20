#include <stdio.h>
#include <stdlib.h>

#if defined(__has_include)
    #if __has_include("ansimuz_mpv.h")
        #include "ansimuz_mpv.h"
    #elif __has_include("include/ansimuz_mpv.h")
        #include "include/ansimuz_mpv.h"
    #else
        #include "ansimuz_mpv.h"
    #endif
#else
    #include "ansimuz_mpv.h"
#endif

/*
 * Portable millisecond sleep helper for event polling loops.
 */
#ifdef _WIN32
#include <windows.h>
static void wait_ms(unsigned int ms) {
    Sleep((DWORD)ms);
}
#else
#include <time.h>
static void wait_ms(unsigned int ms) {
    struct timespec req = {
        .tv_sec = ms / 1000,
        .tv_nsec = (long)(ms % 1000) * 1000000L
    };

    while (nanosleep(&req, &req) == -1) {
        /* Resume remaining sleep time after interrupt signal */
    }
}
#endif

/*
 * Checks whether a local file path exists and is readable.
 */
static int file_exists(const char *path) {
    if (!path || path[0] == '\0') {
        return 0;
    }
    FILE *file = fopen(path, "rb");
    if (file) {
        fclose(file);
        return 1;
    }
    return 0;
}

/*
 * Resolves target audio fixture across typical repository relative paths.
 */
static const char *resolve_default_track(void) {
    if (file_exists("music.mp3")) {
        return "music.mp3";
    }
    if (file_exists("native/mpv/music.mp3")) {
        return "native/mpv/music.mp3";
    }
    if (file_exists("mpv/music.mp3")) {
        return "mpv/music.mp3";
    }
    return NULL;
}

/*
 * Maps normalized event types to human readable strings for logging.
 */
static const char *event_name(ansimuz_mpv_event_type type) {
    switch (type) {
        case ANSIMUZ_MPV_EVENT_FILE_LOADED:
            return "FILE_LOADED";
        case ANSIMUZ_MPV_EVENT_PLAYBACK_RESTART:
            return "PLAYBACK_RESTART";
        case ANSIMUZ_MPV_EVENT_PROPERTY_CHANGE:
            return "PROPERTY_CHANGE";
        case ANSIMUZ_MPV_EVENT_END_FILE:
            return "END_FILE";
        case ANSIMUZ_MPV_EVENT_ERROR:
            return "ERROR";
        default:
            return "NONE";
    }
}

/*
 * Maps property enum IDs to diagnostic string names.
 */
static const char *property_name(ansimuz_mpv_property prop) {
    switch (prop) {
        case ANSIMUZ_MPV_PROPERTY_TIME_POS:
            return "time-pos";
        case ANSIMUZ_MPV_PROPERTY_DURATION:
            return "duration";
        case ANSIMUZ_MPV_PROPERTY_PAUSE:
            return "pause";
        case ANSIMUZ_MPV_PROPERTY_VOLUME:
            return "volume";
        default:
            return "unknown";
    }
}

/*
 * Maps end reasons to diagnostic strings.
 */
static const char *end_reason_name(ansimuz_mpv_end_reason reason) {
    switch (reason) {
        case ANSIMUZ_MPV_END_EOF:
            return "EOF";
        case ANSIMUZ_MPV_END_STOP:
            return "STOP";
        case ANSIMUZ_MPV_END_QUIT:
            return "QUIT";
        case ANSIMUZ_MPV_END_ERROR:
            return "ERROR";
        case ANSIMUZ_MPV_END_REDIRECT:
            return "REDIRECT";
        default:
            return "UNKNOWN";
    }
}

/*
 * Smoke test verifying native event polling and property change notifications.
 */
int main(int argc, char **argv) {
    ansimuz_mpv *player = NULL;
    char error[512] = {0};

    int status = ansimuz_mpv_create(
        &player, error, sizeof(error)
    );

    if (status < 0) {
        fprintf(stderr, "Initialization failed: %s\n", error);
        return 1;
    }

    printf("[OK] Backend initialized\n");

    const char *target_path = NULL;
    if (argc > 1 && argv[1][0] != '\0') {
        target_path = argv[1];
    } else {
        target_path = resolve_default_track();
        if (target_path) {
            printf("[INFO] Auto detected fixture at: %s\n", target_path);
        } else {
            fprintf(stderr, "[ERROR] Default fixture music.mp3 not found\n");
            fprintf(stderr, "[INFO] Usage: %s <audio-file-or-url>\n", argv[0]);
            ansimuz_mpv_destroy(player);
            return 1;
        }
    }

    status = ansimuz_mpv_loadfile(player, target_path);

    if (status < 0) {
        fprintf(stderr, "Load command failed: %s\n",
                ansimuz_mpv_error_string(status));
        ansimuz_mpv_destroy(player);
        return 1;
    }

    printf("[OK] Load dispatched for: %s\n", target_path);
    printf("[INFO] Listening for native events...\n");

    /*
     * Poll for up to 15 seconds.
     * This test intentionally does not block inside libmpv.
     */
    for (int elapsed = 0; elapsed < 15000; elapsed += 50) {
        ansimuz_mpv_event event;

        int result = ansimuz_mpv_poll_event(player, &event);

        if (result < 0) {
            fprintf(stderr, "Event polling failed: %s\n",
                    ansimuz_mpv_error_string(result));
            break;
        }

        if (result == 1) {
            printf("[EVENT] %s", event_name(event.type));

            if (event.type == ANSIMUZ_MPV_EVENT_PROPERTY_CHANGE) {
                printf(" | Property: %s (%d) | Value: %.3f",
                       property_name(event.property), event.property, event.value);
            }

            if (event.type == ANSIMUZ_MPV_EVENT_END_FILE) {
                printf(" | Reason: %s (%d) | Error: %d",
                       end_reason_name(event.end_reason), event.end_reason, event.error_code);
            }

            printf("\n");

            if (event.type == ANSIMUZ_MPV_EVENT_END_FILE) {
                break;
            }
        }

        wait_ms(50);
    }

    ansimuz_mpv_destroy(player);

    printf("[OK] Backend destroyed cleanly\n");

    return 0;
}

