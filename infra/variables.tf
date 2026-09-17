variable "aws_region" {
  description = "AWS region for all resources except the CloudFront ACM certificate (which must be us-east-1)"
  type        = string
  default     = "ap-southeast-2"
}

variable "aws_account_id" {
  description = "AWS account ID for constructing ARNs (must be set explicitly)"
  type        = string
}

variable "aws_profile" {
  description = "AWS CLI/SSO profile Terraform uses for all providers"
  type        = string
  default     = "sdunster"
}

variable "parent_zone_name" {
  description = "Name of the existing Route53 hosted zone microticket's DNS records are added to (e.g. the apex domain whose zone already exists in this account)"
  type        = string
}

variable "support_domain" {
  description = "Domain that serves both the web app and inbound mail (e.g. support.example.com); A/AAAA aliases, the MX record, and the web distribution all live on this one name"
  type        = string
}

variable "github_repo" {
  description = "GitHub repository in \"owner/name\" form, used to scope the GitHub OIDC deploy role's trust policy"
  type        = string
}

variable "db_prefix" {
  description = "DynamoDB table name prefix (e.g. \"prod\" -> tables prod_ticket, prod_instance, ...)"
  type        = string
  default     = "prod"
}

variable "allowed_origins" {
  description = "CORS origins allowed to call the API Lambda function URL"
  type        = list(string)
  default     = []
}

variable "alert_email" {
  description = "Email address subscribed to the operational alert SNS topic"
  type        = string
}

variable "inbound_retention_days" {
  description = "Days raw inbound MIME objects are retained in S3 before lifecycle expiry"
  type        = number
  default     = 30
}
