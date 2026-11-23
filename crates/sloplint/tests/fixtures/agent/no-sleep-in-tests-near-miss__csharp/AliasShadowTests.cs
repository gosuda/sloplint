using Worker = System.Threading.Thread;
class AliasShadowTests {
    void UsesFake(Sleeper Worker) {
        Worker.Sleep(10);
    }
}
