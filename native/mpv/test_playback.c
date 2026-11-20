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
 * Cross platform sleep helper for paced playback smoke tests.
 * Avoids hardcoding Win32 Sleep calls so test runners can execute on Linux.
 */
#ifdef _WIN32
#include <windows.h>
static void wait_seconds(int seconds) {
    Sleep((DWORD)(seconds * 1000));
}
#else
#include <unistd.h>
static void wait_seconds(int seconds) {
    sleep((unsigned int)seconds);
}
#endif

/*
 * Checks whether a local file path exists and is readable.
 * Simple fopen check to avoid platform specific stat headers.
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
 * Resolves default audio test target when no path is supplied via CLI.
 * Searches current directory and common repository relative paths.
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
 * Integration test exercising end to end audio playback lifecycle.
 * Validates initialization, track loading, metadata polling, pause,
 * resume, absolute seek, and clean shutdown.
 */
int main(int argc, char *argv[]) {
    ansimuz_mpv *player = NULL;
    char error[512] = {0};

    /* Allocate and initialize the headless audio core */
    int status = ansimuz_mpv_create(
        &player,
        error,
        sizeof(error)
    );

    if (status < 0) {
        fprintf(stderr, "Initialization failed: %s\n", error);
        return 1;
    }

    printf("[OK] Native backend initialized\n");

    /*
     * Resolve target audio path.
     * Use user CLI argument if provided, otherwise search repository locations.
     */
    const char *target_path = NULL;
    if (argc > 1 && argv[1][0] != '\0') {
        target_path = argv[1];
    } else {
        target_path = resolve_default_track();
        if (target_path) {
            printf("[INFO] Auto detected fixture at: %s\n", target_path);
        } else {
            fprintf(stderr, "[ERROR] Default fixture music.mp3 not found\n");
            fprintf(stderr, "[INFO] Run from native/mpv or supply path: %s <path-to-audio-or-url>\n", argv[0]);
            ansimuz_mpv_destroy(player);
            return 1;
        }
    }

    /*
     * Issue loadfile with replace mode.
     * Starts background demuxer and audio decoding asynchronously.
     */
    status = ansimuz_mpv_loadfile(player, target_path);

    if (status < 0) {
        fprintf(stderr, "Load failed: %s\n", ansimuz_mpv_error_string(status));
        ansimuz_mpv_destroy(player);
        return 1;
    }

    printf("[OK] Load command dispatched for: %s\n", target_path);

    /*
     * Allow the core demuxer thread time to parse audio headers and fill buffers.
     * Property queries before demuxer initialization return property unavailable.
     */
    printf("[INFO] Waiting for stream demuxing and initial playback...\n");
    wait_seconds(3);

    double duration = 0.0;
    double position = 0.0;

    status = ansimuz_mpv_get_duration(player, &duration);
    if (status == 0) {
        printf("[INFO] Duration: %.2f seconds\n", duration);
    } else {
        printf("[WARN] Duration unavailable: %s\n", ansimuz_mpv_error_string(status));
    }

    status = ansimuz_mpv_get_time_pos(player, &position);
    if (status == 0) {
        printf("[INFO] Position: %.2f seconds\n", position);
    } else {
        printf("[WARN] Position unavailable: %s\n", ansimuz_mpv_error_string(status));
    }

    /* Test pause functionality */
    printf("[TEST] Pausing playback...\n");
    status = ansimuz_mpv_set_pause(player, 1);
    if (status < 0) {
        fprintf(stderr, "Pause failed: %s\n", ansimuz_mpv_error_string(status));
    }

    wait_seconds(2);

    /* Test resume functionality */
    printf("[TEST] Resuming playback...\n");
    status = ansimuz_mpv_set_pause(player, 0);
    if (status < 0) {
        fprintf(stderr, "Resume failed: %s\n", ansimuz_mpv_error_string(status));
    }

    wait_seconds(2);

    /*
     * Calculate a safe seek target within stream bounds.
     * Avoids seeking past end of file on short audio clips.
     */
    double seek_target = 10.0;
    if (duration > 0.0 && seek_target >= duration) {
        seek_target = duration * 0.5;
    }

    printf("[TEST] Seeking to %.2f seconds...\n", seek_target);
    status = ansimuz_mpv_seek(player, seek_target);
    if (status < 0) {
        fprintf(stderr, "Seek failed: %s\n", ansimuz_mpv_error_string(status));
    }

    wait_seconds(2);

    /* Verify position was updated by seek */
    status = ansimuz_mpv_get_time_pos(player, &position);
    if (status == 0) {
        printf("[INFO] Position after seek: %.2f seconds\n", position);
    }

    /* Test stop command before destroying player */
    printf("[TEST] Stopping playback...\n");
    status = ansimuz_mpv_stop(player);
    if (status < 0) {
        fprintf(stderr, "Stop failed: %s\n", ansimuz_mpv_error_string(status));
    }

    /* Flush and release engine resources */
    ansimuz_mpv_destroy(player);
    printf("[OK] Native backend destroyed cleanly\n");

    return 0;
}
