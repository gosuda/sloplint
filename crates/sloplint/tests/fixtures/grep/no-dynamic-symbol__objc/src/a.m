#include <dlfcn.h>

int LoadIt(void) {
    void *handle = dlsym(0, "symbol_name");
    return handle != 0;
}
