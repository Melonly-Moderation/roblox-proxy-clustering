package cache

import (
	"context"
	"time"

	redis "github.com/redis/go-redis/v9"
)

const defaultRedisTimeout = 600 * time.Millisecond

// Redis implements Layer backed by a Redis instance.
type Redis struct {
	client *redis.Client
}

// NewRedis instantiates a Redis cache based on a connection URL.
func NewRedis(rawURL string) (*Redis, error) {
	opts, err := redis.ParseURL(rawURL)
	if err != nil {
		return nil, err
	}
	return &Redis{client: redis.NewClient(opts)}, nil
}

// Get fetches a cached payload or returns nil when missing.
func (r *Redis) Get(ctx context.Context, key string) ([]byte, error) {
	ctx, cancel := context.WithTimeout(ctx, defaultRedisTimeout)
	defer cancel()

	data, err := r.client.Get(ctx, key).Bytes()
	if err != nil {
		if err == redis.Nil {
			return nil, nil
		}
		return nil, err
	}
	return data, nil
}

// Set stores a payload using a TTL measured in seconds.
func (r *Redis) Set(ctx context.Context, key string, value []byte, ttlSeconds int) error {
	ctx, cancel := context.WithTimeout(ctx, defaultRedisTimeout)
	defer cancel()

	return r.client.Set(ctx, key, value, time.Duration(ttlSeconds)*time.Second).Err()
}

// Close releases the Redis client resources.
func (r *Redis) Close() error {
	return r.client.Close()
}
