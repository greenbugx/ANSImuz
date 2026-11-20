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

/*
 * Loads an audio file or stream URL by issuing the loadfile command.
 * Passes replace mode to stop current playback and swap the active track
 * without creating an mpv internal playlist queue.
 */
int ansimuz_mpv_loadfile(ansimuz_mpv *player, const char *path) {
    if (!player || !player->handle || !path || path[0] == '\0') {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    const char *args[] = {
        "loadfile",
        path,
        "replace",
        NULL
    };

    return mpv_command(player->handle, args);
}

/*
 * Toggles playback pause state by writing to the pause property flag.
 * Uses MPV_FORMAT_FLAG with integer 1 or 0 to map to internal boolean flags.
 */
int ansimuz_mpv_set_pause(ansimuz_mpv *player, int paused) {
    if (!player || !player->handle) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    int value = paused ? 1 : 0;

    return mpv_set_property(
        player->handle,
        "pause",
        MPV_FORMAT_FLAG,
        &value
    );
}

/*
 * Issues the synchronous stop command.
 * Clears the active stream and resets audio decoders back to idle state.
 */
int ansimuz_mpv_stop(ansimuz_mpv *player) {
    if (!player || !player->handle) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    const char *args[] = {
        "stop",
        NULL
    };

    return mpv_command(player->handle, args);
}

/*
 * Performs an absolute seek by setting the time-pos property.
 * Guard checks against negative values to fail fast before hitting the core.
 */
int ansimuz_mpv_seek(ansimuz_mpv *player, double seconds) {
    if (!player || !player->handle || seconds < 0.0) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    return mpv_set_property(
        player->handle,
        "time-pos",
        MPV_FORMAT_DOUBLE,
        &seconds
    );
}

/*
 * Reads the time-pos property as a double precision float.
 * Note that this can fail with MPV_ERROR_PROPERTY_UNAVAILABLE during transitions
 * or initial track buffering.
 */
int ansimuz_mpv_get_time_pos(
    ansimuz_mpv *player,
    double *out_seconds
) {
    if (!player || !player->handle || !out_seconds) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    return mpv_get_property(
        player->handle,
        "time-pos",
        MPV_FORMAT_DOUBLE,
        out_seconds
    );
}

/*
 * Reads media duration from the container header metadata.
 * Returns MPV_ERROR_PROPERTY_UNAVAILABLE until container parsing completes
 * or when streaming endless live audio.
 */
int ansimuz_mpv_get_duration(
    ansimuz_mpv *player,
    double *out_seconds
) {
    if (!player || !player->handle || !out_seconds) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    return mpv_get_property(
        player->handle,
        "duration",
        MPV_FORMAT_DOUBLE,
        out_seconds
    );
}

/*
 * Translates native libmpv status codes to human readable static strings.
 * Keeps error formatting consistent across the FFI boundary.
 */
const char *ansimuz_mpv_error_string(int status) {
    return mpv_error_string(status);
}
