#!/usr/bin/env bash
# Sourced by check-attribution-honesty.sh after its fail helper is defined.
# UTM has no runtime, interface, or redistribution role in BridgeVM. Operator
# records and the user-requested process comparison are the only exceptions.
utm_hits=$(
  git grep -niI -E '(^|[^[:alnum:]_])UTM([^[:alnum:]_]|$)' -- . 2>/dev/null |
    grep -vE '^(GOAL|PLAN|HANDOFF)\.md:|^scripts/check-attribution-product-names\.sh:|^docs/reference/upstream-development-practices\.md:' || true
)
if [[ -n "$utm_hits" ]]; then
  echo "$utm_hits" >&2
  fail "UTM is named outside operator records or the explicit process comparison"
fi
