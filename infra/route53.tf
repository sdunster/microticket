# Looks up the existing parent zone rather than creating one — the zone (and
# its NS delegation at the registrar, if any) is assumed to already exist in
# this account. See variables.tf's parent_zone_name.
data "aws_route53_zone" "parent" {
  name         = var.parent_zone_name
  private_zone = false
}

locals {
  # CloudFront's fixed hosted-zone id for Route53 alias targets — the same
  # value for every CloudFront distribution, in every account, worldwide.
  # Not a secret or an account-specific value; it's a constant AWS
  # publishes.
  cloudfront_alias_zone_id = "Z2FDTNDATAQYW2"
}

# ── Web app ──────────────────────────────────────────────────────────────────

resource "aws_route53_record" "web_a" {
  zone_id = data.aws_route53_zone.parent.zone_id
  name    = var.support_domain
  type    = "A"

  alias {
    name                   = aws_cloudfront_distribution.web.domain_name
    zone_id                = local.cloudfront_alias_zone_id
    evaluate_target_health = false
  }
}

resource "aws_route53_record" "web_aaaa" {
  zone_id = data.aws_route53_zone.parent.zone_id
  name    = var.support_domain
  type    = "AAAA"

  alias {
    name                   = aws_cloudfront_distribution.web.domain_name
    zone_id                = local.cloudfront_alias_zone_id
    evaluate_target_health = false
  }
}

# ── Inbound mail ─────────────────────────────────────────────────────────────
# Coexists with the A/AAAA aliases above on the same name — var.support_domain
# serves both the web app and inbound mail (see variables.tf).

resource "aws_route53_record" "mx" {
  zone_id = data.aws_route53_zone.parent.zone_id
  name    = var.support_domain
  type    = "MX"
  ttl     = 300
  records = ["10 inbound-smtp.${var.aws_region}.amazonaws.com"]
}

resource "aws_route53_record" "spf" {
  zone_id = data.aws_route53_zone.parent.zone_id
  name    = var.support_domain
  type    = "TXT"
  ttl     = 300
  records = ["v=spf1 include:amazonses.com ~all"]
}

resource "aws_route53_record" "dmarc" {
  zone_id = data.aws_route53_zone.parent.zone_id
  name    = "_dmarc.${var.support_domain}"
  type    = "TXT"
  ttl     = 300
  records = ["v=DMARC1; p=none;"]
}

# ── SES easy DKIM ────────────────────────────────────────────────────────────

resource "aws_route53_record" "ses_dkim" {
  count = 3

  zone_id = data.aws_route53_zone.parent.zone_id
  name    = "${aws_sesv2_email_identity.main.dkim_signing_attributes[0].tokens[count.index]}._domainkey.${var.support_domain}"
  type    = "CNAME"
  ttl     = 1800
  records = ["${aws_sesv2_email_identity.main.dkim_signing_attributes[0].tokens[count.index]}.dkim.amazonses.com"]
}

# ── SES custom MAIL FROM (mail.<support_domain>) ────────────────────────────

resource "aws_route53_record" "mail_from_mx" {
  zone_id = data.aws_route53_zone.parent.zone_id
  name    = "mail.${var.support_domain}"
  type    = "MX"
  ttl     = 300
  records = ["10 feedback-smtp.${var.aws_region}.amazonses.com"]
}

resource "aws_route53_record" "mail_from_spf" {
  zone_id = data.aws_route53_zone.parent.zone_id
  name    = "mail.${var.support_domain}"
  type    = "TXT"
  ttl     = 300
  records = ["v=spf1 include:amazonses.com ~all"]
}
