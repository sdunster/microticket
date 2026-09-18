output "api_function_url" {
  description = "Paste into the VITE_API_URL GitHub repo variable (deploy-prod.yml's web build step)."
  value       = aws_lambda_function_url.api.function_url
}

output "github_deploy_role_arn" {
  description = "Paste into the AWS_DEPLOY_ROLE_ARN GitHub repo variable (deploy-prod.yml's OIDC role assumption)."
  value       = aws_iam_role.github_deploy.arn
}

output "cloudfront_distribution_id" {
  description = "Paste into the CLOUDFRONT_DISTRIBUTION_ID GitHub repo variable (deploy-prod.yml's post-deploy invalidation)."
  value       = aws_cloudfront_distribution.web.id
}

output "web_bucket_name" {
  description = "Paste into the WEB_BUCKET_NAME GitHub repo variable (deploy-prod.yml's S3 sync target)."
  value       = aws_s3_bucket.web.id
}

output "route53_name_servers" {
  description = "Informational only — the parent zone is looked up (data \"aws_route53_zone\"), not created, so nothing needs to be pasted anywhere unless the zone itself is new to its registrar."
  value       = data.aws_route53_zone.parent.name_servers
}
