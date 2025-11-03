package proxy

import (
	"context"
	"errors"
	"io"
	"log/slog"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Forwarder streams the incoming request to an upstream target with minimal overhead.
type Forwarder struct {
	Client         *http.Client
	Logger         *slog.Logger
	RequestTimeout time.Duration
}

var hopHeaders = []string{
	"Connection",
	"Proxy-Connection",
	"Keep-Alive",
	"Proxy-Authenticate",
	"Proxy-Authorization",
	"Te",
	"Trailer",
	"Transfer-Encoding",
	"Upgrade",
}

// Do forwards the request to the target URL.
func (f *Forwarder) Do(w http.ResponseWriter, r *http.Request, target *url.URL) error {
	if f.Client == nil {
		return errors.New("forwarder client is nil")
	}

	f.Logger.Info("forwarding request", slog.String("method", r.Method), slog.String("url", r.URL.String()), slog.String("target", target.String()))

	ctx, cancel := context.WithTimeout(r.Context(), f.RequestTimeout)
	defer cancel()

	upstreamReq, err := cloneRequestWithURL(ctx, r, target)
	if err != nil {
		return err
	}

	reqResp, err := f.Client.Do(upstreamReq)
	if err != nil {
		return err
	}
	defer reqResp.Body.Close()

	copyHeaders(w.Header(), reqResp.Header)
	for _, h := range hopHeaders {
		w.Header().Del(h)
	}
	w.WriteHeader(reqResp.StatusCode)

	if reqResp.Body != nil {
		buf := make([]byte, 32*1024)
		if _, err := io.CopyBuffer(w, reqResp.Body, buf); err != nil {
			return err
		}
	}

	return nil
}

func cloneRequestWithURL(ctx context.Context, r *http.Request, target *url.URL) (*http.Request, error) {
	var body io.ReadCloser
	if r.Body != nil {
		body = r.Body
	}

	upstreamReq, err := http.NewRequestWithContext(ctx, r.Method, target.String(), body)
	if err != nil {
		return nil, err
	}

	copyHeaders(upstreamReq.Header, r.Header)
	for _, h := range hopHeaders {
		upstreamReq.Header.Del(h)
	}

	setForwardedHeaders(upstreamReq.Header, r)

	upstreamReq.ContentLength = r.ContentLength
	upstreamReq.TransferEncoding = r.TransferEncoding
	upstreamReq.Trailer = cloneHeader(r.Trailer)
	upstreamReq.Host = target.Host

	return upstreamReq, nil
}

func setForwardedHeaders(header http.Header, r *http.Request) {
	clientIP, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		clientIP = r.RemoteAddr
	}

	if clientIP != "" {
		prior := header.Get("X-Forwarded-For")
		if prior == "" {
			header.Set("X-Forwarded-For", clientIP)
		} else {
			header.Set("X-Forwarded-For", strings.Join([]string{prior, clientIP}, ", "))
		}
	}

	if proto := r.Header.Get("X-Forwarded-Proto"); proto == "" {
		header.Set("X-Forwarded-Proto", schemeFromRequest(r))
	}

	header.Set("X-Forwarded-Host", r.Host)
}

func schemeFromRequest(r *http.Request) string {
	if r.TLS != nil {
		return "https"
	}
	if scheme := r.Header.Get("X-Forwarded-Proto"); scheme != "" {
		return scheme
	}
	if r.URL.Scheme != "" {
		return r.URL.Scheme
	}
	return "http"
}

func copyHeaders(dst, src http.Header) {
	for k, vv := range src {
		for _, v := range vv {
			dst.Add(k, v)
		}
	}
}

func cloneHeader(src http.Header) http.Header {
	if len(src) == 0 {
		return nil
	}

	dst := make(http.Header, len(src))
	for k, vv := range src {
		cv := make([]string, len(vv))
		copy(cv, vv)
		dst[k] = cv
	}
	return dst
}
