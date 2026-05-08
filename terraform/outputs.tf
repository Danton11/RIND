output "cluster_name" {
  description = "EKS cluster name"
  value       = module.eks.cluster_name
}

output "cluster_endpoint" {
  description = "EKS cluster API endpoint"
  value       = module.eks.cluster_endpoint
}

output "ecr_repository_url" {
  description = "ECR repository URL for docker push"
  value       = aws_ecr_repository.rind.repository_url
}

output "kubeconfig_command" {
  description = "Command to configure kubectl"
  value       = "aws eks update-kubeconfig --name ${var.cluster_name} --region ${var.region}"
}

output "docker_push_commands" {
  description = "Commands to build and push the RIND image"
  value       = <<-EOT
    aws ecr get-login-password --region ${var.region} | docker login --username AWS --password-stdin ${aws_ecr_repository.rind.repository_url}
    docker build --build-arg FEATURES=kubernetes -t ${aws_ecr_repository.rind.repository_url}:latest -f docker/Dockerfile .
    docker push ${aws_ecr_repository.rind.repository_url}:latest
  EOT
}

output "lb_controller_role_arn" {
  description = "IAM role ARN for the AWS Load Balancer Controller"
  value       = aws_iam_role.lb_controller.arn
}

output "rind_role_arn" {
  description = "IAM role ARN for RIND pods (IRSA)"
  value       = aws_iam_role.rind.arn
}
