# Contributing

Thanks for helping. raiox is small and opinionated, so open an issue before a
large change: it saves you work if the change does not fit.

## Run it from source

See [docs/building.md](docs/building.md). In short:

```sh
cd apps/web && pnpm install && pnpm build
cd ../.. && cargo build --release
./target/release/raiox open
```

To try changes without touching your own data, point raiox at a throwaway
index: `./target/release/raiox open --db /tmp/raiox-try/index.sqlite`.
Do not test bots on projects you care about; make a small test folder.

## Before you open a pull request

```sh
cargo test --release
cargo clippy --all-targets --release   # no warnings
cd apps/web && npx tsc --noEmit -p . && pnpm build
```

- Add a test for a fix or a new behaviour, and see it fail without your change.
- Keep the panel's texts in both languages (`apps/web/src/i18n.ts`, English and
  Portuguese).
- The panel follows one visual scale (radii, type sizes, depths in
  `apps/web/src/styles.css`): reuse its tokens and classes instead of adding
  new values.
- Never commit secrets, `.env` files, personal paths or anyone's logs. Test
  fixtures are made up.

## Commits

One change per commit, with a message in the form `type: what changed`
(`feat`, `fix`, `style`, `docs`, `test`, `chore`, `refactor`, `perf`), and a
body that says why and how you tested it.

## Security

Found a way to reach someone's data or run something they did not approve?
Do not open an issue: follow [SECURITY.md](SECURITY.md).
