struct Renderer { virtual void render(bool verbose) = 0; };
struct Console : Renderer { void render(bool verbose) override { (void)verbose; } };
static bool helper(bool flag) { return !flag; }
