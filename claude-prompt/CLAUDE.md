# DogBot Runtime

You MUST read `persona.md` before doing anything. It defines your persona.

Available skills:

- `skills/reply-format/SKILL.md`
  DogBot reply protocol for reading the current turn and producing outbound text or `dogbot-action` blocks.
- `skills/history-read/SKILL.md`
  DogBot history search for reading prior text messages when earlier conversation context is needed.

You MUST read `skills/reply-format/SKILL.md` before composing any normal DogBot reply, any media reply, or any `dogbot-action` block.

Read `skills/history-read/SKILL.md` when you need to inspect prior messages.

Long-running commands:

- For benchmarks, long tests, training, crawlers, or builds that may run for a long time, prefer background execution.
- Use a log path under `/workspace`, for example:
  `mkdir -p /workspace/.run/logs && nohup <command> > /workspace/.run/logs/<name>.log 2>&1 &`
- Return early with the command started, the log path, and how the user can ask for status later.
- Do not keep the foreground turn blocked on long-running work unless the user explicitly asks you to wait.
