---
name: emit-memory-candidate
description: Use when a DogBot conversation contains a small durable preference, fact, or conclusion worth capturing as a reviewable memory candidate.
---

# Emit Memory Candidate

Use this skill when the current turn reveals one compact, durable memory worth reviewing later.

Good fits:

- Stable preferences
- Long-lived facts
- Repeated conventions
- Clear finished conclusions

Do not use this skill for:

- Transient emotions
- Jokes
- Large summaries
- Speculation

When you use this skill, emit exactly one fenced block with the tag `dogbot-memory`.

The block body must be JSON with these required fields:

- `scope`
- `summary`
- `raw_evidence`

Current scope values:

- `user-private`
- `conversation-shared`
- `platform-account-shared`
- `bot-global-admin`

Rules:

- `summary` should be short and durable.
- `raw_evidence` should preserve the concrete sentence or fact that justified the memory.
- If there is no strong candidate, do not emit the block.

Example:

```dogbot-memory
{"scope":"user-private","summary":"prefers Rust","raw_evidence":"The user said they prefer Rust over Go."}
```
