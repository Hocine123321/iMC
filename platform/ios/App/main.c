#include "horizon_ios.h"

// Everything else (the UIApplication, its delegate, the window and the Metal view) is created by
// winit from the Rust side.
int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    horizon_ios_main();
    return 0;
}
