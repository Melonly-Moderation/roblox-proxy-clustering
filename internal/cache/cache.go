package cache

import (
	"context"
	"time"
)

// Entry represents a cached payload with metadata used for staleness checks.
type Entry struct {
	Payload  []byte
	StoredAt time.Time
}

// Store describes cache backends capable of storing opaque payloads with TTLs.
type Store interface {
	Get(ctx context.Context, key string) (Entry, bool, error)
	Set(ctx context.Context, key string, payload []byte, ttl time.Duration) error
}
