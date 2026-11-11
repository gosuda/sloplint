#include <dlfcn.h>
void use_literal(void* h) {
    void* sym = dlsym(h, "symbol_name");
    NSSelectorFromString(@"run");
}
