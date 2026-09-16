# opencode agent — a chat client over `opencode serve`

Standalone Avalonia concept app — **not** part of the DevTools solution. It spawns
`opencode serve` in a chosen working folder (or attaches if the port already answers),
drives a conversational chat over its HTTP + SSE API, and renders opencode permission
asks as inline cards with **Allow / Always allow / Deny**.

## Run

```sh
dotnet run --project concept/OpenCodeAgent
```

Type a prompt, press Enter (or Send). Shift+Enter inserts a newline. To see a
permission card, ask for something that triggers a tool, e.g. *"Use your bash tool to
actually execute: echo hello. You must really run it."* — bash/edit/webfetch are
forced to "ask" (below).

## Features

- Chat sessions in a left sidebar: list (newest first), switch (transcript is restored
  from the server), rename (context menu), delete (context menu), New chat. New
  sessions are auto-titled from the first message.
- Streaming replies with markdown rendering (headings, bold/italic, inline code,
  fenced code blocks, lists, links), selectable assistant text, timestamps.
- Image attachments: paste from the clipboard (Ctrl+V in the input or the Paste
  button), preview chips with remove, sent as `file` parts; images in the
  transcript render as bubbles and can be copied back out.
- Model picker with a variant combo (per-model `variants` from
  `/config/providers`, e.g. reasoning effort low/high/max); `(default)` omits
  the field.
- Stop button aborts the running turn (`POST /session/{id}/abort`).
- Context menu on any message: Copy.
- Smart autoscroll: follows the stream only while you are at the bottom; a
  "jump to latest" button appears when you scroll up.
- Typing indicator while the turn is running; empty-state hints.

## How it works

- Spawn: `opencode serve --hostname 127.0.0.1 --port 14096` with `WorkingDirectory` set
  to the chosen folder. Permissions are forced to *ask* via the `OPENCODE_CONFIG_CONTENT`
  env var (opencode 1.18.31 has no `--config` flag on serve) so the app always
  exercises the approval flow without touching your global config or the folder.
- Chat: sessions come from `GET /session`, transcripts from `GET /session/{id}/message`;
  sending is `POST /session/{id}/message` (fire-and-forget) and the UI is driven purely
  by the `GET /event` SSE stream (`message.part.updated` carries the full text-so-far).
  Images ride along as `{type:"file", mime, filename, url:"data:image/png;base64,…"}`
  parts; the optional `variant` is a top-level body field next to `model`.
- Permissions: `permission.asked` events become cards; a reply is
  `POST /session/{id}/permissions/{permissionId}` with `{"response":"once"|"always"|"reject"}`
  (v2 asks go to `POST /permission/{requestId}/reply`).
- Rename/delete: `PATCH`/`DELETE /session/{id}`; abort: `POST /session/{id}/abort`.
- Close/Stop kills a spawned server (process tree); an attached server is left running.

Verified live against opencode 1.18.31 on Linux: allow, deny, streaming, model list
(`GET /config/providers`), session list/restore/rename/delete/abort, and orphan-free
shutdown.

Note: whether a model actually calls a tool (and thus triggers a card) is model
behavior — instruct it explicitly ("really run … via the tool") if it answers in text.
