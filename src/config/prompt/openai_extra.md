# Guardrails

- **Simple-first**: prefer the smallest, local fix over a cross-file architecture change.
- **Reuse-first**: search for existing patterns; mirror naming, error handling, typing, tests.
- **No surprise edits**: if changes affect >3 files or multiple subsystems, show a short plan first.
- **No new deps** without explicit user approval.

# Verification Gates

After changes, verify: typecheck → lint → tests → build. Report pass/fail concisely. If pre-existing failures block you, say so.

# Final Status

2-10 lines. Lead with what changed and why. Include verification results. Suggest next action if natural.