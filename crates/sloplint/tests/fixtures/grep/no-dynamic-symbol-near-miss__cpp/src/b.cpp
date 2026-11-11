#include <dlfcn.h>
void* load_literal(void* h) { return dlsym(h, "symbol_name"); }
void* load_win(void* h) { return GetProcAddress(h, "symbol_name"); }
