# Security Policy

## Supported versions

We patch only the latest minor release. Upgrade to receive security fixes.

| Version | Supported |
|---|---|
| 0.1.x | Yes (current) |
| < 0.1 | No |

## Reporting a vulnerability

Email: `security@datawizard.io` (to be set up — until then, open a
private security advisory at
<https://github.com/heictojpgpics/data-wizard-rs/security/advisories/new>).

- **Do not** open a public issue for security bugs.
- Expect an acknowledgement within 72 hours.
- Expect a fix or mitigation within 30 days for high-severity issues.
- Please do not publish the vulnerability until we've shipped a fix.

## Threat model summary

See [`docs/security/our-posture.md`](docs/security/our-posture.md) for
the full threat model.

In scope:
- Memory safety bugs in the Rust core.
- Path traversal in scan-result file paths.
- Privilege escalation via the (future) elevated helper service.
- DLL planting in the install directory.
- Malicious disk image crafted to crash / hang the parser.

Out of scope:
- Physical access to the user's machine (use BitLocker).
- Compromised code-signing certificate.
- Compromised GitHub account / Actions runner.
- Zero-days in WebView2 / Windows itself.

## Hardening

Every binary we ship has:

- ASLR (`DYNAMIC_BASE`)
- DEP / NX (`NX_COMPAT`)
- CFG (`GUARD_CF`)
- Stack cookies
- High-entropy VA (x86_64)

CI verifies these with `winchecksec` on every build.

The entire core is in Rust with `#![deny(unsafe_code)]` except for a
small, audited FFI shim in `dw-disk`'s Windows module.

## Dependency policy

- `Cargo.lock` and `package-lock.json` are committed.
- `cargo audit` and `npm audit` run in CI on every PR.
- No third-party proprietary code.
- No third-party telemetry / analytics SDKs.
