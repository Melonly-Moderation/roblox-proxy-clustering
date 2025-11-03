package server

import (
	"fmt"
	"log/slog"
	"net/http"

	"github.com/NoahCxrest/roblox-proxy-clustering/internal/cache"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/config"
	memberhandler "github.com/NoahCxrest/roblox-proxy-clustering/internal/server/member"
	providerhandler "github.com/NoahCxrest/roblox-proxy-clustering/internal/server/provider"
)

// NewHandler constructs the appropriate HTTP handler based on the configured role.
func NewHandler(cfg config.Config, logger *slog.Logger, cacheStore cache.Store, client *http.Client) (http.Handler, error) {
	switch cfg.Role {
	case config.RoleMember:
		return memberhandler.New(cfg, logger, cacheStore, client)
	case config.RoleProvider:
		return providerhandler.New(cfg, logger, client)
	default:
		return nil, fmt.Errorf("unsupported role %q", cfg.Role)
	}
}
