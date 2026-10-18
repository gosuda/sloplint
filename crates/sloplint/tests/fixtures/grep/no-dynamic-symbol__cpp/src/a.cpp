#include <dlfcn.h>
void* load(void* h) { return dlsym(h, "sym"); }
