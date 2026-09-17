# Partial configuration: bucket/key/region/profile are supplied at init time so
# no account-specific name is committed:
#
#   terraform init \
#     -backend-config="bucket=<state bucket>" \
#     -backend-config="key=microticket/terraform.tfstate" \
#     -backend-config="region=<region>" \
#     -backend-config="profile=<profile>"
#
# `make check` runs `terraform init -backend=false`, so it never needs these.
terraform {
  backend "s3" {
    encrypt = true
  }
}
