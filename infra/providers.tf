# CloudFront's ACM certificate must be issued in us-east-1 regardless of which
# region the rest of the stack lives in. Used only by acm.tf (infra + deploy
# step); declared here now so `terraform validate` has a stable provider set as
# resources land.
provider "aws" {
  alias   = "us_east_1"
  region  = "us-east-1"
  profile = var.aws_profile
}
