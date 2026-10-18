package main
import "os"
var Counter int
func main() {
	println(Counter, os.Getenv("HOME"))
}
