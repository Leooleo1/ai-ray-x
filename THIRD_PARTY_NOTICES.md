# Third-party notices

raiox is MIT licensed (see `LICENSE`). It includes or builds with the
following work by others, under their own licenses.

## open-bot-avatar (MIT)

The avatar shapes in `assets/avatars/` were copied from the open-bot-avatar
gallery, and the panel renders every avatar with the `open-bot-avatar` npm
package. Its license:

```
MIT License

Copyright (c) 2026 Dominik Scholz

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## AI logos (lobe-icons, MIT)

The marks in `apps/web/public/ia/` (Claude, OpenAI, Cursor, Antigravity and
opencode) were copied from the `@lobehub/icons-static-svg` package, version
1.104.0 (https://github.com/lobehub/lobe-icons), with the single-colour ones
filled light for the dark panel. The package is MIT licensed, Copyright (c)
LobeHub. The marks themselves are trademarks of their owners; raiox shows
them only to say which AI a conversation, a plan or a cost belongs to.

## Fonts (SIL Open Font License 1.1)

The panel loads these fonts from their npm packages at build time; the font
files are not in this repository, and each build ships them with their license.

- Geist and Geist Mono: Copyright 2024 The Geist Project Authors
  (https://github.com/vercel/geist-font), packages `@fontsource-variable/geist`
  and `@fontsource-variable/geist-mono`.
- Doto: Copyright 2024 The Doto Project Authors
  (https://github.com/oliverlalan/Doto), package `@fontsource/doto`.

## Libraries

The Rust crates in `Cargo.lock` and the npm packages in
`apps/web/package.json` are used under their own licenses (mostly MIT or
Apache-2.0) and are fetched at build time, not vendored here.
