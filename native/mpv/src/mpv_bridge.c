#include "ansimuz_mpv.h"
#include <mpv/client.h>
#include <stdio.h>
#include <stdlib.h>

/*
 * Internal handle holder. Keeping mpv_handle internal insulates callers
 * from libmpv header drift and keeps the C ABI boundary clean.
 */
struct ansimuz_mpv {
    mpv_handle *handle;
};

/*
 * Writes a formatted error message into caller provided storage.
 * Leaves the buffer unchanged if buffer is NULL or capacity is zero.
 */
static void set_error(
    char *buffer,
    size_t buffer_size,
    const char *operation,
    int status
) {
    if (!buffer || buffer_size == 0) {
        return;
    }

    snprintf(
        buffer,
        buffer_size,
        "%s: %s",
        operation,
        mpv_error_string(status)
    );
}

/*
 * Sets a string option on the uninitialized mpv instance and records failures.
 * Returns 0 on success or negative mpv error code on failure.
 */
static int configure_option(
    mpv_handle *handle,
    const char *name,
    const char *value,
    char *error_buffer,
    size_t error_buffer_size
) {
    int status = mpv_set_option_string(handle, name, value);

    if (status < 0) {
        set_error(
            error_buffer,
            error_buffer_size,
            name,
            status
        );
    }

    return status;
}

int ansimuz_mpv_create(
    ansimuz_mpv **out_player,
    char *error_buffer,
    size_t error_buffer_size
) {
    if (error_buffer && error_buffer_size > 0) {
        error_buffer[0] = '\0';
    }

    if (!out_player) {
        return -1;
    }

    *out_player = NULL;

    ansimuz_mpv *player = calloc(1, sizeof(*player));

    if (!player) {
        return -1;
    }

    player->handle = mpv_create();

    if (!player->handle) {
        if (error_buffer && error_buffer_size > 0) {
            snprintf(
                error_buffer,
                error_buffer_size,
                "mpv_create failed"
            );
        }

        free(player);
        return -1;
    }

    int status;

    /*
     * Disable loading user configuration files and custom lua scripts.
     * Prevents external config overrides or unexpected keybindings from
     * interfering with internal playback control.
     */
    status = configure_option(
        player->handle, "config", "no",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    /*
     * Disable terminal control and console output hooks.
     * Essential because mpv terminal output will corrupt Ratatui drawing.
     */
    status = configure_option(
        player->handle, "terminal", "no",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    /*
     * Force audio only mode and bypass video decoders and window creation.
     * Cuts down memory footprint and startup latency.
     */
    status = configure_option(
        player->handle, "video", "no",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    /*
     * Commit options and spawn internal playback threads.
     * Once initialized most core options become read only or require property writes.
     */
    status = mpv_initialize(player->handle);

    if (status < 0) {
        set_error(
            error_buffer,
            error_buffer_size,
            "mpv_initialize",
            status
        );

        goto fail;
    }

    *out_player = player;
    return 0;

fail:
    mpv_destroy(player->handle);
    free(player);
    return status;
}

void ansimuz_mpv_destroy(ansimuz_mpv *player) {
    if (!player) {
        return;
    }

    /*
     * mpv_terminate_destroy signals the core to stop playback and joins all
     * worker threads before freeing instance resources.
     */
    if (player->handle) {
        mpv_terminate_destroy(player->handle);
    }

    free(player);
}
