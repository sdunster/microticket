# SES: the domain identity (with easy DKIM), a custom MAIL FROM subdomain, a
# configuration set for reputation tracking, and inbound receiving.
#
# Uses the SESv2 identity/configuration-set resources (aws_sesv2_*) but the
# classic v1 receipt-rule resources for inbound — the Terraform AWS provider
# has no SESv2 equivalent for receipt rules, and both APIs operate on the
# same underlying account identity, so mixing them is safe.

resource "aws_sesv2_configuration_set" "main" {
  configuration_set_name = "microticket"

  reputation_options {
    reputation_metrics_enabled = true
  }

  sending_options {
    sending_enabled = true
  }
}

resource "aws_sesv2_email_identity" "main" {
  email_identity = var.support_domain

  # Every send from this identity — including the raw sends api/src/sesmail.rs
  # makes via aws-sdk-sesv2, which never sets a per-message configuration set
  # — uses this one by default. That's what makes the bounce/complaint
  # reputation metrics in monitoring.tf actually see traffic.
  configuration_set_name = aws_sesv2_configuration_set.main.configuration_set_name
}

resource "aws_sesv2_email_identity_mail_from_attributes" "main" {
  email_identity   = aws_sesv2_email_identity.main.email_identity
  mail_from_domain = "mail.${var.support_domain}"

  # If the custom MAIL FROM domain's MX/SPF records ever go missing (DNS
  # propagation lag, a misconfigured zone), fall back to sending from
  # amazonses.com rather than hard-failing every outbound send.
  behavior_on_mx_failure = "USE_DEFAULT_VALUE"
}

# ── Inbound receiving ─────────────────────────────────────────────────────────
#
# An AWS account has exactly ONE active receipt rule set per region. If
# anything else in this account ever receives mail, aws_ses_active_receipt_rule_set
# below is the resource that conflicts — check `aws ses
# describe-active-receipt-rule-set` before applying against an account that
# might already have one.

resource "aws_ses_receipt_rule_set" "main" {
  rule_set_name = "microticket"
}

resource "aws_ses_active_receipt_rule_set" "main" {
  rule_set_name = aws_ses_receipt_rule_set.main.rule_set_name
}

# recipients = [var.support_domain] (not a specific address) catches every
# local part at that domain — this is what makes both plain addresses and
# the "*@domain" wildcard inbound_address kind work; routing.rs resolves the
# actual address -> instance mapping downstream, this rule just accepts
# everything and hands it to S3 + SNS.
resource "aws_ses_receipt_rule" "inbound" {
  name          = "inbound"
  rule_set_name = aws_ses_receipt_rule_set.main.rule_set_name
  recipients    = [var.support_domain]
  enabled       = true
  scan_enabled  = true

  s3_action {
    position          = 1
    bucket_name       = aws_s3_bucket.mail.id
    object_key_prefix = "inbound/"
  }

  sns_action {
    position  = 2
    topic_arn = aws_sns_topic.inbound_mail.arn
  }

  # SES must be able to write to the bucket before this rule can reference
  # it — without this, a from-scratch apply can create the rule before the
  # bucket policy exists.
  depends_on = [aws_s3_bucket_policy.mail]
}
