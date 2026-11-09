#include "horizon_ios.h"

// Horizon's own client (milestone M4). Horizon creates its winit event loop, window and renderer.
int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    horizon_ios_game_main();
    return 0;
}
