#include <dlfcn.h>
void* load(void* h, const char* name) { return dlsym(h, name); }
