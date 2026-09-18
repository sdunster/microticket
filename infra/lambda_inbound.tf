resource "aws_lambda_function" "inbound_mail" {
  function_name = "microticket-inbound-mail"
  role          = aws_iam_role.inbound_mail_lambda.arn
  runtime       = "provided.al2023"
  handler       = "bootstrap"
  timeout       = 60
  memory_size   = 512
  filename      = "${path.module}/placeholder.zip"

  environment {
    variables = {
      DB_PREFIX = var.db_prefix
      # api/src/s3storage.rs: same bucket the API lambda reads/writes — raw
      # MIME lands under inbound/, extracted attachments under attachments/.
      MAIL_BUCKET = aws_s3_bucket.mail.id
    }
  }

  logging_config {
    log_format = "JSON"
  }

  # See lambda_api.tf's identical block — Terraform owns configuration, CI
  # owns code.
  lifecycle {
    ignore_changes = [filename, source_code_hash]
  }
}

resource "aws_lambda_event_source_mapping" "inbound_mail" {
  event_source_arn = aws_sqs_queue.inbound_mail.arn
  function_name    = aws_lambda_function.inbound_mail.arn
  batch_size       = 1
}
