#!/usr/bin/env bash
set -euo pipefail

# Migrate existing RIND records (from standalone LMDB) to Kubernetes CRDs.
#
# Reads all records from the REST API and generates DnsRecord CRD manifests.
#
# Usage:
#   ./scripts/migrate-to-crd.sh                          # defaults to localhost:8080
#   ./scripts/migrate-to-crd.sh http://10.0.1.5:8080     # custom API endpoint
#   ./scripts/migrate-to-crd.sh --apply                  # generate and apply directly
#
# Output: writes YAML to stdout (pipe to file or kubectl apply -f -)

API_BASE="${1:-http://localhost:8080}"
APPLY=false
NAMESPACE="${RIND_NAMESPACE:-rind-system}"

if [[ "${1:-}" == "--apply" ]]; then
    API_BASE="${2:-http://localhost:8080}"
    APPLY=true
elif [[ "${2:-}" == "--apply" ]]; then
    APPLY=true
fi

# Strip trailing slash
API_BASE="${API_BASE%/}"

log() { echo "# $*" >&2; }

# Fetch all records from the REST API. Tests can bypass the network by
# setting `RIND_MIGRATE_RECORDS_JSON` to the API response body directly.
if [ -n "${RIND_MIGRATE_RECORDS_JSON:-}" ]; then
    RECORDS="$RIND_MIGRATE_RECORDS_JSON"
else
    log "Fetching records from $API_BASE/records..."
    RECORDS=$(curl -sf "$API_BASE/records?per_page=10000" || { echo "Failed to reach API at $API_BASE" >&2; exit 1; })
fi

# Check if we got valid data
TOTAL=$(echo "$RECORDS" | jq -r '.data.total // 0')
if [ "$TOTAL" -eq 0 ]; then
    log "No records found. Nothing to migrate."
    exit 0
fi

log "Found $TOTAL records to migrate."

# Generate CRD YAML for each record
generate_crd() {
    local record="$1"
    local id name ttl class record_type

    id=$(echo "$record" | jq -r '.id')
    name=$(echo "$record" | jq -r '.name')
    ttl=$(echo "$record" | jq -r '.ttl')
    class=$(echo "$record" | jq -r '.class')
    record_type=$(echo "$record" | jq -r '.type')

    cat <<EOF
apiVersion: dns.rind.dev/v1alpha1
kind: DnsRecord
metadata:
  name: "${id}"
  namespace: ${NAMESPACE}
  labels:
    dns.rind.dev/record-name: "${name}"
    dns.rind.dev/record-type: "${record_type}"
    dns.rind.dev/migrated: "true"
spec:
  name: "${name}"
  ttl: ${ttl}
  class: "${class}"
  recordData:
    type: "${record_type}"
EOF

    # Add type-specific fields
    case "$record_type" in
        A|AAAA)
            local ip
            ip=$(echo "$record" | jq -r '.ip')
            echo "    ip: \"${ip}\""
            ;;
        CNAME|PTR|NS)
            local target
            target=$(echo "$record" | jq -r '.target')
            echo "    target: \"${target}\""
            ;;
        MX)
            local preference exchange
            preference=$(echo "$record" | jq -r '.preference')
            exchange=$(echo "$record" | jq -r '.exchange')
            echo "    preference: ${preference}"
            echo "    exchange: \"${exchange}\""
            ;;
        TXT)
            echo "    strings:"
            echo "$record" | jq -r '.strings[]' | while read -r s; do
                echo "      - \"${s}\""
            done
            ;;
        SOA)
            local mname rname serial refresh retry expire min_ttl
            mname=$(echo "$record"   | jq -r '.mname')
            rname=$(echo "$record"   | jq -r '.rname')
            serial=$(echo "$record"  | jq -r '.serial')
            refresh=$(echo "$record" | jq -r '.refresh')
            retry=$(echo "$record"   | jq -r '.retry')
            expire=$(echo "$record"  | jq -r '.expire')
            min_ttl=$(echo "$record" | jq -r '.minimum')
            echo "    mname: \"${mname}\""
            echo "    rname: \"${rname}\""
            echo "    serial: ${serial}"
            echo "    refresh: ${refresh}"
            echo "    retry: ${retry}"
            echo "    expire: ${expire}"
            echo "    minimum: ${min_ttl}"
            ;;
        SRV)
            local priority weight port target
            priority=$(echo "$record" | jq -r '.priority')
            weight=$(echo "$record"   | jq -r '.weight')
            port=$(echo "$record"     | jq -r '.port')
            target=$(echo "$record"   | jq -r '.target')
            echo "    priority: ${priority}"
            echo "    weight: ${weight}"
            echo "    port: ${port}"
            echo "    target: \"${target}\""
            ;;
        *)
            echo "warning: unknown record type '${record_type}', skipping" >&2
            return 1
            ;;
    esac
}

# Process all records
echo "$RECORDS" | jq -c '.data.records[]' | while read -r record; do
    generate_crd "$record"
    echo "---"
done | if [ "$APPLY" = true ]; then
    log "Applying CRDs to cluster..."
    kubectl apply -f -
    log "Migration complete."
else
    cat
    log "YAML generated. Pipe to 'kubectl apply -f -' or save to a file."
fi
