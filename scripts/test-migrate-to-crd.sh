#!/usr/bin/env bash
set -euo pipefail

# Fixture test for migrate-to-crd.sh. Pipes a synthetic REST response (one
# record per supported type) through the migrator and checks the rendered
# YAML for the type-specific fields each arm of the case statement should
# emit. If a record type ever drops out of the case statement (as SOA/SRV
# silently did before), this test fails loudly.
#
# Run:  ./scripts/test-migrate-to-crd.sh
#
# CI invokes it via `bash scripts/test-migrate-to-crd.sh`.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MIGRATE="$SCRIPT_DIR/migrate-to-crd.sh"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'
fail() { echo -e "${RED}[fail]${NC} $*" >&2; exit 1; }
pass() { echo -e "${GREEN}[pass]${NC} $*"; }

# Synthetic REST response covering all 9 record types. Field names mirror the
# real REST schema so we'd notice if the migrator started reading the wrong
# JSON paths.
read -r -d '' RECORDS <<'EOF' || true
{
  "data": {
    "total": 9,
    "records": [
      {"id": "id-a",     "name": "a.test",      "ttl": 300,  "class": "IN", "type": "A",     "ip": "1.2.3.4"},
      {"id": "id-aaaa",  "name": "aaaa.test",   "ttl": 300,  "class": "IN", "type": "AAAA",  "ip": "::1"},
      {"id": "id-cname", "name": "cn.test",     "ttl": 300,  "class": "IN", "type": "CNAME", "target": "x.test"},
      {"id": "id-ptr",   "name": "p.test",      "ttl": 300,  "class": "IN", "type": "PTR",   "target": "host.test"},
      {"id": "id-ns",    "name": "z.test",      "ttl": 300,  "class": "IN", "type": "NS",    "target": "ns1.test"},
      {"id": "id-mx",    "name": "m.test",      "ttl": 300,  "class": "IN", "type": "MX",    "preference": 10, "exchange": "mail.test"},
      {"id": "id-txt",   "name": "t.test",      "ttl": 300,  "class": "IN", "type": "TXT",   "strings": ["v=spf1 -all"]},
      {"id": "id-soa",   "name": "z.test",      "ttl": 3600, "class": "IN", "type": "SOA",   "mname": "ns1.test", "rname": "hostmaster.test", "serial": 1, "refresh": 7200, "retry": 3600, "expire": 1209600, "minimum": 300},
      {"id": "id-srv",   "name": "_sip._tcp.test", "ttl": 300, "class": "IN", "type": "SRV", "priority": 10, "weight": 5, "port": 5060, "target": "sipsrv.test"}
    ]
  }
}
EOF

OUTPUT=$(RIND_MIGRATE_RECORDS_JSON="$RECORDS" RIND_NAMESPACE=rind-system bash "$MIGRATE" http://unused 2>/dev/null) \
    || fail "migrator exited non-zero"

assert_in() {
    local needle="$1" desc="$2"
    if grep -qF -- "$needle" <<<"$OUTPUT"; then
        pass "$desc"
    else
        echo "--- output ---" >&2
        echo "$OUTPUT" >&2
        echo "--------------" >&2
        fail "$desc — missing: $needle"
    fi
}

# Headers
assert_in 'apiVersion: dns.rind.dev/v1alpha1' 'apiVersion present'
assert_in 'kind: DnsRecord'                   'kind present'

# Per-type field assertions — each one would silently disappear if its case
# arm regressed, exactly the bug we just fixed.
assert_in 'ip: "1.2.3.4"'                                'A: ip rendered'
assert_in 'ip: "::1"'                                    'AAAA: ip rendered'
assert_in 'target: "x.test"'                             'CNAME: target rendered'
assert_in 'target: "host.test"'                          'PTR: target rendered'
assert_in 'target: "ns1.test"'                           'NS: target rendered'
assert_in 'preference: 10'                               'MX: preference rendered'
assert_in 'exchange: "mail.test"'                        'MX: exchange rendered'
assert_in 'v=spf1 -all'                                  'TXT: string rendered'
assert_in 'mname: "ns1.test"'                            'SOA: mname rendered'
assert_in 'rname: "hostmaster.test"'                     'SOA: rname rendered'
assert_in 'serial: 1'                                    'SOA: serial rendered'
assert_in 'minimum: 300'                                 'SOA: minimum rendered'
assert_in 'priority: 10'                                 'SRV: priority rendered'
assert_in 'port: 5060'                                   'SRV: port rendered'
assert_in 'target: "sipsrv.test"'                        'SRV: target rendered'

# Header repeats once per record (9 records, 9 docs).
docs=$(grep -c '^apiVersion:' <<<"$OUTPUT")
[ "$docs" = "9" ] || fail "expected 9 generated docs, got $docs"
pass "9 documents emitted (one per record)"

echo
echo -e "${GREEN}migrate-to-crd.sh: all assertions passed${NC}"
