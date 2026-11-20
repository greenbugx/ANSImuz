#include <stdio.h>
#include <mpv/client.h>

int main(void)
{
    mpv_handle *mpv = mpv_create();

    if (!mpv) {
        fprintf(stderr, "Failed to create mpv instance\n");
        return 1;
    }

    mpv_set_option_string(mpv, "config", "no");
    mpv_set_option_string(mpv, "terminal", "no");
    mpv_set_option_string(mpv, "video", "no");

    int status = mpv_initialize(mpv);

    if (status < 0) {
        fprintf(stderr, "mpv initialization failed: %s\n",
                mpv_error_string(status));

        mpv_destroy(mpv);
        return 1;
    }

    printf("libmpv initialized successfully!\n");
    printf("libmpv API version: %llu\n",
           (unsigned long long)mpv_client_api_version());

    mpv_terminate_destroy(mpv);

    printf("libmpv shut down successfully!\n");

    return 0;
}
