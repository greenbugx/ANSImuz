#ifndef ANSIMUZ_MPV_H
#define ANSIMUZ_MPV_H
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif

/*
 * Maximum number of native events inspected per ansimuz_mpv_poll_event call.
 * Prevents high volume native event bursts from starving the caller thread.
 */
#define ANSIMUZ_MPV_MAX_DRAIN 64

/*
 * Opaque wrapper around an mpv client instance.
 * Exposes a minimal C ABI suitable for Rust FFI bindings.
 */
typedef struct ansimuz_mpv ansimuz_mpv;

/*
 * Normalized playback event types.
 *
 * Event contract:
 *   FILE_LOADED      Stream opened and container headers parsed successfully
 *   PLAYBACK_RESTART Audio rendering started or resumed after seek or pause
 *   PROPERTY_CHANGE  Observed property updated (duration, pause, volume)
 *   END_FILE         Track playback finished, stopped, replaced, or failed
 *   ERROR            Engine or command failure outside track lifecycle
 *   NONE             No event available within drain batch limit
 */
typedef enum ansimuz_mpv_event_type {
    ANSIMUZ_MPV_EVENT_NONE = 0,
    ANSIMUZ_MPV_EVENT_FILE_LOADED,
    ANSIMUZ_MPV_EVENT_PLAYBACK_RESTART,
    ANSIMUZ_MPV_EVENT_PROPERTY_CHANGE,
    ANSIMUZ_MPV_EVENT_END_FILE,
    ANSIMUZ_MPV_EVENT_ERROR
} ansimuz_mpv_event_type;

/* Observed playback properties */
typedef enum ansimuz_mpv_property {
    ANSIMUZ_MPV_PROPERTY_NONE = 0,
    ANSIMUZ_MPV_PROPERTY_TIME_POS,
    ANSIMUZ_MPV_PROPERTY_DURATION,
    ANSIMUZ_MPV_PROPERTY_PAUSE,
    ANSIMUZ_MPV_PROPERTY_VOLUME
} ansimuz_mpv_property;

/* Reasons why media playback stopped or ended */
typedef enum ansimuz_mpv_end_reason {
    ANSIMUZ_MPV_END_UNKNOWN = 0,
    ANSIMUZ_MPV_END_EOF,
    ANSIMUZ_MPV_END_STOP,
    ANSIMUZ_MPV_END_QUIT,
    ANSIMUZ_MPV_END_ERROR,
    ANSIMUZ_MPV_END_REDIRECT
} ansimuz_mpv_end_reason;

/* Normalized event payload passed across the C ABI */
typedef struct ansimuz_mpv_event {
    ansimuz_mpv_event_type type;

    /* Monotonically increasing load sequence identifier */
    uint64_t load_id;

    /* Populated for PROPERTY_CHANGE */
    ansimuz_mpv_property property;

    /* Property value for numeric or boolean properties */
    double value;

    /* Populated for END_FILE */
    ansimuz_mpv_end_reason end_reason;

    /* Native libmpv error code, zero when not applicable */
    int error_code;
} ansimuz_mpv_event;

/*
 * Allocates and initializes a dedicated mpv playback instance.
 *
 * Configures the instance for headless audio only playback before initialization.
 * On failure out_player is set to NULL and an error description is copied into
 * error_buffer if provided.
 *
 * Returns 0 on success or a negative mpv error code on failure.
 */
int ansimuz_mpv_create(
    ansimuz_mpv **out_player,
    char *error_buffer,
    size_t error_buffer_size
);

/*
 * Tears down the player instance and terminates background worker threads.
 * Safe to call with a NULL pointer.
 */
void ansimuz_mpv_destroy(ansimuz_mpv *player);

/*
 * Loads an audio file path or streaming URL into the playback engine.
 *
 * Uses replace mode so any currently playing audio stream is halted and evicted.
 * Playback starts asynchronously on the core demuxer thread.
 * Increments the internal monotonic load counter on successful dispatch.
 *
 * Returns 0 on command dispatch success or a negative mpv error code.
 */
int ansimuz_mpv_loadfile(ansimuz_mpv *player, const char *path);

/*
 * Returns the sequence identifier of the most recently initiated load.
 * Returns 0 if no track has been loaded.
 */
uint64_t ansimuz_mpv_get_last_load_id(const ansimuz_mpv *player);

/*
 * Sets playback pause state.
 *
 * Pass non zero to pause playback or 0 to resume.
 * Halts audio rendering without flushing audio pipeline buffers.
 *
 * Returns 0 on success or a negative mpv error code.
 */
int ansimuz_mpv_set_pause(ansimuz_mpv *player, int paused);

/*
 * Stops playback and clears the loaded file.
 *
 * Flushes active decoders and resets internal playback state without
 * destroying the core engine instance.
 *
 * Returns 0 on success or a negative mpv error code.
 */
int ansimuz_mpv_stop(ansimuz_mpv *player);

/*
 * Seeks to an absolute playback position in seconds.
 *
 * Clamps target to valid media bounds on the core thread.
 * Negative timestamps are rejected upfront as invalid arguments.
 *
 * Returns 0 on success or a negative mpv error code.
 */
int ansimuz_mpv_seek(ansimuz_mpv *player, double seconds);

/*
 * Queries current playback progress in seconds.
 *
 * Writes elapsed track position to out_seconds.
 * May return MPV_ERROR_PROPERTY_UNAVAILABLE if called during stream loading
 * or before initial demuxer metadata is resolved.
 *
 * Returns 0 on success or a negative mpv error code.
 */
int ansimuz_mpv_get_time_pos(
    ansimuz_mpv *player,
    double *out_seconds
);

/*
 * Queries total duration of the currently loaded media in seconds.
 *
 * Writes total length to out_seconds once container headers are parsed.
 * Returns MPV_ERROR_PROPERTY_UNAVAILABLE for endless live streams or before
 * the demuxer finishes analyzing the stream header.
 *
 * Returns 0 on success or a negative mpv error code.
 */
int ansimuz_mpv_get_duration(
    ansimuz_mpv *player,
    double *out_seconds
);

/*
 * Converts an mpv error code to a static null terminated error description.
 * Useful for building diagnostics without pulling libmpv headers into Rust.
 */
const char *ansimuz_mpv_error_string(int status);

/*
 * Retrieves the next pending event without blocking.
 *
 * Drains up to ANSIMUZ_MPV_MAX_DRAIN native events per call to avoid
 * starving the caller thread when the internal queue is busy.
 *
 * Returns 1 if an event was retrieved, 0 if no event was available or the
 * drain limit was reached, or a negative mpv error code on invalid parameters.
 *
 * The caller owns the output structure and can reuse it after each call.
 */
int ansimuz_mpv_poll_event(
    ansimuz_mpv *player,
    ansimuz_mpv_event *out_event
);

#ifdef __cplusplus
}
#endif

#endif
