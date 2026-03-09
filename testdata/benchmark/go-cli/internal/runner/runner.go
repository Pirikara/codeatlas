package runner

type Result struct {
	Name    string
	Success bool
	Output  string
}

type Runner interface {
	Run(target string) (*Result, error)
	Status() (*Result, error)
}
