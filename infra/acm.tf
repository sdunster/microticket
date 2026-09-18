# CloudFront requires its certificate to be issued in us-east-1 regardless
# of which region the rest of the stack runs in — see providers.tf's
# aws.us_east_1 alias.

resource "aws_acm_certificate" "web" {
  provider          = aws.us_east_1
  domain_name       = var.support_domain
  validation_method = "DNS"

  lifecycle {
    create_before_destroy = true
  }
}

# The validation record itself is NOT created here: the parent zone lives in
# another AWS account (see dns.tf). It is emitted by the `dns_records_required`
# output for manual creation instead.
#
# This resource therefore simply waits for the certificate to reach ISSUED,
# with no validation_record_fqdns to point at. That makes the first apply a
# two-phase affair: apply once to mint the certificate and print the record,
# create the record by hand, then apply again. Terraform will sit here for up
# to the timeout below while it waits, which is the honest behaviour -- the
# distribution genuinely cannot serve TLS until the certificate issues.
resource "aws_acm_certificate_validation" "web" {
  provider        = aws.us_east_1
  certificate_arn = aws_acm_certificate.web.arn

  timeouts {
    create = "30m"
  }
}
