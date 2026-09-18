resource "aws_s3_bucket" "web" {
  bucket = "microticket-web-${var.aws_account_id}"
}

resource "aws_s3_bucket_public_access_block" "web" {
  bucket = aws_s3_bucket.web.id

  block_public_acls       = true
  ignore_public_acls      = true
  block_public_policy     = true
  restrict_public_buckets = true
}

resource "aws_cloudfront_origin_access_control" "web" {
  name                              = "microticket-web"
  origin_access_control_origin_type = "s3"
  signing_behavior                  = "always"
  signing_protocol                  = "sigv4"
}

# Bucket policy is conditioned on the distribution's own ARN, so only this
# specific CloudFront distribution (not "any CloudFront distribution in the
# account") can read from the bucket.
resource "aws_s3_bucket_policy" "web" {
  bucket = aws_s3_bucket.web.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid       = "AllowCloudFrontOAC"
      Effect    = "Allow"
      Principal = { Service = "cloudfront.amazonaws.com" }
      Action    = "s3:GetObject"
      Resource  = "${aws_s3_bucket.web.arn}/*"
      Condition = {
        StringEquals = {
          "AWS:SourceArn" = aws_cloudfront_distribution.web.arn
        }
      }
    }]
  })
}

resource "aws_cloudfront_distribution" "web" {
  aliases             = [var.support_domain]
  enabled             = true
  http_version        = "http2"
  is_ipv6_enabled     = true
  default_root_object = "index.html"

  origin {
    origin_id                = "web-s3"
    domain_name              = aws_s3_bucket.web.bucket_regional_domain_name
    origin_access_control_id = aws_cloudfront_origin_access_control.web.id
  }

  # Everything except /assets/* (i.e. index.html and any other top-level
  # file Vite emits) — short TTL so a deploy is visible quickly. The web app
  # itself is versioned by VITE_CLIENT_VERSION for the mismatch banner, not
  # by cache-busting index.html's URL, so this TTL is the main thing
  # standing between a deploy and users seeing it.
  default_cache_behavior {
    target_origin_id       = "web-s3"
    viewer_protocol_policy = "redirect-to-https"
    allowed_methods        = ["GET", "HEAD"]
    cached_methods         = ["GET", "HEAD"]
    compress               = true
    min_ttl                = 0
    default_ttl            = 300
    max_ttl                = 300

    forwarded_values {
      query_string = false
      cookies {
        forward = "none"
      }
    }
  }

  # /assets/* is content-hashed by Vite (immutable filenames), so the
  # managed CachingOptimized policy (long TTL, gzip/brotli) is safe — a
  # stale cached copy here just means a browser reusing bytes that will
  # never change under that URL.
  ordered_cache_behavior {
    path_pattern           = "/assets/*"
    target_origin_id       = "web-s3"
    viewer_protocol_policy = "redirect-to-https"
    allowed_methods        = ["GET", "HEAD"]
    cached_methods         = ["GET", "HEAD"]
    compress               = true
    cache_policy_id        = "658327ea-f89d-4fab-a63d-7e88639e58f6" # managed: CachingOptimized
  }

  # The OAC origin returns 403 (not 404) for a missing S3 key, so both codes
  # need to fall through to the SPA shell for client-side routing
  # (/app/tickets/123 etc.) to work on a hard reload or direct link.
  custom_error_response {
    error_code            = 403
    response_code         = 200
    response_page_path    = "/index.html"
    error_caching_min_ttl = 10
  }

  custom_error_response {
    error_code            = 404
    response_code         = 200
    response_page_path    = "/index.html"
    error_caching_min_ttl = 10
  }

  restrictions {
    geo_restriction {
      restriction_type = "none"
    }
  }

  viewer_certificate {
    acm_certificate_arn      = aws_acm_certificate_validation.web.certificate_arn
    ssl_support_method       = "sni-only"
    minimum_protocol_version = "TLSv1.2_2021"
  }
}
