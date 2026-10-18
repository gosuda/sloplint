typedef enum {
    Red,
    Green,
    Blue
} Color;

int Pick(Color color) {
    switch (color) {
        case Red:
            return 1;
        case Green:
            return 2;
        default:
            return 0;
    }
}
