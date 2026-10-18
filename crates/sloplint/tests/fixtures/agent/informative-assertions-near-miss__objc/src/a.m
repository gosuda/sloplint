#include <assert.h>

int CheckIt(int ok) {
    assert(ok && "cache must be warm");
    return ok;
}
