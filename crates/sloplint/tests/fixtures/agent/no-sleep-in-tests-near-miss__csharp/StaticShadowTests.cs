using static System.Threading.Thread;
class StaticShadowTests {
    void Sleep(int millis) {}
    void AdvancesClock() {
        Sleep(10);
    }
}
