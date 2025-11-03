package cache

import "context"

// Layer is a lightweight cache abstraction used for targeted response caching.
type Layer interface {
	Get(ctx context.Context, key string) ([]byte, error)
	Set(ctx context.Context, key string, value []byte, ttlSeconds int) error
}

// Noop implements a zero-effect cache layer.
type Noop struct{}

// Get always returns nil with no error.
func (Noop) Get(context.Context, string) ([]byte, error) {
	return nil, nil
}

// Set performs no operation and always succeeds.
func (Noop) Set(context.Context, string, []byte, int) error {
	return nil
}
