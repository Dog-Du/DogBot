---
name: reply-format
description: Use when composing any normal DogBot reply so the output matches DogBot's text-plus-action protocol
---

# Reply Format

## Overview

Use this skill whenever you reply through DogBot.

DogBot expects your output to be:

- plain user-visible text
- optionally followed by one `dogbot-action` fenced block containing JSON

DogBot will parse your output and send the actual platform messages for you.

## Read The Current Turn

Read the current turn from the prompt you were given:

- `Turn context (JSON)`
  - `conversation` and `actor` are short-lived metadata for this turn
  - `trigger_message_id` is the current message id when you need to reply to or react to the triggering message
  - `trigger_reply_to_message_id` is the message id that the current message itself replied to, when present
  - `reply_excerpt` is quoted context when available
  - `trigger_summary` is a compact summary of what triggered this turn
  - `mention_refs` maps `[#m1]`, `[#m2]` and similar markers in `trigger_summary` back to canonical `actor_id` values
- `User prompt`
  - this is the main message content you should answer

Treat `reply_excerpt` as context only. Do not repeat it unless the reply really needs it.

If `trigger_summary` contains something like `@fly-dog[#m1]`, that means:

- the user-visible mention text is `@fly-dog`
- the real target id is in `mention_refs`

Use the `actor_id` from `mention_refs` when you need to mention the same person in a structured message. Do not copy `[#m1]` into your user-visible reply text.

## How To Reply

Default to plain text.

If you only need to say something, output plain text and nothing else:

```text
今天这件事已经处理完啦，结果是服务已经恢复正常。
```

If you need non-text delivery, keep the normal text outside the action block and append one `dogbot-action` block after it:

```text
图片我放在下面啦。
```dogbot-action
{"type":"send_image","source_type":"workspace_path","source_value":"/workspace/outbox/result.png"}
```
```

If you need multiple actions, use one envelope:

```text
整理好了，附件在下面。
```dogbot-action
{"actions":[
  {"type":"send_file","source_type":"workspace_path","source_value":"/workspace/outbox/report.pdf"},
  {"type":"send_image","source_type":"workspace_path","source_value":"/workspace/outbox/cover.png","caption_text":"封面图"}
]}
```
```

If you need to control which message DogBot replies to, add `reply_to` inside the same action block:

- omit `reply_to`: use DogBot's default behavior, which usually replies to the triggering message
- `"reply_to":"123456"`: explicitly reply to message `123456`
- `"reply_to":null`: explicitly do not add a reply target

Example: keep the normal text but disable the default reply target:

```text
这条我直接说，不挂 reply。
```dogbot-action
{"reply_to":null}
```
```

Example: override the default reply target:

```text
我回上一条就好。
```dogbot-action
{"reply_to":"123456"}
```
```

If you need structured mentions in the same visible message, add `mentions` inside the same action block. The normal text still stays outside the JSON block:

```text
请看一下。
```dogbot-action
{"mentions":[{"actor_id":"qq:user:77","display":"@fly-dog"}]}
```
```

Each mention object needs:

- `actor_id`
- `display`

When the current turn includes `mention_refs`, use those values directly instead of inventing them.

## Supported Actions

Reaction actions:

- `reaction_add`
- `reaction_remove`

Media actions:

- `send_image`
- `send_file`
- `send_voice`
- `send_video`
- `send_sticker`

Reaction example:

```text
收到啦。
```dogbot-action
{"type":"reaction_add","target_message_id":"99","emoji":"👍"}
```
```

`reaction` and `reply` are different:

- `reaction` is an action on an existing message
- `reply` is a new outbound message

If the user explicitly asks for a reaction on the current triggering message, and `trigger_message_id` is present in `Turn context (JSON)`, prefer a structured `reaction_add` action instead of replying with plain text only.

Example:

```text
收到啦。
```dogbot-action
{"type":"reaction_add","target_message_id":"trigger_message_id_here","emoji":"👍"}
```
```

Reaction-only example:

```text
```dogbot-action
{"type":"reaction_add","target_message_id":"99","emoji":"😂"}
```
```

Media example with caption:

```text
我把图发出来啦。
```dogbot-action
{"type":"send_image","source_type":"workspace_path","source_value":"/workspace/outbox/chart.png","caption_text":"本周趋势图"}
```
```

File example:

```text
报告整理好了，发你一份。
```dogbot-action
{"type":"send_file","source_type":"workspace_path","source_value":"/workspace/outbox/report.md"}
```

Mention example using `mention_refs` from the current turn:

```text
请他看一下。
```dogbot-action
{"mentions":[{"actor_id":"qq:user:77","display":"@fly-dog"}]}
```
```

Do not invent a mention target yourself. If you need to mention someone from the triggering message, first find their `actor_id` and `display` in `mention_refs`, then copy those values into `mentions`.
```

## Media Rules

- Media files must already exist under `/workspace`
- Do not invent file paths
- Do not reference `/state`, `/tmp`, the home directory, or any path outside `/workspace`
- If the file does not exist yet, say so in plain text instead of pretending it is ready

## Important Restrictions

- Do not use Markdown in outbound social-platform messages
- Do not rely on headings, lists, bold, inline code, or Markdown links for presentation
- Do not emit QQ CQ codes, WeChat private syntax, XML cards, or any other platform-specific syntax directly
- Do not try to encode reply or @ mention syntax yourself; DogBot runtime and platform adapters handle delivery behavior
- Do not wrap your whole reply in JSON
- Do not put explanations outside and then a second plain-text message inside `dogbot-action`

Bad:

```text
**处理完成**
- 结果在这里
[查看图片](file:///workspace/outbox/result.png)
```

Good:

```text
处理完成啦，结果我直接发在下面。
```dogbot-action
{"type":"send_image","source_type":"workspace_path","source_value":"/workspace/outbox/result.png"}
```
```

Bad:

```text
[CQ:reply,id=99][CQ:at,qq=42] 好了
```

Good:

```text
已经好了，你看一下。
```

## Common Mistakes

- Writing Markdown because it looks nicer in the model output
- Emitting platform-private syntax directly
- Pointing media actions at non-`/workspace` paths
- Splitting actions across multiple `dogbot-action` blocks when one block is enough
- Forgetting to keep normal user-visible text outside the JSON block
