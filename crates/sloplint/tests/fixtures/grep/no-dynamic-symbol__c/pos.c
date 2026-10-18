#define PASTE(a, b) a##b

int paste_demo(void) {
    return PASTE(1, 2);
}
