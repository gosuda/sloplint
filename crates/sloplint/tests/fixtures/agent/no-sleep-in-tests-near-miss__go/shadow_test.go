package clocks
import clock "time"
var _ = clock.Second
type sleeper struct { Sleep func(int) }
func useFake(clock sleeper) {
    clock.Sleep(1)
}
