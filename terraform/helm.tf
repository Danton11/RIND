# AWS Load Balancer Controller
resource "helm_release" "lb_controller" {
  name       = "aws-load-balancer-controller"
  repository = "https://aws.github.io/eks-charts"
  chart      = "aws-load-balancer-controller"
  namespace  = "kube-system"
  version    = "1.7.1"

  set {
    name  = "clusterName"
    value = var.cluster_name
  }
  set {
    name  = "serviceAccount.create"
    value = "true"
  }
  set {
    name  = "serviceAccount.name"
    value = "aws-load-balancer-controller"
  }
  set {
    name  = "serviceAccount.annotations.eks\\.amazonaws\\.com/role-arn"
    value = aws_iam_role.lb_controller.arn
  }
  set {
    name  = "region"
    value = var.region
  }
  set {
    name  = "vpcId"
    value = aws_vpc.main.id
  }

  depends_on = [module.eks]
}

# RIND DNS server
resource "helm_release" "rind" {
  count = var.deploy_rind ? 1 : 0

  name             = "rind"
  chart            = "${path.module}/../charts/rind"
  namespace        = var.rind_namespace
  create_namespace = true

  set {
    name  = "image.repository"
    value = aws_ecr_repository.rind.repository_url
  }
  set {
    name  = "image.tag"
    value = "latest"
  }
  set {
    name  = "image.pullPolicy"
    value = "Always"
  }
  set {
    name  = "service.dns.type"
    value = "LoadBalancer"
  }
  set {
    name  = "service.dns.annotations.service\\.beta\\.kubernetes\\.io/aws-load-balancer-type"
    value = "external"
  }
  set {
    name  = "service.dns.annotations.service\\.beta\\.kubernetes\\.io/aws-load-balancer-nlb-target-type"
    value = "ip"
  }
  set {
    name  = "service.dns.annotations.service\\.beta\\.kubernetes\\.io/aws-load-balancer-scheme"
    value = "internet-facing"
  }
  set {
    name  = "service.dns.annotations.service\\.beta\\.kubernetes\\.io/aws-load-balancer-healthcheck-port"
    value = "8080"
  }
  set {
    name  = "service.dns.annotations.service\\.beta\\.kubernetes\\.io/aws-load-balancer-healthcheck-protocol"
    value = "TCP"
  }
  set {
    name  = "autoscaling.enabled"
    value = "true"
  }
  set {
    name  = "autoscaling.minReplicas"
    value = "2"
  }
  set {
    name  = "autoscaling.maxReplicas"
    value = "10"
  }
  set {
    name  = "serviceAccount.annotations.eks\\.amazonaws\\.com/role-arn"
    value = aws_iam_role.rind.arn
  }
  set {
    name  = "metrics.serviceMonitor.enabled"
    value = tostring(var.deploy_monitoring)
  }
  set {
    name  = "resources.requests.memory"
    value = "256Mi"
  }
  set {
    name  = "resources.requests.cpu"
    value = "200m"
  }
  set {
    name  = "resources.limits.memory"
    value = "512Mi"
  }
  set {
    name  = "resources.limits.cpu"
    value = "500m"
  }

  depends_on = [helm_release.lb_controller]
}

# Monitoring stack (Prometheus + Grafana)
resource "helm_release" "monitoring" {
  count = var.deploy_monitoring ? 1 : 0

  name             = "monitoring"
  repository       = "https://prometheus-community.github.io/helm-charts"
  chart            = "kube-prometheus-stack"
  namespace        = "monitoring"
  create_namespace = true

  values = [file("${path.module}/../k8s/monitoring/values.yaml")]

  depends_on = [module.eks]
}

# Loki for log aggregation
resource "helm_release" "loki" {
  count = var.deploy_monitoring ? 1 : 0

  name       = "loki"
  repository = "https://grafana.github.io/helm-charts"
  chart      = "loki-stack"
  namespace  = "monitoring"

  values = [file("${path.module}/../k8s/monitoring/loki-values.yaml")]

  depends_on = [helm_release.monitoring]
}
