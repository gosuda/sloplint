#include <dlfcn.h>

int LoadIt(const char *name) {
    void *handle = dlsym(0, name);
    return handle != 0;
}
