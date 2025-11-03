package app

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"time"

	"github.com/NoahCxrest/roblox-proxy-clustering/internal/cache"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/cache/redisstore"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/config"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/server"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/transport"
)

// App wires configuration, dependencies, and the HTTP server together.
type App struct {
	cfg       config.Config
	logger    *slog.Logger
	cache     cache.Store
	stopCache func() error
	httpSrv   *http.Server
}

// New creates a fully initialised application.
func New(cfg config.Config) (*App, error) {
	logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

	redisStore, err := redisstore.New(cfg.RedisURL)
	if err != nil {
		return nil, fmt.Errorf("setup redis: %w", err)
	}

	httpClient := transport.NewHTTPClient(cfg)

	handler, err := server.NewHandler(cfg, logger, redisStore, httpClient)
	if err != nil {
		return nil, fmt.Errorf("build handler: %w", err)
	}

	httpSrv := &http.Server{
		Addr:              cfg.ListenAddr,
		Handler:           instrumentHandler(handler, logger, cfg.Role),
		ReadHeaderTimeout: 5 * time.Second,
		ReadTimeout:       cfg.RequestTimeout + cfg.TransportTimeout,
		WriteTimeout:      cfg.TransportTimeout + cfg.RequestTimeout,
		IdleTimeout:       cfg.IdleConnTimeout,
	}

	return &App{
		cfg:       cfg,
		logger:    logger,
		cache:     redisStore,
		stopCache: redisStore.Close,
		httpSrv:   httpSrv,
	}, nil
}

// Run blocks until the server shuts down or the context is cancelled.
func (a *App) Run(ctx context.Context) error {
	errCh := make(chan error, 1)
	defer func() {
		if a.stopCache != nil {
			if err := a.stopCache(); err != nil {
				a.logger.Warn("cache close failed", slog.String("error", err.Error()))
			}
		}
	}()

	go func() {
		a.logger.Info("proxy server starting", slog.String("addr", a.cfg.ListenAddr), slog.String("role", string(a.cfg.Role)))
		err := a.httpSrv.ListenAndServe()
		if err != nil && !errors.Is(err, http.ErrServerClosed) {
			errCh <- err
		} else {
			errCh <- nil
		}
	}()

	select {
	case <-ctx.Done():
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		return a.httpSrv.Shutdown(shutdownCtx)
	case err := <-errCh:
		return err
	}
}

func instrumentHandler(next http.Handler, logger *slog.Logger, role config.Role) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		w.Header().Set("X-Proxy-Role", string(role))
		next.ServeHTTP(w, r)
		dur := time.Since(start)
		logger.Debug("handled request",
			slog.String("method", r.Method),
			slog.String("path", r.URL.Path),
			slog.String("remote", r.RemoteAddr),
			slog.Duration("duration", dur),
			slog.String("role", string(role)))
	})
}
