package main

// Ping and Pong call each other, forming an explicit A→B→A cycle.
// This fixture exists solely to test BFS cycle safety.

func Ping() {
	Pong()
}

func Pong() {
	Ping()
}

// Relay and Forward also form a 2-node mutual recursion cycle.
func Relay() {
	Forward()
}

func Forward() {
	Relay()
}

func main() {
	Ping()
	Relay()
}
