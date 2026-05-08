#!/usr/bin/env bash
set -euo pipefail

# EKS setup script for RIND DNS server.
# Creates an EKS cluster, installs required controllers, pushes the image
# to ECR, and applies the kustomize manifests.
#
# Prerequisites:
#   - aws CLI configured (use AWS_PROFILE or env credentials)
#   - eksctl installed
#   - helm installed
#   - docker installed
#   - kubectl installed
#
# Usage:
#   AWS_PROFILE=myprofile AWS_REGION=eu-west-2 ./scripts/eks-setup.sh
#   AWS_PROFILE=myprofile AWS_REGION=eu-west-2 ./scripts/eks-setup.sh deploy
#   AWS_PROFILE=myprofile AWS_REGION=eu-west-2 ./scripts/eks-setup.sh teardown
#
# Environment variables:
#   AWS_PROFILE         - AWS CLI profile to use
#   AWS_REGION          - AWS region (required)
#   RIND_EKS_CLUSTER    - cluster name (default: rind-prod)
#   RIND_EKS_NODE_TYPE  - instance type (default: t3.medium)
#   RIND_EKS_NODES      - number of nodes (default: 3)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

CLUSTER_NAME="${RIND_EKS_CLUSTER:-rind-prod}"
REGION="${AWS_REGION:?AWS_REGION must be set (e.g. eu-west-2)}"
NODE_TYPE="${RIND_EKS_NODE_TYPE:-t3.medium}"
NODE_COUNT="${RIND_EKS_NODES:-3}"
ECR_REPO_NAME="rind"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[rind-eks]${NC} $*"; }
warn() { echo -e "${YELLOW}[rind-eks]${NC} $*"; }
err()  { echo -e "${RED}[rind-eks]${NC} $*" >&2; }

check_prerequisites() {
    local missing=()
    command -v aws >/dev/null 2>&1     || missing+=("aws")
    command -v eksctl >/dev/null 2>&1  || missing+=("eksctl")
    command -v helm >/dev/null 2>&1    || missing+=("helm")
    command -v docker >/dev/null 2>&1  || missing+=("docker")
    command -v kubectl >/dev/null 2>&1 || missing+=("kubectl")

    if [ ${#missing[@]} -gt 0 ]; then
        err "Missing required tools: ${missing[*]}"
        exit 1
    fi

    if ! aws sts get-caller-identity >/dev/null 2>&1; then
        err "AWS credentials not configured. Set AWS_PROFILE or run 'aws configure'."
        exit 1
    fi
}

get_account_id() {
    aws sts get-caller-identity --query Account --output text
}

get_ecr_uri() {
    local account_id
    account_id="$(get_account_id)"
    echo "${account_id}.dkr.ecr.${REGION}.amazonaws.com/${ECR_REPO_NAME}"
}

get_vpc_id() {
    aws eks describe-cluster --name "$CLUSTER_NAME" --region "$REGION" \
        --query "cluster.resourcesVpcConfig.vpcId" --output text
}

create_cluster() {
    if eksctl get cluster --name "$CLUSTER_NAME" --region "$REGION" >/dev/null 2>&1; then
        log "Cluster '$CLUSTER_NAME' already exists, skipping creation"
        aws eks update-kubeconfig --name "$CLUSTER_NAME" --region "$REGION"
        return
    fi

    log "Creating EKS cluster '$CLUSTER_NAME' in $REGION..."
    log "  Node type: $NODE_TYPE, Count: $NODE_COUNT"
    eksctl create cluster \
        --name "$CLUSTER_NAME" \
        --region "$REGION" \
        --nodegroup-name rind-workers \
        --node-type "$NODE_TYPE" \
        --nodes "$NODE_COUNT" \
        --nodes-min 2 \
        --nodes-max 5 \
        --managed \
        --with-oidc

    log "Cluster created successfully."
}

install_lb_controller() {
    local account_id
    account_id="$(get_account_id)"
    local policy_arn="arn:aws:iam::${account_id}:policy/AWSLoadBalancerControllerIAMPolicy"

    log "Setting up AWS Load Balancer Controller..."

    # Create IAM policy if it doesn't exist
    if ! aws iam get-policy --policy-arn "$policy_arn" >/dev/null 2>&1; then
        log "Creating IAM policy for LB controller..."
        local policy_file
        policy_file=$(mktemp)
        # shellcheck disable=SC2064
        trap "rm -f '$policy_file'" RETURN
        curl -sfo "$policy_file" \
            https://raw.githubusercontent.com/kubernetes-sigs/aws-load-balancer-controller/v2.7.1/docs/install/iam_policy.json
        aws iam create-policy \
            --policy-name AWSLoadBalancerControllerIAMPolicy \
            --policy-document "file://$policy_file" >/dev/null
    fi

    # Create IRSA service account if not exists
    if ! kubectl get serviceaccount aws-load-balancer-controller -n kube-system >/dev/null 2>&1; then
        log "Creating IRSA service account..."
        eksctl create iamserviceaccount \
            --cluster "$CLUSTER_NAME" \
            --region "$REGION" \
            --namespace kube-system \
            --name aws-load-balancer-controller \
            --attach-policy-arn "$policy_arn" \
            --attach-policy-arn arn:aws:iam::aws:policy/ElasticLoadBalancingFullAccess \
            --approve
    fi

    # Install/upgrade the controller via Helm
    helm repo add eks https://aws.github.io/eks-charts 2>/dev/null || true
    helm repo update eks

    local vpc_id
    vpc_id="$(get_vpc_id)"

    helm upgrade --install aws-load-balancer-controller eks/aws-load-balancer-controller \
        -n kube-system \
        --set clusterName="$CLUSTER_NAME" \
        --set serviceAccount.create=false \
        --set serviceAccount.name=aws-load-balancer-controller \
        --set region="$REGION" \
        --set vpcId="$vpc_id" \
        --wait --timeout 2m

    log "AWS Load Balancer Controller installed."
}

install_addons() {
    log "Installing EBS CSI driver addon..."
    if ! eksctl get addon --cluster "$CLUSTER_NAME" --region "$REGION" 2>/dev/null | grep -q "aws-ebs-csi-driver"; then
        eksctl create addon \
            --cluster "$CLUSTER_NAME" \
            --region "$REGION" \
            --name aws-ebs-csi-driver \
            --force
    else
        log "EBS CSI driver already installed"
    fi

    install_lb_controller
}

create_ecr_repo() {
    if aws ecr describe-repositories --repository-names "$ECR_REPO_NAME" --region "$REGION" >/dev/null 2>&1; then
        log "ECR repository '$ECR_REPO_NAME' already exists"
        return
    fi

    log "Creating ECR repository '$ECR_REPO_NAME'..."
    aws ecr create-repository \
        --repository-name "$ECR_REPO_NAME" \
        --region "$REGION" \
        --image-scanning-configuration scanOnPush=true
}

build_and_push() {
    local ecr_uri
    ecr_uri="$(get_ecr_uri)"

    log "Authenticating Docker to ECR..."
    aws ecr get-login-password --region "$REGION" | \
        docker login --username AWS --password-stdin "${ecr_uri%%/*}"

    log "Building RIND image with kubernetes feature..."
    docker build \
        -t "${ecr_uri}:latest" \
        --build-arg FEATURES=kubernetes \
        -f "$PROJECT_DIR/docker/Dockerfile" \
        "$PROJECT_DIR"

    log "Pushing image to ECR..."
    docker push "${ecr_uri}:latest"
    log "Image pushed: ${ecr_uri}:latest"
}

apply_manifests() {
    local ecr_uri account_id
    ecr_uri="$(get_ecr_uri)"
    account_id="$(get_account_id)"

    # Generate a local kustomization overlay without modifying tracked files.
    # This creates a temporary overlay that references the EKS overlay and
    # sets the correct image + service account annotation.
    local deploy_dir="$PROJECT_DIR/k8s/overlays/eks/.deploy"
    mkdir -p "$deploy_dir"

    cat > "$deploy_dir/kustomization.yaml" <<EOF
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - ..
patches:
  - target:
      kind: ServiceAccount
      name: rind-sa
    patch: |
      - op: replace
        path: /metadata/annotations/eks.amazonaws.com~1role-arn
        value: "arn:aws:iam::${account_id}:role/rind-irsa-role"
images:
  - name: rind
    newName: ${ecr_uri}
    newTag: latest
EOF

    log "Applying kustomize manifests (EKS overlay)..."
    kubectl apply -k "$deploy_dir"
    rm -rf "$deploy_dir"

    log "Waiting for deployment rollout..."
    kubectl rollout status deployment/rind -n rind-system --timeout=120s
}

show_status() {
    echo ""
    log "=== RIND EKS Deployment Status ==="
    echo ""
    kubectl get pods -n rind-system -o wide
    echo ""
    kubectl get svc -n rind-system
    echo ""

    local dns_endpoint
    dns_endpoint=$(kubectl get svc rind-dns -n rind-system \
        -o jsonpath='{.status.loadBalancer.ingress[0].hostname}' 2>/dev/null || echo "pending")

    log "=== Access Points ==="
    log "  DNS NLB:  $dns_endpoint"
    log "  Test:     dig @${dns_endpoint} www.example.com"
    echo ""
    log "=== Quick Start ==="
    log "  kubectl apply -f $PROJECT_DIR/k8s/examples/sample-records.yaml"
    log ""
    log "  Note: NLB may take 2-3 minutes to provision and pass health checks."
    log "  Health check uses TCP on port 8080 (REST API)."
}

teardown() {
    warn "This will delete the EKS cluster '$CLUSTER_NAME' and all resources in $REGION."
    read -p "Are you sure? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        log "Aborted."
        exit 0
    fi

    # Delete the LB controller first (cleans up NLBs/target groups)
    log "Removing AWS Load Balancer Controller..."
    helm uninstall aws-load-balancer-controller -n kube-system 2>/dev/null || true
    sleep 10

    log "Deleting EKS cluster '$CLUSTER_NAME'..."
    eksctl delete cluster --name "$CLUSTER_NAME" --region "$REGION" --wait

    log "Cluster deleted."

    read -p "Delete ECR repository '$ECR_REPO_NAME'? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        aws ecr delete-repository --repository-name "$ECR_REPO_NAME" --region "$REGION" --force
        log "ECR repository deleted."
    fi

    # Clean up IAM policy
    read -p "Delete IAM policy AWSLoadBalancerControllerIAMPolicy? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        local account_id
        account_id="$(get_account_id)"
        aws iam delete-policy --policy-arn "arn:aws:iam::${account_id}:policy/AWSLoadBalancerControllerIAMPolicy" 2>/dev/null || true
        log "IAM policy deleted."
    fi
}

# --- Main ---

check_prerequisites

case "${1:-setup}" in
    setup)
        create_cluster
        install_addons
        create_ecr_repo
        build_and_push
        apply_manifests
        show_status
        ;;
    deploy)
        create_ecr_repo
        build_and_push
        apply_manifests
        show_status
        ;;
    teardown|destroy|delete)
        teardown
        ;;
    status)
        show_status
        ;;
    *)
        err "Unknown command: $1"
        echo "Usage: $0 [setup|deploy|teardown|status]"
        exit 1
        ;;
esac
