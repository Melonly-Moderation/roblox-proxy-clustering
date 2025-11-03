package transport

import (
	"crypto/tls"
	"net"
	"net/http"
	"time"

	"github.com/NoahCxrest/roblox-proxy-clustering/internal/config"
)

// NewHTTPClient constructs an http.Client tuned for low-latency proxying.
func NewHTTPClient(cfg config.Config) *http.Client {
	transport := &http.Transport{
		Proxy:                 http.ProxyFromEnvironment,
		DialContext:           (&net.Dialer{Timeout: cfg.DialTimeout, KeepAlive: 60 * time.Second}).DialContext,
		TLSHandshakeTimeout:   cfg.DialTimeout,
		MaxIdleConns:          cfg.MaxIdleConns,
		MaxIdleConnsPerHost:   cfg.MaxIdleConnsPerHost,
		IdleConnTimeout:       cfg.IdleConnTimeout,
		ForceAttemptHTTP2:     true,
		ExpectContinueTimeout: 150 * time.Millisecond,
		TLSClientConfig: &tls.Config{
			MinVersion:         tls.VersionTLS12,
			ClientSessionCache: tls.NewLRUClientSessionCache(512),
		},
	}

	return &http.Client{
		Transport: transport,
		Timeout:   cfg.TransportTimeout,
	}
}
