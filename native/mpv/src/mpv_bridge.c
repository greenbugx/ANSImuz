#include "ansimuz_mpv.h"
#include <mpv/client.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/*
 * Maximum number of concurrent in-flight track loads tracked in ring buffer.
 * Holds load records during asynchronous transitions and track replacements.
 */
#define ANSIMUZ_MPV_MAX_TRACKED_LOADS 16

/*
 * Association between a native mpv playlist entry and an application load ID.
 */
typedef struct track_load_entry {
    int64_t playlist_entry_id;
    uint64_t load_id;
    int is_loaded;
} track_load_entry;

/*
 * Internal handle holder. Keeping mpv_handle internal insulates callers
 * from libmpv header drift and keeps the C ABI boundary clean.
 * Correlates native mpv playlist entry identifiers with monotonic application
 * load IDs to ensure accurate event attribution across track replacements.
 */
struct ansimuz_mpv {
    mpv_handle *handle;
    uint64_t next_load_id;
    uint64_t active_load_id;

    track_load_entry load_queue[ANSIMUZ_MPV_MAX_TRACKED_LOADS];
    int load_queue_head;
    int load_queue_count;
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

    return 0;
}

/*
 * Enqueues a pending load request into the internal tracking ring buffer.
 * Drops the oldest entry if capacity is exhausted under rapid replacement.
 */
static void enqueue_load(ansimuz_mpv *player, uint64_t load_id) {
    if (player->load_queue_count >= ANSIMUZ_MPV_MAX_TRACKED_LOADS) {
        player->load_queue_head =
            (player->load_queue_head + 1) % ANSIMUZ_MPV_MAX_TRACKED_LOADS;
        player->load_queue_count--;
    }

    int idx = (player->load_queue_head + player->load_queue_count) %
              ANSIMUZ_MPV_MAX_TRACKED_LOADS;
    player->load_queue[idx].playlist_entry_id = -1;
    player->load_queue[idx].load_id = load_id;
    player->load_queue[idx].is_loaded = 0;
    player->load_queue_count++;
}

/*
 * Associates a native mpv playlist entry ID with the earliest unbound load record.
 */
static void bind_playlist_entry(ansimuz_mpv *player, int64_t playlist_entry_id) {
    for (int i = 0; i < player->load_queue_count; i++) {
        int idx = (player->load_queue_head + i) % ANSIMUZ_MPV_MAX_TRACKED_LOADS;
        if (player->load_queue[idx].playlist_entry_id == -1) {
            player->load_queue[idx].playlist_entry_id = playlist_entry_id;
            break;
        }
    }
}

/*
 * Marks the active entry as loaded and returns its correlated load ID.
 */
static uint64_t mark_file_loaded(ansimuz_mpv *player) {
    for (int i = 0; i < player->load_queue_count; i++) {
        int idx = (player->load_queue_head + i) % ANSIMUZ_MPV_MAX_TRACKED_LOADS;
        if (player->load_queue[idx].playlist_entry_id != -1 &&
            !player->load_queue[idx].is_loaded) {
            player->load_queue[idx].is_loaded = 1;
            return player->load_queue[idx].load_id;
        }
    }

    if (player->load_queue_count > 0) {
        int idx = (player->load_queue_head + player->load_queue_count - 1) %
                  ANSIMUZ_MPV_MAX_TRACKED_LOADS;
        player->load_queue[idx].is_loaded = 1;
        return player->load_queue[idx].load_id;
    }

    return player->next_load_id;
}

/*
 * Removes a finished or failed track by its native playlist entry identifier.
 * Returns the correlated load identifier, or 0 if no match was found.
 */
static uint64_t remove_load_entry(ansimuz_mpv *player, int64_t playlist_entry_id) {
    for (int i = 0; i < player->load_queue_count; i++) {
        int idx = (player->load_queue_head + i) % ANSIMUZ_MPV_MAX_TRACKED_LOADS;
        if (player->load_queue[idx].playlist_entry_id == playlist_entry_id) {
            uint64_t matched_id = player->load_queue[idx].load_id;

            for (int j = i; j < player->load_queue_count - 1; j++) {
                int curr = (player->load_queue_head + j) %
                           ANSIMUZ_MPV_MAX_TRACKED_LOADS;
                int next = (player->load_queue_head + j + 1) %
                           ANSIMUZ_MPV_MAX_TRACKED_LOADS;
                player->load_queue[curr] = player->load_queue[next];
            }
            player->load_queue_count--;
            return matched_id;
        }
    }

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
 * Increments the monotonic load counter and enqueues a pending tracking record.
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
    if (status >= 0) {
        player->next_load_id++;
        enqueue_load(player, player->next_load_id);
    }

    return status;
}

/*
 * Returns the sequence identifier of the most recently initiated load.
 * Returns 0 if no track has been loaded.
 */
uint64_t ansimuz_mpv_get_last_load_id(const ansimuz_mpv *player) {
    if (!player) {
        return 0;
    }
    return player->next_load_id;
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
                player->active_load_id : player->next_load_id;
            return 1;
        }

        switch (event->event_id) {
            case MPV_EVENT_START_FILE: {
                mpv_event_start_file *start =
                    (mpv_event_start_file *)event->data;

                if (start) {
                    bind_playlist_entry(player, start->playlist_entry_id);
                }

                /* START_FILE is internal bookkeeping so continue draining */
                continue;
            }

            case MPV_EVENT_FILE_LOADED: {
                uint64_t loaded_id = mark_file_loaded(player);
                player->active_load_id = loaded_id;
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
                }

                /*
                 * Correlate track completion with its native playlist entry ID.
                 * Ensures displacement end events from previous tracks match
                 * their original load sequence ID even after a new loadfile call.
                 */
                uint64_t matched_id = 0;
                if (end && end->playlist_entry_id > 0) {
                    matched_id = remove_load_entry(player, end->playlist_entry_id);
                }

                if (matched_id == 0) {
                    /* Fallback when playlist entry ID was not provided */
                    if (player->active_load_id != 0) {
                        matched_id = player->active_load_id;
                    } else if (player->load_queue_count > 0) {
                        int idx = player->load_queue_head;
                        matched_id = player->load_queue[idx].load_id;
                        player->load_queue_head =
                            (player->load_queue_head + 1) %
                            ANSIMUZ_MPV_MAX_TRACKED_LOADS;
                        player->load_queue_count--;
                    } else {
                        matched_id = player->next_load_id;
                    }
                }

                out_event->load_id = matched_id;

                if (player->active_load_id == matched_id) {
                    player->active_load_id = 0;
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
