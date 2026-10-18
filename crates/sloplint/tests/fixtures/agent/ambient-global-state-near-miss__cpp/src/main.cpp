const int LIMIT = 100;
#include <cstdlib>
int main() {
    const char* home = getenv("HOME");
    return home ? 0 : LIMIT;
}
