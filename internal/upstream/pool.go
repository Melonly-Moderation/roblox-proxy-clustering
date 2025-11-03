package upstream

import (
	"net/url"
	"sync/atomic"
)

// Target represents a single upstream cluster endpoint.
type Target struct {
	base *url.URL
}

// URL returns a cloned url.URL for safe mutation by callers.
func (t *Target) URL() *url.URL {
	clone := *t.base
	return &clone
}

// Resolve returns a fully-qualified URL assembled from the upstream base, path, and query string.
func (t *Target) Resolve(path, rawQuery string) *url.URL {
	u := t.URL()
	u.Path = joinURLPath(u.Path, path)
	u.RawQuery = rawQuery
	return u
}

// Pool implements a lock-free round-robin selector over upstream targets.
type Pool struct {
	targets []*Target
	cursor  atomic.Uint64
}

// NewPool constructs a pool from the provided URLs.
func NewPool(urls []*url.URL) *Pool {
	targets := make([]*Target, len(urls))
	for i, u := range urls {
		clone := *u
		targets[i] = &Target{base: &clone}
	}
	return &Pool{targets: targets}
}

// Next returns the next target in a round-robin fashion.
func (p *Pool) Next() *Target {
	idx := int(p.cursor.Add(1)-1) % len(p.targets)
	return p.targets[idx]
}

// Len reports how many upstream targets are available.
func (p *Pool) Len() int {
	return len(p.targets)
}

func joinURLPath(basePath, reqPath string) string {
	switch {
	case basePath == "":
		if reqPath == "" {
			return "/"
		}
		return ensureLeadingSlash(reqPath)
	case reqPath == "":
		return ensureLeadingSlash(basePath)
	default:
		b := ensureLeadingSlash(basePath)
		r := ensureLeadingSlash(reqPath)
		if b[len(b)-1] == '/' {
			return b + r[1:]
		}
		return b + r
	}
}

func ensureLeadingSlash(path string) string {
	if path == "" {
		return "/"
	}
	if path[0] != '/' {
		return "/" + path
	}
	return path
}
