package main

import (
	"context"
	"log"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"roblox-proxy-clustering/internal/cache"
	"roblox-proxy-clustering/internal/config"
	"roblox-proxy-clustering/internal/httpclient"
	"roblox-proxy-clustering/internal/server"
	"roblox-proxy-clustering/internal/upstream"
)

func main() {
	cfg, err := config.Load()
	if err != nil {
		log.Fatalf("configuration error: %v", err)
	}

	logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: parseLogLevel()}))
	logger.Info("starting roblox proxy", slog.String("role", string(cfg.Role)), slog.Int("clusters", len(cfg.ClusterTargets)))

	pool := upstream.NewPool(cfg.ClusterTargets)
	client := httpclient.New(pool, cfg.RequestTimeout)

	cacheLayer := buildCacheLayer(logger, cfg.RedisURL)
	if closer, ok := cacheLayer.(interface{ Close() error }); ok {
		defer closer.Close()
	}

	srv := server.New(cfg, client, cacheLayer, logger)
	httpServer := &http.Server{
		Addr:         cfg.ListenAddr,
		Handler:      srv.Handler(),
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 15 * time.Second,
		IdleTimeout:  60 * time.Second,
	}

	errCh := make(chan error, 1)
	go func() {
		logger.Info("http server listening", slog.String("addr", cfg.ListenAddr))
		errCh <- httpServer.ListenAndServe()
	}()

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	select {
	case err := <-errCh:
		if err != nil && err != http.ErrServerClosed {
			logger.Error("server terminated unexpectedly", slog.Any("err", err))
		}
	case sig := <-sigCh:
		logger.Info("shutdown signal received", slog.String("signal", sig.String()))
	}

	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()

	if err := httpServer.Shutdown(ctx); err != nil {
		logger.Error("graceful shutdown failed", slog.Any("err", err))
	} else {
		logger.Info("shutdown complete")
	}
}

func parseLogLevel() slog.Level {
	switch os.Getenv("PROXY_LOG_LEVEL") {
	case "debug", "DEBUG":
		return slog.LevelDebug
	case "warn", "WARNING", "WARN":
		return slog.LevelWarn
	case "error", "ERROR":
		return slog.LevelError
	default:
		return slog.LevelInfo
	}
}

func buildCacheLayer(logger *slog.Logger, redisURL string) cache.Layer {
	if redisURL == "" {
		logger.Info("cache layer", slog.String("type", "noop"))
		return cache.Noop{}
	}

	r, err := cache.NewRedis(redisURL)
	if err != nil {
		logger.Error("failed to initialize redis cache, falling back to noop", slog.Any("err", err))
		return cache.Noop{}
	}

	logger.Info("cache layer", slog.String("type", "redis"))
	return r
}
