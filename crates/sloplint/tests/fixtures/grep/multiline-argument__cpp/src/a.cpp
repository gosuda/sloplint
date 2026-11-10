#include <initializer_list>
int compute(std::initializer_list<int> xs);
int f() {
    return compute({
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        12,
        13,
        14,
        15,
    });
}
