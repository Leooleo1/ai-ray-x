# Security

raiox runs on your own computer and reads your AI tools' local logs, so a
flaw here can expose what is on that computer. Please report one privately:
open the repository's **Security** tab and choose **Report a vulnerability**.
Do not open a public issue for it.

Say what can be reached, from where (this computer, a paired phone, the
tunnel, a chat through the MCP server or Telegram), and how to reproduce it.
You will get an answer within a week.

What raiox is meant to guarantee:

- The panel listens on 127.0.0.1 only, behind a per-run token.
- From outside (the Cloudflare tunnel), only paired devices get in, and they
  cannot change the setup, the phone access, permissions or approvals.
- An AI changes a project only after you approve, or when you turned on
  "Edits without asking" for it, and always through an undoable draft.
- Tokens and keys raiox keeps (devices, push, Telegram) stay in private files
  in its data folder and never go on a command line.
