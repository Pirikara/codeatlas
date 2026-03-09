package main

type Runner interface {
	Run(x int) error
	Status() string
}

// WrongRunner: Run has no args → name matches but signature does not → should NOT IMPLEMENTS Runner
type WrongRunner struct{}

func (r *WrongRunner) Run() error      { return nil }
func (r *WrongRunner) Status() string  { return "" }

// CorrectRunner: signature fully matches → should IMPLEMENTS Runner
type CorrectRunner struct{}

func (r *CorrectRunner) Run(x int) error { return nil }
func (r *CorrectRunner) Status() string  { return "" }

func main() {}
