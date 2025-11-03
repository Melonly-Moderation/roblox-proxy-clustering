package upstream

import (
	"fmt"
	"net/url"
	"strings"
)

// MemberTargetKind represents the strategy used by a member node to contact Roblox.
type MemberTargetKind int

const (
	MemberTargetUnknown MemberTargetKind = iota
	MemberTargetDirect
	MemberTargetStatic
)

// MemberTarget represents an upstream endpoint a member node can use.
type MemberTarget struct {
	Kind MemberTargetKind
	Base *url.URL
}

// ParseMemberTargets converts raw strings into structured member targets.
func ParseMemberTargets(raw []string) ([]MemberTarget, error) {
	if len(raw) == 0 {
		return nil, fmt.Errorf("no member targets provided")
	}

	targets := make([]MemberTarget, 0, len(raw))
	for _, v := range raw {
		if strings.EqualFold(v, "direct://") {
			targets = append(targets, MemberTarget{Kind: MemberTargetDirect})
			continue
		}

		u, err := url.Parse(v)
		if err != nil {
			return nil, fmt.Errorf("parse member target %q: %w", v, err)
		}

		if u.Scheme != "http" && u.Scheme != "https" {
			return nil, fmt.Errorf("member target %q must use http or https scheme", v)
		}

		// Normalize to ensure trailing slash removed for stable path joins.
		u.Path = strings.TrimRight(u.Path, "/")

		targets = append(targets, MemberTarget{Kind: MemberTargetStatic, Base: u})
	}

	return targets, nil
}
