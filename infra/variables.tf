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
  description = "AWS CLI/SSO profile Terraform uses for all providers (no default — there is no account to point at until you set one up)"
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

# microticket has no application secret at all: sessions are opaque `mtu_`
# tokens stored only as a sha256 in DynamoDB (see api/src/auth.rs), so there
# is no signing key to provision. Turnstile is the one optional exception —
# verification is skipped whenever this is unset (api/src/turnstile.rs), so a
# fork works with no Cloudflare account. Passed straight through to the API
# Lambda's environment (lambda_api.tf) rather than via SSM: with only one
# optional value in the entire system, a SecureString parameter plus its own
# IAM read policy would be more machinery than the secret it protects.
variable "turnstile_secret_key" {
  description = "Cloudflare Turnstile secret key for the API Lambda (optional — leave unset to skip CAPTCHA verification entirely)"
  type        = string
  default     = ""
  sensitive   = true
}
