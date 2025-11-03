package redisstore

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/redis/go-redis/v9"

	"github.com/NoahCxrest/roblox-proxy-clustering/internal/cache"
)

// Store implements cache.Store backed by Redis.
type Store struct {
	client *redis.Client
}

type envelope struct {
	StoredAt time.Time       `json:"stored_at"`
	Payload  json.RawMessage `json:"payload"`
}

// New constructs a Redis-backed cache store.
func New(rawURL string) (*Store, error) {
	opts, err := redis.ParseURL(rawURL)
	if err != nil {
		return nil, fmt.Errorf("parse redis url: %w", err)
	}

	client := redis.NewClient(opts)

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	if err := client.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("redis ping failed: %w", err)
	}

	return &Store{client: client}, nil
}

// Client returns the underlying redis client.
func (s *Store) Client() *redis.Client {
	return s.client
}

// Close terminates the underlying Redis client connections.
func (s *Store) Close() error {
	return s.client.Close()
}

// Get retrieves a cached entry if present.
func (s *Store) Get(ctx context.Context, key string) (cache.Entry, bool, error) {
	data, err := s.client.Get(ctx, key).Bytes()
	if err != nil {
		if err == redis.Nil {
			return cache.Entry{}, false, nil
		}
		return cache.Entry{}, false, fmt.Errorf("redis get %q: %w", key, err)
	}

	var env envelope
	if err := json.Unmarshal(data, &env); err != nil {
		return cache.Entry{}, false, fmt.Errorf("decode cached payload %q: %w", key, err)
	}

	return cache.Entry{
		Payload:  append([]byte(nil), env.Payload...),
		StoredAt: env.StoredAt,
	}, true, nil
}

// Set stores a cached entry with the provided TTL.
func (s *Store) Set(ctx context.Context, key string, payload []byte, ttl time.Duration) error {
	env := envelope{
		StoredAt: time.Now().UTC(),
		Payload:  append([]byte(nil), payload...),
	}

	data, err := json.Marshal(env)
	if err != nil {
		return fmt.Errorf("encode cached payload %q: %w", key, err)
	}

	if err := s.client.Set(ctx, key, data, ttl).Err(); err != nil {
		return fmt.Errorf("redis set %q: %w", key, err)
	}

	return nil
}
