using System.Threading;
class ShadowTests {
    void UsesFake(Sleeper @Thread) {
        Thread.Sleep(10);
    }
}
