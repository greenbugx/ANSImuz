#include <stdio.h>
#include "ansimuz_mpv.h"

/*
 * Standalone verification binary to validate native library linking and
 * basic instance lifecycle before integrating with the Rust build script.
 */
int main(void) {
    ansimuz_mpv *player = NULL;
    char error[512] = {0};

    /* Verify instance creation and audio only option initialization */
    int status = ansimuz_mpv_create(
        &player,
        error,
        sizeof(error)
    );

    if (status != 0) {
        fprintf(stderr, "ANSImuz mpv error: %s\n", error);
        return 1;
    }

    printf("ANSImuz native backend initialized!\n");

    /* Ensure clean shutdown and thread termination */
    ansimuz_mpv_destroy(player);

    printf("ANSImuz native backend destroyed!\n");

    return 0;
}
