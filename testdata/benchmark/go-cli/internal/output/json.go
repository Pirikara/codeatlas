package output

import (
	"encoding/json"
	"fmt"

	"go-cli/internal/config"
	"go-cli/internal/runner"
)

type JSONFormatter struct {
	config *config.Config
}

func NewJSONFormatter(cfg *config.Config) *JSONFormatter {
	return &JSONFormatter{config: cfg}
}

func (f *JSONFormatter) Format(result *runner.Result) error {
	data, err := marshal(result)
	if err != nil {
		return fmt.Errorf("failed to marshal: %w", err)
	}
	fmt.Println(string(data))
	return nil
}

func marshal(v interface{}) ([]byte, error) {
	return json.MarshalIndent(v, "", "  ")
}
