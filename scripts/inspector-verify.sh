#!/usr/bin/env bash
# End-to-end tools/list verification with the official MCP Inspector CLI.
#
# The Rust tests validate tool *definitions* in-process. They cannot catch a
# regression that only appears to a real client over a real transport: a
# handshake that yields no usable session, a protocol era the server mishandles,
# or a payload a strict client rejects. This script closes that gap by driving
# the server with the same client an operator would use.
#
# Both protocol eras are exercised because they take different code paths in
# rmcp: `legacy` negotiates a session id and reuses it, while `modern`
# (2026-07-28) is sessionless and carries protocol metadata per request. A
# server can be perfectly healthy on one and broken on the other.
#
# Usage: scripts/inspector-verify.sh [url]
set -euo pipefail

URL="${1:-http://127.0.0.1:8757/mcp}"
INSPECTOR="${INSPECTOR_PKG:-@modelcontextprotocol/inspector}"
# A healthy tools/list answers in milliseconds; the Inspector's own default is
# 60s, which turns a hang into a very slow "failure" long after CI could react.
TIMEOUT_MS="${INSPECTOR_TIMEOUT_MS:-20000}"
# Sentinel tool that must always be present, so an empty or truncated list fails
# instead of silently passing.
REQUIRE_TOOL="${REQUIRE_TOOL:-list_users}"

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

fail=0

for era in legacy modern; do
  cat > "$workdir/$era.json" <<EOF
{
  "mcpServers": {
    "target": {
      "type": "http",
      "url": "$URL",
      "protocolEra": "$era"
    }
  }
}
EOF

  echo "==> tools/list (protocolEra: $era)"
  start=$(date +%s)
  if npx -y "$INSPECTOR" --cli \
      --config "$workdir/$era.json" --server target \
      --stored-auth-only \
      --connect-timeout "$TIMEOUT_MS" \
      --method tools/list --format json \
      > "$workdir/$era-out.json" 2> "$workdir/$era-err.json"; then
    elapsed=$(( $(date +%s) - start ))
    count=$(jq '.result.tools | length' "$workdir/$era-out.json")
    if ! jq -e --arg t "$REQUIRE_TOOL" \
        '.result.tools | map(.name) | index($t)' \
        "$workdir/$era-out.json" > /dev/null; then
      echo "    FAIL: $era listed $count tools but $REQUIRE_TOOL was missing"
      fail=1
      continue
    fi
    echo "    ok: $count tools in ${elapsed}s"
  else
    code=$?
    elapsed=$(( $(date +%s) - start ))
    # The CLI's exit codes name the failure class: 3 auth, 4 unreachable.
    case $code in
      3) reason="server requires authentication" ;;
      4) reason="server unreachable (or timed out)" ;;
      *) reason="tools/list failed" ;;
    esac
    echo "    FAIL: $era $reason (exit $code, ${elapsed}s)"
    tail -1 "$workdir/$era-err.json" || true
    fail=1
  fi
done

exit $fail
