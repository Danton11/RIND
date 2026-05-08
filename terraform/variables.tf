variable "region" {
  description = "AWS region to deploy into"
  type        = string
}

variable "cluster_name" {
  description = "EKS cluster name"
  type        = string
  default     = "rind-prod"
}

variable "cluster_version" {
  description = "Kubernetes version for EKS"
  type        = string
  default     = "1.30"
}

variable "node_instance_type" {
  description = "EC2 instance type for worker nodes"
  type        = string
  default     = "t3.medium"
}

variable "node_desired_count" {
  description = "Desired number of worker nodes"
  type        = number
  default     = 3
}

variable "node_min_count" {
  description = "Minimum number of worker nodes"
  type        = number
  default     = 2
}

variable "node_max_count" {
  description = "Maximum number of worker nodes"
  type        = number
  default     = 5
}

variable "ecr_repository_name" {
  description = "Name of the ECR repository for RIND images"
  type        = string
  default     = "rind"
}

variable "rind_namespace" {
  description = "Kubernetes namespace for RIND deployment"
  type        = string
  default     = "rind-system"
}

variable "deploy_monitoring" {
  description = "Deploy Prometheus + Grafana + Loki monitoring stack"
  type        = bool
  default     = true
}

variable "deploy_rind" {
  description = "Deploy RIND via Helm chart (set false if deploying manually)"
  type        = bool
  default     = true
}
