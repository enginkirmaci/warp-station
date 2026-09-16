# Concept: a chat window over `opencode serve`

Standalone Avalonia concept app — **not** part of the DevTools solution. It spawns
`opencode serve` in a chosen working folder (or attaches if the port already answers),
drives a conversational chat over its HTTP + SSE API, and renders opencode permission
asks as inline cards with **Allow / Always allow / Deny**.

## Run

```sh
dotnet run --project concept/ConceptChat
```

Type a prompt, press Enter (or Send). To see a permission card, ask for something that
triggers a tool, e.g. *"Use your bash tool to actually execute: echo hello. You must
really run it."* — bash/edit/webfetch are forced to "ask" (below).

## How it works

- Spawn: `opencode serve --hostname 127.0.0.1 --port 14096` with `WorkingDirectory` set
  to the chosen folder. Permissions are forced to *ask* via the `OPENCODE_CONFIG_CONTENT`
  env var (opencode 1.18.31 has no `--config` flag on serve) so the concept always
  exercises the approval flow without touching your global config or the folder.
- Chat: `POST /session` once, then `POST /session/{id}/message`; the UI is driven purely
  by the `GET /event` SSE stream (`message.part.updated` carries the full text-so-far).
- Permissions: `permission.asked` events become cards; a reply is
  `POST /session/{id}/permissions/{permissionId}` with `{"response":"once"|"always"|"reject"}`
  (v2 asks go to `POST /permission/{requestId}/reply`).
- Close/Stop kills a spawned server (process tree); an attached server is left running.

Verified live against opencode 1.18.31 on Linux: allow, deny, streaming, model list
(`GET /config/providers`), and orphan-free shutdown.

Note: whether a model actually calls a tool (and thus triggers a card) is model
behavior — instruct it explicitly ("really run … via the tool") if it answers in text.
