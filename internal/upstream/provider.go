package upstream

import (
	"fmt"
	"net/url"
)

// ParseProviderTargets parses and validates provider upstream URLs.
func ParseProviderTargets(raw []string) ([]*url.URL, error) {
	if len(raw) == 0 {
		return nil, fmt.Errorf("no provider targets provided")
	}

	upstreams := make([]*url.URL, 0, len(raw))
	for _, v := range raw {
		u, err := url.Parse(v)
		if err != nil {
			return nil, fmt.Errorf("parse provider target %q: %w", v, err)
		}

		if u.Scheme != "http" && u.Scheme != "https" {
			return nil, fmt.Errorf("provider target %q must use http or https scheme", v)
		}

		upstreams = append(upstreams, u)
	}

	return upstreams, nil
}
