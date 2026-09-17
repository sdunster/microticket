# Security Policy

## Reporting a vulnerability

**Please do not report security issues through public GitHub issues, discussions, or pull requests.**

We use [GitHub's private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability) for this project. To report a vulnerability:

1. Go to the **Security** tab of this repository.
2. Click **Report a vulnerability**.
3. Fill in the form with a description, affected component (`api/`, `web/`, `infra/`), reproduction steps, and impact.

This opens a private draft advisory visible only to the maintainers. You'll receive an acknowledgement within **7 days**, and a status update within **30 days**. If a fix is required, we'll coordinate a disclosure timeline with you and credit you in the published advisory (unless you'd prefer to remain anonymous).

## Scope

In scope:
- The Rust API and Lambda binaries under `api/`, including the inbound-mail parsing/routing
  pipeline
- The React/Relay frontend under `web/`
- The Terraform configuration under `infra/`

Out of scope:
- Vulnerabilities in third-party services we integrate with (AWS managed services, Cloudflare
  Turnstile) — please report those to the relevant vendor.
- Issues that require an already-compromised user account to exploit.
- Denial-of-service via volumetric traffic against shared infrastructure.
- Email spoofing/spam delivered *to* an instance's inbound address — that's an SES/DNS
  (SPF/DKIM/DMARC) concern, not an application vulnerability, unless it lets a message be
  misattributed to an existing thread it shouldn't join.

## Supported versions

Only the `main` branch is supported. Fixes will land there and propagate to the `prod` branch on
the next deployment.
