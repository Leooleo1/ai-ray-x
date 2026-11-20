## What changed

<!-- One or two sentences, and why. Link the issue if there is one. -->

## How it was tested

<!-- What you ran or clicked, and what you saw. A screenshot for a visual change. -->

## Checklist

- [ ] `cargo test --release` and `cargo clippy --all-targets --release` pass, with no warnings
- [ ] `cd apps/web && npx tsc --noEmit -p . && pnpm build` passes
- [ ] New texts are in both languages (`i18n.ts`)
- [ ] No secrets, `.env` contents, personal paths or real logs in the change
