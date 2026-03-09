package cmd

import (
	"fmt"

	"go-cli/internal/config"
	"go-cli/internal/output"
	"go-cli/internal/runner"
)

func Execute(args []string) error {
	cfg, err := config.Load(
		config.WithTimeout(30),
		config.WithVerbose(true),
	)
	if err != nil {
		return fmt.Errorf("failed to load config: %w", err)
	}

	formatter := output.NewJSONFormatter(cfg)
	r := runner.NewLocalRunner(cfg)

	if len(args) == 0 {
		return fmt.Errorf("no command specified")
	}

	switch args[0] {
	case "run":
		return runCommand(r, formatter, args[1:])
	case "status":
		return statusCommand(r, formatter)
	default:
		return fmt.Errorf("unknown command: %s", args[0])
	}
}

func runCommand(r runner.Runner, f output.Formatter, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("no target specified")
	}
	result, err := r.Run(args[0])
	if err != nil {
		return err
	}
	return f.Format(result)
}

func statusCommand(r runner.Runner, f output.Formatter) error {
	result, err := r.Status()
	if err != nil {
		return err
	}
	return f.Format(result)
}
