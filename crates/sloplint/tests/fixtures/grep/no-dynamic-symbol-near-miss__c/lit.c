#include <dlfcn.h>
void* load_literal(void* h) { return dlsym(h, "symbol_name"); }
void* load_adjacent(void* h) { return dlsym(h, "sym" "bol_name"); }
