#!/usr/bin/env bash
# Sourced by check-attribution-honesty.sh after its fail helper is defined.
# UTM has no runtime, interface, or redistribution role in BridgeVM. Operator
# records are excluded because they may preserve local investigation paths.
utm_hits=$(
  git grep -niI -E '(^|[^[:alnum:]_])UTM([^[:alnum:]_]|$)' -- . 2>/dev/null |
    grep -vE '^(GOAL|PLAN|HANDOFF)\.md:|^scripts/check-attribution-product-names\.sh:' || true
)
if [[ -n "$utm_hits" ]]; then
  echo "$utm_hits" >&2
  fail "UTM is named outside operator-owned planning records"
fi
