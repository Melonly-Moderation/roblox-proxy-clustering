package util

import (
	"hash/fnv"
)

// ConsistentIndex computes a stable shard index for the provided string.
func ConsistentIndex(key string, buckets int) int {
	if buckets <= 0 {
		return 0
	}

	h := fnv.New32a()
	_, _ = h.Write([]byte(key))
	return int(h.Sum32() % uint32(buckets))
}
