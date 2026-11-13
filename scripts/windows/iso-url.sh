#!/usr/bin/env bash
# Prints "<sha256> <url>" for the newest English (US) Windows 11 Arm64 ISO, for windows-vm.sh: the
# link Microsoft's download page hands a browser, good for 24 hours, and the SHA-256 the page lists
# for it. The page asks its download service for the link only after a session handshake with
# ov-df.microsoft.com (the page's mdt.js), which this does the same way; the instance and profile
# ids are the ones in the page's site.*.min.js. Microsoft refuses links to VPN and datacenter
# addresses (error 715-123130): this then exits saying so.
set -euo pipefail
page=https://www.microsoft.com/en-us/software-download/windows11arm64
ua="Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:133.0) Gecko/20100101 Firefox/133.0"
instance=560dc9f3-1aa5-4a2f-b63c-9e18f8d0e175
profile=606624d44113
sid=$(uuidgen | tr '[:upper:]' '[:lower:]')
get() { curl -fsS --proto '=https' --tlsv1.2 -A "$ua" "$@"; }

html=$(get -H 'Accept:' "$page")
# The page's one product: the multi-edition ISO (3816 in 2026).
edition=$(grep -Eo '<option value="[0-9]+">Windows' <<<"$html" | head -1 | tr -cd 0-9)
sha=$(python3 -c 'import re,sys; print(dict(re.findall(r"<td>([^<]+?)\s*</td>\s*<td>([A-F0-9]{64})</td>", sys.stdin.read()))["English  64-bit"])' <<<"$html")

# The session: the vlscppe tag the page loads (not needed in 2026, but cheap) and the ov-df
# handshake, without which the link request is refused.
get -o /dev/null "https://vlscppe.microsoft.com/tags?org_id=y6jn8c31&session_id=$sid"
js=$(get "https://ov-df.microsoft.com/mdt.js?instanceId=$instance&PageId=si&session_id=$sid")
w=$(sed -nE 's/.*[?&]w=([A-F0-9]+).*/\1/p' <<<"$js")
rticks=$(sed -nE 's/.*rticks="\+?([0-9]+).*/\1/p' <<<"$js")
get -o /dev/null "https://ov-df.microsoft.com/?session_id=$sid&CustomerId=$instance&PageId=si&w=$w&mdt=$(date +%s)000&rticks=$rticks"

api=https://www.microsoft.com/software-download-connector/api
sku=$(get "$api/getskuinformationbyproductedition?profile=$profile&ProductEditionId=$edition&SKU=undefined&friendlyFileName=undefined&Locale=en-US&sessionID=$sid" |
  python3 -c 'import json,sys; print(next(s["Id"] for s in json.load(sys.stdin)["Skus"] if s["Language"]=="English"))')
url=$(get -H "Referer: $page" "$api/GetProductDownloadLinksBySku?profile=$profile&productEditionId=undefined&SKU=$sku&friendlyFileName=undefined&Locale=en-US&sessionID=$sid" |
  python3 -c '
import json, sys
d = json.load(sys.stdin)
if d.get("Errors"):
    sys.exit("Microsoft refused the download (%s); it refuses VPN and datacenter addresses" % d["Errors"])
print(d["ProductDownloadOptions"][0]["Uri"])')
echo "$sha $url"
