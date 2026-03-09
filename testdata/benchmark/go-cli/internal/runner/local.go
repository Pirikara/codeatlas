package runner

import (
	"fmt"

	"go-cli/internal/config"
)

type LocalRunner struct {
	config *config.Config
}

func NewLocalRunner(cfg *config.Config) *LocalRunner {
	return &LocalRunner{config: cfg}
}

func (r *LocalRunner) Run(target string) (*Result, error) {
	if r.config.Verbose {
		fmt.Printf("Running target: %s\n", target)
	}

	output := executeTarget(target)

	return &Result{
		Name:    target,
		Success: true,
		Output:  output,
	}, nil
}

func (r *LocalRunner) Status() (*Result, error) {
	return &Result{
		Name:    "status",
		Success: true,
		Output:  fmt.Sprintf("timeout=%d verbose=%v", r.config.Timeout, r.config.Verbose),
	}, nil
}

func executeTarget(target string) string {
	return fmt.Sprintf("executed: %s", target)
}
