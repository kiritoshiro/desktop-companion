# Cybersecurity baseline

Project type: **Python desktop application**. Default branch: `main`.

Added scans: Semgrep CE, TruffleHog, zizmor, Trivy (weekly/manual on private repos). Existing checks are retained.
One Linux job shares setup across tools; timeout is 15 minutes and superseded runs
are cancelled. Private repos run fast source/secret/workflow checks on PRs, with
Trivy and WordPress PHPCS on weekly/manual runs. There is no duplicate private
push run. Public repositories also scan default-branch pushes and run heavy checks
on PRs. These conditions use the verified visibility at rollout; review them if
visibility changes. A weekly scan can detect dependency issues after a PR merges:
manually run the full baseline on a release candidate before shipping.

Budget planning: target 500 minutes/month for new private security checks, leaving
headroom for existing CI within the account's 2,000-minute Free allowance. This is
an estimate, not an enforced cap: measure actual durations and count all existing
workflows, dependabot PRs and retries. Example: 20 PR runs/week at 4 minutes plus
8 weekly scans at 5 minutes is about 516 minutes/month (4.3 weeks).
New Dependabot schedules are monthly with at most two open PRs per ecosystem.
Security updates remain urgent; do not defer an exploited vulnerability to that schedule.

All new scan findings fail the job. Installation, parsing or database errors are
incomplete coverage, never a clean result. Legacy findings need review; do not add
broad exclusions merely to make CI green. Review narrow exceptions with an owner,
reason and expiry. Existing advisory workflows remain explicitly advisory.

Source paths are in `.github/security/profile.json`. Generated/minified code and
vendor/node_modules are excluded from custom-code SAST; Trivy scans supported
lockfiles separately. Semgrep's engine is pinned; upstream registry packs may
evolve independently. A green scan does not establish that the application is safe.

Existing Gitleaks is preserved where present. New TruffleHog scans use PR commit
ranges or all fetched history otherwise, without credential verification. Output
contains only detector/file/line, never secret values. Deleted/unfetched refs are
not covered. Rotate a real leak before removing it from history.

Psalm taint analysis is a subsequent tuned phase: framework hooks/stubs, sources,
sinks and test cases must be modeled first. Existing PHPStan/Larastan is retained.
CodeQL does not scan PHP. ZAP/WPScan require an isolated staging target and WPScan
vulnerability API access; no production-targeted scan or deployment is added.

CLI failures work on private GitHub Free without paid SARIF features. Protected
branches on private repositories are unavailable on that plan. This change does
not alter release triggers; maintainers must verify the exact release commit.
Never ship `.github/security` or its tooling in application packages.

Public conversion is a separate decision. Review full history, release assets,
credentials, personal data, licensing and fork restrictions before changing
visibility. No visibility changes are part of this rollout.

CodeQL additionally scans python in this public repository and uploads results to code scanning. It automatically skips if made private.

Reviewed exception (2026-09-29, owner: repository maintainer; review by 2026-12-29):
`live_channel.channel_name` uses SHA-1 only to derive a stable IPC name from a
local directory, not to authenticate a peer or verify content. A one-call Semgrep
annotation preserves compatibility with existing overlay/settings processes.
