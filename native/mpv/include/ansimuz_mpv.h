#ifndef ANSIMUZ_MPV_H
#define ANSIMUZ_MPV_H
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif

/*
 * Opaque wrapper around an mpv client instance.
 * Exposes a minimal C ABI suitable for Rust FFI bindings.
 */
typedef struct ansimuz_mpv ansimuz_mpv;

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

#ifdef __cplusplus
}
#endif

#endif
