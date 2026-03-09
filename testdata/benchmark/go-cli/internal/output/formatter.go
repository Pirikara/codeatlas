package output

import (
	"go-cli/internal/runner"
)

type Formatter interface {
	Format(result *runner.Result) error
}
