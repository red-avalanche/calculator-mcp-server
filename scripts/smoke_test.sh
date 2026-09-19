#!/usr/bin/env bash
set -euo pipefail
BIN="${1:-./target/debug/calculator-mcp-server}"
echo "Running smoke test on $BIN"
out=$( (printf '%s\n' \
'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"smoke","version":"0.1"}}}' \
'{"jsonrpc":"2.0","method":"notifications/initialized"}' \
'{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
'{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"version","arguments":{}}}' \
'{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"calculate","arguments":{"expression":"2 * 3 + 4"}}}' \
'{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"plot_function","arguments":{"expression":"x**2"}}}' \
'{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"convert_unit","arguments":{"value":1,"from_unit":"mi","to_unit":"km"}}}' \
'{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"base_convert","arguments":{"value":"255","to_base":16}}}'; sleep 3) \
| "$BIN" 2>/dev/null)
grep -q '"protocolVersion"' <<<"$out"
grep -q '"version"' <<<"$out"
grep -q '1.1.0' <<<"$out"
grep -q '10' <<<"$out"
grep -q 'Plot generated' <<<"$out"
grep -q '1.609344' <<<"$out"
grep -q '0xff' <<<"$out"
echo "smoke test OK"