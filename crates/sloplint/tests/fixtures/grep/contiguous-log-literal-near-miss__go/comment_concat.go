package logcall

import "log"

// A block comment beside '+' must not hide the concat: the message
// still leads with a searchable literal, so the call stays silent.
func Warn(detail string) {
	log.Println("a long searchable literal " /* note */ + detail)
}
