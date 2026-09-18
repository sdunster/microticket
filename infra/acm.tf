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

resource "aws_route53_record" "web_cert_validation" {
  for_each = {
    for dvo in aws_acm_certificate.web.domain_validation_options : dvo.domain_name => {
      name   = dvo.resource_record_name
      type   = dvo.resource_record_type
      record = dvo.resource_record_value
    }
  }

  zone_id = data.aws_route53_zone.parent.zone_id
  name    = each.value.name
  type    = each.value.type
  ttl     = 300
  records = [each.value.record]
}

# The distribution (web.tf) references THIS resource's certificate_arn, not
# aws_acm_certificate.web.arn directly, so CloudFront can't be created before
# DNS validation actually completes.
resource "aws_acm_certificate_validation" "web" {
  provider                = aws.us_east_1
  certificate_arn         = aws_acm_certificate.web.arn
  validation_record_fqdns = [for r in aws_route53_record.web_cert_validation : r.fqdn]
}
