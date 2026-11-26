#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "test_common.h"
#include "ansimuz_mpv.h"

int main(void) {
    ansimuz_mpv *player = NULL;
    char error[512] = {0};

    int status = ansimuz_mpv_create(&player, error, sizeof(error));
    if (status < 0) {
        printf("[FATAL] Initialization failed: %s\n", error);
        return 1;
    }

    ensure_test_audio_file("native/mpv/music1.mp3", 4);

    char file_url[512] = {0};
#ifdef _WIN32
    char abs_path[400] = {0};
    if (!_fullpath(abs_path, "native\\mpv\\music1.mp3", sizeof(abs_path))) {
        printf("[FAIL] Could not resolve absolute test audio path\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }
    for (size_t i = 0; abs_path[i] != '\0'; i++) {
        if (abs_path[i] == '\\') abs_path[i] = '/';
    }
    snprintf(file_url, sizeof(file_url), "file:///%s", abs_path);
#else
    char *resolved = realpath("native/mpv/music1.mp3", NULL);
    if (!resolved) {
        printf("[FAIL] Could not resolve realpath\n");
        ansimuz_mpv_destroy(player);
        return 1;
    }
    snprintf(file_url, sizeof(file_url), "file://%s", resolved);
    free(resolved);
#endif

    status = ansimuz_mpv_loadfile_direct(
        player,
        file_url,
        "ANSImuz-Direct-Test/1.0",
        NULL
    );
    if (status < 0) {
        printf("[FAIL] ansimuz_mpv_loadfile_direct failed: %s\n", ansimuz_mpv_error_string(status));
        ansimuz_mpv_destroy(player);
        return 1;
    }

    char ytdl_val[16] = {0};
    status = ansimuz_mpv_get_ytdl(player, ytdl_val, sizeof(ytdl_val));
    if (status < 0 || strcmp(ytdl_val, "no") != 0) {
        printf("[FAIL] Expected ytdl=no, got status=%d val='%s'\n", status, ytdl_val);
        ansimuz_mpv_destroy(player);
        return 1;
    }

    int loaded = 0;
    for (int elapsed = 0; elapsed < 3000; elapsed += 50) {
        ansimuz_mpv_event ev;
        int r = ansimuz_mpv_poll_event(player, &ev);
        if (r == 1 && (ev.type == ANSIMUZ_MPV_EVENT_FILE_LOADED || ev.type == ANSIMUZ_MPV_EVENT_PLAYBACK_RESTART)) {
            loaded = 1;
            break;
        }
        if (r == 1 && ev.type == ANSIMUZ_MPV_EVENT_END_FILE && ev.end_reason == ANSIMUZ_MPV_END_ERROR) {
            printf("[FAIL] END_FILE error=%d (%s)\n", ev.error_code, ansimuz_mpv_error_string(ev.error_code));
            ansimuz_mpv_destroy(player);
            return 1;
        }
        wait_ms(50);
    }

    ansimuz_mpv_destroy(player);
    if (!loaded) {
        printf("[FAIL] Timed out waiting for direct playback load\n");
        return 1;
    }

    printf("[OK] test_direct passed (ytdl=no verified)\n");
    return 0;
}

