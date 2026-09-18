# The inbound-mail pipeline's delivery path:
#   SES receipt rule (ses.tf) -> S3 (raw MIME, s3_mail.tf) -> SNS topic
#     -> this SQS queue -> inbound-mail-lambda (event source mapping,
#        lambda_inbound.tf)

resource "aws_sqs_queue" "inbound_mail_dlq" {
  name                      = "microticket-inbound-dlq"
  message_retention_seconds = 1209600 # 14 days
}

resource "aws_sqs_queue" "inbound_mail" {
  name = "microticket-inbound"

  # Must be >= the inbound-mail lambda's timeout (60s, lambda_inbound.tf).
  # If it were shorter, SQS could make a message visible to a second
  # consumer while the first invocation was still mid-processing it —
  # exactly the duplicate-delivery race processed_message's conditional
  # PutItem exists to guard against, so there's no reason to invite it when
  # a generous visibility timeout avoids it for free.
  visibility_timeout_seconds = 90

  redrive_policy = jsonencode({
    deadLetterTargetArn = aws_sqs_queue.inbound_mail_dlq.arn
    maxReceiveCount     = 3
  })
}

data "aws_iam_policy_document" "inbound_mail_queue" {
  statement {
    sid    = "AllowSNSPublish"
    effect = "Allow"

    principals {
      type        = "Service"
      identifiers = ["sns.amazonaws.com"]
    }

    actions   = ["sqs:SendMessage"]
    resources = [aws_sqs_queue.inbound_mail.arn]

    condition {
      test     = "ArnEquals"
      variable = "aws:SourceArn"
      values   = [aws_sns_topic.inbound_mail.arn]
    }
  }
}

resource "aws_sqs_queue_policy" "inbound_mail" {
  queue_url = aws_sqs_queue.inbound_mail.id
  policy    = data.aws_iam_policy_document.inbound_mail_queue.json
}

resource "aws_sns_topic" "inbound_mail" {
  name = "microticket-inbound"
}

data "aws_iam_policy_document" "inbound_mail_topic" {
  statement {
    sid    = "AllowSESPublish"
    effect = "Allow"

    principals {
      type        = "Service"
      identifiers = ["ses.amazonaws.com"]
    }

    actions   = ["sns:Publish"]
    resources = [aws_sns_topic.inbound_mail.arn]

    condition {
      test     = "StringEquals"
      variable = "AWS:SourceAccount"
      values   = [var.aws_account_id]
    }
  }
}

resource "aws_sns_topic_policy" "inbound_mail" {
  arn    = aws_sns_topic.inbound_mail.arn
  policy = data.aws_iam_policy_document.inbound_mail_topic.json
}

resource "aws_sns_topic_subscription" "inbound_mail_sqs" {
  topic_arn = aws_sns_topic.inbound_mail.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.inbound_mail.arn
}
