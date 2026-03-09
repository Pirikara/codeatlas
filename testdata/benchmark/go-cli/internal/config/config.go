package config

type Config struct {
	Timeout int
	Verbose bool
	Output  string
}

type Option func(*Config)

func WithTimeout(t int) Option {
	return func(c *Config) {
		c.Timeout = t
	}
}

func WithVerbose(v bool) Option {
	return func(c *Config) {
		c.Verbose = v
	}
}

func WithOutput(o string) Option {
	return func(c *Config) {
		c.Output = o
	}
}

func Load(opts ...Option) (*Config, error) {
	cfg := &Config{
		Timeout: 10,
		Verbose: false,
		Output:  "text",
	}

	for _, opt := range opts {
		opt(cfg)
	}

	return cfg, nil
}

func DefaultConfig() *Config {
	cfg, _ := Load()
	return cfg
}
