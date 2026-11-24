#include <math.h>
#include <stdint.h>
#include <stddef.h>
#include "ansimuz_mpv.h"
#include <mpv/client.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/*
 * Internal handle holder
 * Keeps mpv_handle internal to isolate Rust FFI from libmpv header drift
 */
struct ansimuz_mpv {
    mpv_handle *handle;
    uint64_t current_load_id;
    uint64_t active_load_id;
    int64_t current_entry_id;
    int64_t previous_entry_id;
    uint64_t previous_load_id;
    FILE *log_file;
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

/*
 * Registers property observation hooks on the initialized mpv instance.
 * Uses discrete integer reply_userdata IDs to avoid string lookups in poll loop.
 *
 * High frequency properties like time-pos are intentionally omitted from
 * observation. Position changes occur continuously during playback and can
 * saturate libmpv internal event ring buffer if the client event loop is busy
 * rendering or processing input. Callers query playback position on demand
 * via ansimuz_mpv_get_time_pos instead.
 */
static int observe_properties(mpv_handle *handle) {
    int status;

    status = mpv_observe_property(
        handle, 1, "time-pos", MPV_FORMAT_DOUBLE
    );
    if (status < 0) return status;

    status = mpv_observe_property(
        handle, 2, "duration", MPV_FORMAT_DOUBLE
    );
    if (status < 0) return status;

    status = mpv_observe_property(
        handle, 3, "pause", MPV_FORMAT_FLAG
    );
    if (status < 0) return status;

    status = mpv_observe_property(
        handle, 4, "volume", MPV_FORMAT_DOUBLE
    );
    if (status < 0) return status;

    status = mpv_observe_property(
        handle, 5, "mute", MPV_FORMAT_FLAG
    );
    if (status < 0) return status;

    return 0;
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

    /* Check that runtime libmpv major version matches the header version */
    unsigned long runtime = mpv_client_api_version();
    if ((runtime >> 16) != (MPV_CLIENT_API_VERSION >> 16)) {
        if (error_buffer && error_buffer_size > 0) {
            snprintf(
                error_buffer,
                error_buffer_size,
                "libmpv major version mismatch: runtime %lu, headers %lu",
                runtime >> 16,
                (unsigned long)(MPV_CLIENT_API_VERSION >> 16)
            );
        }
        return MPV_ERROR_UNSUPPORTED;
    }

    ansimuz_mpv *player = calloc(1, sizeof(*player));

    if (!player) {
        return -1;
    }

    player->current_entry_id = -1;
    player->previous_entry_id = -1;

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
     * Disable built-in Lua and JavaScript scripts
     * Cuts down startup time and prevents script command hooks from running
     */
    status = configure_option(
        player->handle, "load-scripts", "no",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    /*
     * Disable on-screen controller and default keyboard bindings
     */
    status = configure_option(
        player->handle, "osc", "no",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    status = configure_option(
        player->handle, "input-default-bindings", "no",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    status = configure_option(
        player->handle, "input-vo-keyboard", "no",
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
     * Silence all terminal messages to prevent corrupting the TUI.
     */
    status = configure_option(
        player->handle, "msg-level", "all=no",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    /*
     * Enable mpv internal ytdl hook for direct stream URL resolution.
     */
    status = configure_option(
        player->handle, "ytdl", "yes",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    /*
     * Disable window creation and idle mode to match CLI audio playback arguments.
     */
    status = configure_option(
        player->handle, "force-window", "no",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    /*
     * Enable idle mode in libmpv so the player waits idly for commands
     * rather than exiting immediately when the playlist is empty.
     */
    status = configure_option(
        player->handle, "idle", "yes",
        error_buffer, error_buffer_size
    );

    if (status < 0) goto fail;

    /*
     * Optional audio output driver override for headless or test environments
     */
    const char *ao_env = getenv("ANSIMUZ_MPV_AO");
    if (ao_env && *ao_env) {
        status = configure_option(
            player->handle, "ao", ao_env,
            error_buffer, error_buffer_size
        );

        if (status < 0) goto fail;
    }

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

    /* Optional log file destination for diagnostics */
    const char *log_path = getenv("ANSIMUZ_MPV_LOG");
    if (log_path && *log_path) {
        player->log_file = fopen(log_path, "a");
        if (player->log_file) {
            mpv_request_log_messages(player->handle, "info");
        }
    }

    /*
     * Register property observers immediately after initialize succeeds.
     * If observer setup fails, tear down the initialized engine before exiting.
     */
    status = observe_properties(player->handle);

    if (status < 0) {
        set_error(
            error_buffer,
            error_buffer_size,
            "observe_properties",
            status
        );

        mpv_terminate_destroy(player->handle);
        free(player);
        return status;
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

    if (player->log_file) {
        fclose(player->log_file);
        player->log_file = NULL;
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
 * Loads an audio file or stream URL by issuing the loadfile replace command
 * Zero allocations, delegates directly to libmpv command dispatch
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

    int status = mpv_command(player->handle, args);
    if (status < 0) {
        return status;
    }

    player->previous_entry_id = player->current_entry_id;
    player->previous_load_id = player->active_load_id;
    player->current_load_id++;
    player->active_load_id = player->current_load_id;

    if (player->log_file) {
        fprintf(player->log_file,
            "[LOAD] load_id=%llu path=%s\n",
            (unsigned long long)player->current_load_id,
            path
        );
        fflush(player->log_file);
    }

    return 0;
}

/*
 * Returns the sequence identifier of the most recently initiated load
 * Returns 0 if no track has been loaded
 */
uint64_t ansimuz_mpv_get_last_load_id(const ansimuz_mpv *player) {
    if (!player) {
        return 0;
    }
    return player->current_load_id;
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

    player->active_load_id = 0;
    player->current_entry_id = -1;
    player->previous_entry_id = -1;

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
 * Sets playback volume between 0.0 and 100.0.
 * Rejects non finite numbers and values outside allowable percentage bounds.
 */
int ansimuz_mpv_set_volume(
    ansimuz_mpv *player,
    double volume
) {
    if (!player || !player->handle ||
        !isfinite(volume) || volume < 0.0 || volume > 100.0) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    return mpv_set_property(
        player->handle,
        "volume",
        MPV_FORMAT_DOUBLE,
        &volume
    );
}

/*
 * Reads current playback volume level.
 * Writes a double precision value between 0.0 and 100.0 to out_volume.
 */
int ansimuz_mpv_get_volume(
    ansimuz_mpv *player,
    double *out_volume
) {
    if (!player || !player->handle || !out_volume) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    return mpv_get_property(
        player->handle,
        "volume",
        MPV_FORMAT_DOUBLE,
        out_volume
    );
}

/*
 * Enables or disables audio mute.
 * Writes integer flag to mpv mute property without changing volume levels.
 */
int ansimuz_mpv_set_mute(
    ansimuz_mpv *player,
    int muted
) {
    if (!player || !player->handle) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    int value = muted ? 1 : 0;

    return mpv_set_property(
        player->handle,
        "mute",
        MPV_FORMAT_FLAG,
        &value
    );
}

/*
 * Reads current mute state.
 * Writes 1 if audio output is muted or 0 if audio is unmuted.
 */
int ansimuz_mpv_get_mute(
    ansimuz_mpv *player,
    int *out_muted
) {
    if (!player || !player->handle || !out_muted) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    return mpv_get_property(
        player->handle,
        "mute",
        MPV_FORMAT_FLAG,
        out_muted
    );
}

/*
 * Translates native libmpv status codes to human readable static strings.
 * Keeps error formatting consistent across the FFI boundary.
 */
const char *ansimuz_mpv_error_string(int status) {
    return mpv_error_string(status);
}

/*
 * Maps numeric observation userdata IDs to enum property identifiers.
 */
static ansimuz_mpv_property map_property_id(uint64_t id) {
    switch (id) {
        case 1:
            return ANSIMUZ_MPV_PROPERTY_TIME_POS;
        case 2:
            return ANSIMUZ_MPV_PROPERTY_DURATION;
        case 3:
            return ANSIMUZ_MPV_PROPERTY_PAUSE;
        case 4:
            return ANSIMUZ_MPV_PROPERTY_VOLUME;
        case 5:
            return ANSIMUZ_MPV_PROPERTY_MUTE;
        default:
            return ANSIMUZ_MPV_PROPERTY_NONE;
    }
}

/*
 * Maps libmpv playback termination reasons to ANSImuz end reasons.
 */
static ansimuz_mpv_end_reason map_end_reason(mpv_end_file_reason reason) {
    switch (reason) {
        case MPV_END_FILE_REASON_EOF:
            return ANSIMUZ_MPV_END_EOF;
        case MPV_END_FILE_REASON_STOP:
            return ANSIMUZ_MPV_END_STOP;
        case MPV_END_FILE_REASON_QUIT:
            return ANSIMUZ_MPV_END_QUIT;
        case MPV_END_FILE_REASON_ERROR:
            return ANSIMUZ_MPV_END_ERROR;
        case MPV_END_FILE_REASON_REDIRECT:
            return ANSIMUZ_MPV_END_REDIRECT;
        default:
            return ANSIMUZ_MPV_END_UNKNOWN;
    }
}

/*
 * Polls the native mpv event queue without blocking.
 *
 * Discards unexposed internal notifications until a relevant event is found
 * or the drain limit of ANSIMUZ_MPV_MAX_DRAIN is reached.
 * Correlates events with native playlist entry IDs to guarantee that late
 * notifications from displaced tracks are never misattributed to new requests.
 */
int ansimuz_mpv_poll_event(
    ansimuz_mpv *player,
    ansimuz_mpv_event *out_event
) {
    if (!player || !player->handle || !out_event) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    memset(out_event, 0, sizeof(*out_event));

    for (int drain_count = 0; drain_count < ANSIMUZ_MPV_MAX_DRAIN; drain_count++) {
        /* Zero timeout ensures this never blocks the calling thread */
        mpv_event *event = mpv_wait_event(player->handle, 0);

        if (!event || event->event_id == MPV_EVENT_NONE) {
            out_event->type = ANSIMUZ_MPV_EVENT_NONE;
            return 0;
        }

        /*
         * Surface native asynchronous command or engine errors.
         * Track loading and demuxer failures do not arrive here.
         * They are delivered as ANSIMUZ_MPV_EVENT_END_FILE with
         * end_reason set to ANSIMUZ_MPV_END_ERROR.
         */
        if (event->error < 0 && event->event_id != MPV_EVENT_END_FILE) {
            out_event->type = ANSIMUZ_MPV_EVENT_ERROR;
            out_event->error_code = event->error;
            out_event->load_id = player->active_load_id ?
                player->active_load_id : player->current_load_id;
            return 1;
        }

        switch (event->event_id) {
            case MPV_EVENT_LOG_MESSAGE: {
                mpv_event_log_message *msg =
                    (mpv_event_log_message *)event->data;

                if (msg && player->log_file) {
                    fprintf(player->log_file, "[MPV:%s] %s",
                            msg->level ? msg->level : "unknown",
                            msg->text ? msg->text : "");
                    fflush(player->log_file);
                }

                continue;
            }

            case MPV_EVENT_START_FILE: {
                mpv_event_start_file *start =
                    (mpv_event_start_file *)event->data;

                if (start) {
                    player->current_entry_id = start->playlist_entry_id;
                    player->active_load_id = player->current_load_id;

                    if (player->log_file) {
                        fprintf(player->log_file,
                            "[NATIVE START_FILE] entry_id=%lld load_id=%llu\n",
                            (long long)start->playlist_entry_id,
                            (unsigned long long)player->active_load_id
                        );
                        fflush(player->log_file);
                    }
                }

                continue;
            }

            case MPV_EVENT_FILE_LOADED: {
                out_event->type = ANSIMUZ_MPV_EVENT_FILE_LOADED;
                out_event->load_id = player->active_load_id;
                return 1;
            }

            case MPV_EVENT_PLAYBACK_RESTART:
                /* Only attribute restart events if a track is actively loaded */
                if (player->active_load_id == 0) {
                    continue;
                }

                out_event->type = ANSIMUZ_MPV_EVENT_PLAYBACK_RESTART;
                out_event->load_id = player->active_load_id;
                return 1;

            case MPV_EVENT_PROPERTY_CHANGE: {
                /*
                 * Discard transient property updates if no track is active.
                 * Prevents unpolled notifications from displaced tracks from
                 * leaking into new track lifecycle and breaking lyrics sync.
                 */
                if (player->active_load_id == 0) {
                    continue;
                }

                mpv_event_property *property =
                    (mpv_event_property *)event->data;

                if (!property) {
                    continue;
                }

                ansimuz_mpv_property id =
                    map_property_id(event->reply_userdata);

                /* Fallback to matching property name if userdata was unset */
                if (id == ANSIMUZ_MPV_PROPERTY_NONE && property->name) {
                    if (strcmp(property->name, "time-pos") == 0) {
                        id = ANSIMUZ_MPV_PROPERTY_TIME_POS;
                    } else if (strcmp(property->name, "duration") == 0) {
                        id = ANSIMUZ_MPV_PROPERTY_DURATION;
                    } else if (strcmp(property->name, "pause") == 0) {
                        id = ANSIMUZ_MPV_PROPERTY_PAUSE;
                    } else if (strcmp(property->name, "volume") == 0) {
                        id = ANSIMUZ_MPV_PROPERTY_VOLUME;
                    } else if (strcmp(property->name, "mute") == 0) {
                        id = ANSIMUZ_MPV_PROPERTY_MUTE;
                    }
                }

                if (id == ANSIMUZ_MPV_PROPERTY_NONE) {
                    continue;
                }

                if (property->format == MPV_FORMAT_DOUBLE && property->data) {
                    out_event->value = *(double *)property->data;
                } else if (property->format == MPV_FORMAT_FLAG && property->data) {
                    out_event->value = *(int *)property->data;
                } else {
                    /* Property value is temporarily unavailable */
                    continue;
                }

                out_event->type = ANSIMUZ_MPV_EVENT_PROPERTY_CHANGE;
                out_event->property = id;
                out_event->load_id = player->active_load_id;
                return 1;
            }

            case MPV_EVENT_END_FILE: {
                mpv_event_end_file *end =
                    (mpv_event_end_file *)event->data;

                out_event->type = ANSIMUZ_MPV_EVENT_END_FILE;

                if (end) {
                    out_event->end_reason =
                        map_end_reason(end->reason);

                    out_event->error_code = end->error;

                    if (player->previous_entry_id > 0 && end->playlist_entry_id == player->previous_entry_id) {
                        out_event->load_id = player->previous_load_id;
                    } else {
                        out_event->load_id = player->active_load_id;
                    }

                    if (player->log_file) {
                        fprintf(player->log_file,
                            "[NATIVE END_FILE] entry_id=%lld reason=%d error=%d load_id=%llu\n",
                            (long long)end->playlist_entry_id,
                            end->reason,
                            end->error,
                            (unsigned long long)out_event->load_id
                        );
                        fflush(player->log_file);
                    }
                } else {
                    out_event->load_id = player->active_load_id;
                }

                if (out_event->load_id == player->active_load_id && out_event->end_reason != ANSIMUZ_MPV_END_STOP) {
                    player->active_load_id = 0;
                    player->current_entry_id = -1;
                }

                return 1;
            }

            default:
                /* Discard unhandled native event and keep draining */
                continue;
        }
    }

    /* Drain limit reached without finding an exposed event */
    out_event->type = ANSIMUZ_MPV_EVENT_NONE;
    return 0;
}

/*
 * Dumps an audio stream to a file using a secondary headless mpv instance
 * Polls for completion while checking cancel_flag periodically
 * Returns 0 on success, -2 when cancelled, or a negative mpv error code
 */
int ansimuz_mpv_dump_stream(
    const char *url,
    const char *output_path,
    const uint8_t *cancel_flag
) {
    if (!url || !output_path || url[0] == '\0' || output_path[0] == '\0') {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    if (cancel_flag && *cancel_flag != 0) {
        return -2;
    }

    mpv_handle *handle = mpv_create();
    if (!handle) {
        return MPV_ERROR_NOMEM;
    }

    mpv_set_option_string(handle, "terminal", "no");
    mpv_set_option_string(handle, "msg-level", "all=no");
    mpv_set_option_string(handle, "video", "no");
    mpv_set_option_string(handle, "audio", "no");
    mpv_set_option_string(handle, "force-window", "no");
    mpv_set_option_string(handle, "idle", "no");
    mpv_set_option_string(handle, "config", "no");
    mpv_set_option_string(handle, "load-scripts", "no");
    mpv_set_option_string(handle, "osc", "no");
    mpv_set_option_string(handle, "input-default-bindings", "no");
    mpv_set_option_string(handle, "input-vo-keyboard", "no");
    mpv_set_option_string(handle, "ytdl", "yes");
    mpv_set_option_string(handle, "stream-dump", output_path);

    int status = mpv_initialize(handle);
    if (status < 0) {
        mpv_destroy(handle);
        return status;
    }

    const char *args[] = {
        "loadfile",
        url,
        "replace",
        NULL
    };

    status = mpv_command(handle, args);
    if (status < 0) {
        mpv_terminate_destroy(handle);
        return status;
    }

    int result = -1;
    while (1) {
        if (cancel_flag && *cancel_flag != 0) {
            result = -2;
            break;
        }

        mpv_event *event = mpv_wait_event(handle, 0.1);
        if (!event || event->event_id == MPV_EVENT_NONE) {
            continue;
        }

        if (event->event_id == MPV_EVENT_END_FILE) {
            mpv_event_end_file *end = (mpv_event_end_file *)event->data;
            if (end) {
                if (end->reason == MPV_END_FILE_REASON_EOF) {
                    result = 0;
                } else if (end->reason == MPV_END_FILE_REASON_ERROR) {
                    result = end->error < 0 ? end->error : MPV_ERROR_GENERIC;
                } else {
                    result = MPV_ERROR_GENERIC;
                }
            } else {
                result = 0;
            }
            break;
        }

        if (event->event_id == MPV_EVENT_SHUTDOWN) {
            break;
        }
    }

    mpv_terminate_destroy(handle);
    return result;
}
