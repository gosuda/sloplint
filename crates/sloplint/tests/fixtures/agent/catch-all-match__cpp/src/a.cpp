enum Color { Red, Green };
const char* describe(Color c) {
    switch (c) {
        case Red: return "red";
        default: return "other";
    }
}
