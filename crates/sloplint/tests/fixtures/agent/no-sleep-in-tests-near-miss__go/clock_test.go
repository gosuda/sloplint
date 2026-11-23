package clocks

import (
    "testing"
    "time"
    bubble "testing/synctest"
)

func TestClock(t *testing.T) {
    time.Sleep(0)
    time.Sleep(-1)
    bubble.Test(t, func(t *testing.T) {
        time.Sleep(time.Hour)
    })
    bubble.Run(func() { time.Sleep(time.Hour) })
}

func BenchmarkClock(b *testing.B) {
    time.Sleep(1)
}

func ExampleClock() {
    time.Sleep(1)
}
