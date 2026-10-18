#include <assert.h>

int positive(int n) {
    assert(n > 0 && "n must be positive");
    return n;
}
