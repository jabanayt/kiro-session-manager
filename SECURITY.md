# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability, please report it privately rather than opening a public GitHub issue.

**Email:** kirosessionmanager@gmail.com

## What to Include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

## Response

You can expect an initial response within 7 days. We will work with you to understand and address the issue before any public disclosure.

## Scope

KSM is a local CLI tool that reads from kiro-cli's local SQLite database and stores metadata locally. The primary security concerns are:
- Unintended access to session data
- Path traversal in configuration
- Metadata integrity
