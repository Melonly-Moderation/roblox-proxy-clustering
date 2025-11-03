package member

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"net/url"
	"strings"
	"time"

	"golang.org/x/sync/singleflight"

	"github.com/NoahCxrest/roblox-proxy-clustering/internal/cache"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/config"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/proxy"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/upstream"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/util"
)

const (
	corsAllowOrigin                = "*"
	headerAccessControlAllowOrigin = "Access-Control-Allow-Origin"
	headerContentType              = "Content-Type"
	contentTypeJSON                = "application/json"
	userAgent                      = "RobloxProxyCluster/1.0"
)

var (
	errBadPath          = errors.New("unable to determine Roblox upstream from path")
	errNoUpstreamTarget = errors.New("no upstream target available")
)

// Handler routes member traffic either to cached endpoints or Roblox directly.
type Handler struct {
	cfg       config.Config
	logger    *slog.Logger
	cache     cache.Store
	forwarder *proxy.Forwarder
	targets   []upstream.MemberTarget
	sgroup    singleflight.Group
}

// New constructs a member handler.
func New(cfg config.Config, logger *slog.Logger, cacheStore cache.Store, client *http.Client) (*Handler, error) {
	targets, err := upstream.ParseMemberTargets(cfg.MemberClusters)
	if err != nil {
		return nil, err
	}

	return &Handler{
		cfg:    cfg,
		logger: logger.With(slog.String("component", "member-handler")),
		cache:  cacheStore,
		forwarder: &proxy.Forwarder{
			Client:         client,
			Logger:         logger,
			RequestTimeout: cfg.RequestTimeout,
		},
		targets: targets,
	}, nil
}

// ServeHTTP implements http.Handler.
func (h *Handler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()

	if userID := strings.TrimSpace(q.Get("userId")); userID != "" {
		h.handleUserLookup(w, r, userID)
		return
	}

	if search := strings.TrimSpace(q.Get("search")); search != "" {
		h.handleSearch(w, r, search)
		return
	}

	h.handleProxy(w, r)
}

func (h *Handler) handleProxy(w http.ResponseWriter, r *http.Request) {
	target, err := h.pickTargetURL(r)
	if err != nil {
		h.respondError(w, http.StatusBadGateway, err)
		return
	}

	if err := h.forwarder.Do(w, r, target); err != nil {
		h.logger.Error("proxy request failed", slog.String("path", r.URL.Path), slog.String("error", err.Error()))
		h.respondError(w, http.StatusBadGateway, err)
	}
}

func (h *Handler) handleUserLookup(w http.ResponseWriter, r *http.Request, userID string) {
	if !isNumeric(userID) {
		h.respondJSON(w, http.StatusBadRequest, []byte(`{"error":"Invalid or missing userId"}`))
		return
	}

	ctx, cancel := context.WithTimeout(r.Context(), h.cfg.RequestTimeout)
	defer cancel()

	payload, err := h.readThroughCache(ctx, h.userCacheKey(userID), func(ctx context.Context) ([]byte, error) {
		return h.fetchUserPayload(ctx, userID)
	})
	if err != nil {
		h.logger.Error("user lookup failed", slog.String("userId", userID), slog.String("error", err.Error()))
		h.respondError(w, http.StatusInternalServerError, err)
		return
	}

	h.respondCachedJSON(w, payload)
}

func (h *Handler) handleSearch(w http.ResponseWriter, r *http.Request, search string) {
	needle := strings.TrimSpace(search)
	if len(needle) < 3 {
		h.respondJSON(w, http.StatusBadRequest, []byte(`[]`))
		return
	}

	ctx, cancel := context.WithTimeout(r.Context(), h.cfg.RequestTimeout)
	defer cancel()

	key := h.searchCacheKey(strings.ToLower(needle))
	payload, err := h.readThroughCache(ctx, key, func(ctx context.Context) ([]byte, error) {
		return h.fetchSearchPayload(ctx, needle)
	})
	if err != nil {
		h.logger.Error("search failed", slog.String("query", needle), slog.String("error", err.Error()))
		h.respondError(w, http.StatusInternalServerError, err)
		return
	}

	h.respondCachedJSON(w, payload)
}

func (h *Handler) pickTargetURL(r *http.Request) (*url.URL, error) {
	return h.chooseTarget(r.URL.Path, r.URL.RawQuery)
}

func (h *Handler) chooseTarget(path, rawQuery string) (*url.URL, error) {
	if len(h.targets) == 0 {
		return nil, errNoUpstreamTarget
	}

	key := path
	if rawQuery != "" {
		key += "?" + rawQuery
	}

	idx := util.ConsistentIndex(key, len(h.targets))
	target := h.targets[idx]

	switch target.Kind {
	case upstream.MemberTargetDirect:
		host, rewritten, err := resolveRobloxTarget(path)
		if err != nil {
			return nil, err
		}
		return &url.URL{
			Scheme:   "https",
			Host:     host,
			Path:     rewritten,
			RawQuery: rawQuery,
		}, nil
	case upstream.MemberTargetStatic:
		rel := &url.URL{Path: path, RawQuery: rawQuery}
		return target.Base.ResolveReference(rel), nil
	default:
		return nil, errNoUpstreamTarget
	}
}

func (h *Handler) fetchUserPayload(ctx context.Context, userID string) ([]byte, error) {
	var userResp struct {
		Description string `json:"description"`
		Created     string `json:"created"`
		IsBanned    bool   `json:"isBanned"`
		ID          int64  `json:"id"`
		Name        string `json:"name"`
		DisplayName string `json:"displayName"`
	}

	if err := h.fetchJSON(ctx, "users", "/v1/users/"+userID, nil, &userResp); err != nil {
		return nil, err
	}

	params := url.Values{
		"userIds":    {userID},
		"size":       {"48x48"},
		"format":     {"Png"},
		"isCircular": {"false"},
	}

	var avatarResp struct {
		Data []struct {
			ImageURL string `json:"imageUrl"`
		} `json:"data"`
	}

	if err := h.fetchJSON(ctx, "thumbnails", "/v1/users/avatar-bust", params, &avatarResp); err != nil {
		return nil, err
	}

	combined := struct {
		Description string `json:"description"`
		Created     string `json:"created"`
		IsBanned    bool   `json:"isBanned"`
		ID          int64  `json:"id"`
		Name        string `json:"name"`
		DisplayName string `json:"displayName"`
		AvatarURL   string `json:"avatarUrl"`
	}{
		Description: userResp.Description,
		Created:     userResp.Created,
		IsBanned:    userResp.IsBanned,
		ID:          userResp.ID,
		Name:        userResp.Name,
		DisplayName: userResp.DisplayName,
		AvatarURL:   firstAvatarURL(avatarResp.Data),
	}

	return json.Marshal(combined)
}

func (h *Handler) fetchSearchPayload(ctx context.Context, query string) ([]byte, error) {
	params := url.Values{
		"verticalType":    {"user"},
		"searchQuery":     {query},
		"globalSessionId": {"TridentBot"},
		"sessionId":       {"TridentBot"},
	}

	var searchResp struct {
		SearchResults []struct {
			Contents []struct {
				ContentID int64  `json:"contentId"`
				Username  string `json:"username"`
			} `json:"contents"`
		} `json:"searchResults"`
	}

	if err := h.fetchJSON(ctx, "apis", "/search-api/omni-search", params, &searchResp); err != nil {
		return nil, err
	}

	results := searchResp.SearchResults
	if len(results) == 0 || len(results[0].Contents) == 0 {
		return json.Marshal([]any{})
	}

	contents := results[0].Contents
	final := make([]struct {
		PlayerID  string `json:"playerId"`
		Name      string `json:"name"`
		AvatarURL string `json:"avatarUrl"`
	}, len(contents))

	for i, entry := range contents {
		userID := fmt.Sprintf("%d", entry.ContentID)
		avatar, err := h.lookupAvatarURL(ctx, userID)
		if err != nil {
			h.logger.Warn("avatar lookup failed", slog.String("userId", userID), slog.String("error", err.Error()))
		}
		final[i] = struct {
			PlayerID  string `json:"playerId"`
			Name      string `json:"name"`
			AvatarURL string `json:"avatarUrl"`
		}{
			PlayerID:  userID,
			Name:      entry.Username,
			AvatarURL: avatar,
		}
	}

	return json.Marshal(final)
}

func (h *Handler) lookupAvatarURL(ctx context.Context, userID string) (string, error) {
	key := h.avatarCacheKey(userID)
	payload, err := h.readThroughCache(ctx, key, func(ctx context.Context) ([]byte, error) {
		return h.fetchAvatarPayload(ctx, userID)
	})
	if err != nil {
		return "", err
	}

	var body struct {
		URL string `json:"url"`
	}

	if err := json.Unmarshal(payload, &body); err != nil {
		return "", err
	}

	return body.URL, nil
}

func (h *Handler) fetchAvatarPayload(ctx context.Context, userID string) ([]byte, error) {
	params := url.Values{
		"userIds":    {userID},
		"size":       {"420x420"},
		"format":     {"Png"},
		"isCircular": {"false"},
	}

	var avatarResp struct {
		Data []struct {
			ImageURL string `json:"imageUrl"`
		} `json:"data"`
	}

	if err := h.fetchJSON(ctx, "thumbnails", "/v1/users/avatar-bust", params, &avatarResp); err != nil {
		return nil, err
	}

	payload := struct {
		URL string `json:"url"`
	}{URL: firstAvatarURL(avatarResp.Data)}

	return json.Marshal(payload)
}

func (h *Handler) fetchJSON(ctx context.Context, service, path string, params url.Values, dest any) error {
	service = strings.Trim(service, "/")
	basePath := "/" + service
	if path != "" {
		basePath = strings.TrimRight(basePath, "/") + "/" + strings.TrimLeft(path, "/")
	}

	rawQuery := ""
	if params != nil {
		rawQuery = params.Encode()
	}

	target, err := h.chooseTarget(basePath, rawQuery)
	if err != nil {
		return err
	}

	h.logger.Info("fetching JSON", slog.String("service", service), slog.String("path", basePath), slog.String("query", rawQuery), slog.String("target", target.String()))

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, target.String(), nil)
	if err != nil {
		return err
	}

	req.Header.Set("User-Agent", userAgent)
	req.Header.Set("Accept", contentTypeJSON)

	resp, err := h.forwarder.Client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("roblox request failed: %s", resp.Status)
	}

	return json.NewDecoder(resp.Body).Decode(dest)
}

func (h *Handler) readThroughCache(ctx context.Context, key string, fetch func(context.Context) ([]byte, error)) ([]byte, error) {
	if entry, ok, err := h.cache.Get(ctx, key); err != nil {
		return nil, err
	} else if ok {
		age := time.Since(entry.StoredAt)
		if age > h.cfg.BackgroundRefreshAfter {
			h.launchRefresh(key, fetch)
		}
		return entry.Payload, nil
	}

	res, err, _ := h.sgroup.Do(key, func() (any, error) {
		payload, err := fetch(ctx)
		if err != nil {
			return nil, err
		}
		if err := h.storeWithTTL(key, payload); err != nil {
			h.logger.Warn("cache store failed", slog.String("key", key), slog.String("error", err.Error()))
		}
		return payload, nil
	})
	if err != nil {
		return nil, err
	}

	return res.([]byte), nil
}

func (h *Handler) launchRefresh(key string, fetch func(context.Context) ([]byte, error)) {
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), h.cfg.RequestTimeout)
		defer cancel()

		res, err, _ := h.sgroup.Do(key+":refresh", func() (any, error) {
			payload, err := fetch(ctx)
			if err != nil {
				return nil, err
			}
			if err := h.storeWithTTL(key, payload); err != nil {
				h.logger.Warn("refresh cache store failed", slog.String("key", key), slog.String("error", err.Error()))
			}
			return payload, nil
		})

		if err != nil {
			h.logger.Debug("background refresh failed", slog.String("key", key), slog.String("error", err.Error()))
			return
		}

		_ = res
	}()
}

func (h *Handler) storeWithTTL(key string, payload []byte) error {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	return h.cache.Set(ctx, key, payload, h.cfg.CacheTTL)
}

func (h *Handler) respondCachedJSON(w http.ResponseWriter, payload []byte) {
	w.Header().Set(headerContentType, contentTypeJSON)
	w.Header().Set(headerAccessControlAllowOrigin, corsAllowOrigin)
	w.Header().Set("Cache-Control", "max-age=18000")
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(payload)
}

func (h *Handler) respondJSON(w http.ResponseWriter, status int, payload []byte) {
	w.Header().Set(headerContentType, contentTypeJSON)
	w.Header().Set(headerAccessControlAllowOrigin, corsAllowOrigin)
	w.WriteHeader(status)
	_, _ = w.Write(payload)
}

func (h *Handler) respondError(w http.ResponseWriter, status int, err error) {
	msg := fmt.Sprintf(`{"error":"%s"}`, sanitizeError(err))
	h.respondJSON(w, status, []byte(msg))
}

func (h *Handler) userCacheKey(userID string) string {
	return "roblox:user:" + userID
}

func (h *Handler) searchCacheKey(query string) string {
	return "roblox:search:" + query
}

func (h *Handler) avatarCacheKey(userID string) string {
	return "roblox:avatar:" + userID
}

func sanitizeError(err error) string {
	if err == nil {
		return ""
	}
	return strings.ReplaceAll(err.Error(), "\"", "'")
}

func isNumeric(v string) bool {
	if v == "" {
		return false
	}
	for _, ch := range v {
		if ch < '0' || ch > '9' {
			return false
		}
	}
	return true
}

func firstAvatarURL(data []struct {
	ImageURL string `json:"imageUrl"`
}) string {
	if len(data) == 0 {
		return ""
	}
	return data[0].ImageURL
}

func resolveRobloxTarget(path string) (host string, rewrittenPath string, err error) {
	segments := strings.Split(path, "/")
	if len(segments) < 2 || segments[1] == "" {
		return "", "", errBadPath
	}

	domain := segments[1]
	remaining := strings.Join(segments[2:], "/")
	if remaining == "" {
		remaining = "/"
	} else {
		remaining = "/" + remaining
	}

	return domain + ".roblox.com", remaining, nil
}
