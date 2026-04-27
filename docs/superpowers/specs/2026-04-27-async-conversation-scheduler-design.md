# Async Conversation Scheduler Design

## Goal

Simplify `agent-runner` scheduling and improve social-platform responsiveness.
Incoming platform webhooks should no longer block until Claude finishes. The
runner should accept a trigger quickly, give immediate user-visible feedback,
run work in the background, and deliver the final Claude output when the task
finishes.

This design intentionally does not add active in-turn replies, a job manager,
or `/agent-cancel` in the first implementation.

## Current Problems

`agent-runner` currently mixes several concerns in the `/v1/runs` dispatcher:

- global queue depth
- global concurrent workers
- global/user/conversation minute-level rate limits
- per-run timeout enforcement
- synchronous waiting for Claude completion before platform delivery

This causes poor behavior for long tasks. A benchmark or long command can keep
the platform webhook path waiting until the timeout fires, after which
`agent-runner` kills the Claude exec. The user sees a timeout instead of quick
acknowledgement and later completion.

## Design Summary

The new scheduler has three rules:

1. Limit total active Claude runs with `MAX_CONCURRENT_RUNS`.
2. Limit each conversation to one active run.
3. Maintain FIFO waiting queues for conversations whose current run is busy.

The runner removes minute-level rate limits and stops killing Claude runs by
elapsed wall-clock timeout. Long-running foreground work may occupy its
conversation slot until completion, so prompt guidance will tell Claude to move
expected long tasks into the background and return early.

## Non-Goals

- No `/agent-cancel` in this phase.
- No `reply-current` or other active in-turn send API in this phase.
- No generic background job manager in this phase.
- No durable scheduler persistence across `agent-runner` process restarts.
- No compatibility guarantee for the old timeout/rate-limit behavior.

## Scheduling Model

Each triggered platform event becomes a `ScheduledRun` that contains:

- a stable task id
- the original `CanonicalEvent`
- the built `RunRequest`
- the validated run input
- the conversation key, using platform account plus conversation id
- the actor id and trigger message id for logs and status snapshots
- enqueue timestamp

Scheduler state is in memory:

- `running_by_conversation`: maps conversation key to active task
- `queued_by_conversation`: per-conversation FIFO queues
- `ready_conversations`: FIFO list of conversations that have queued work and
  no active run
- `active_count`: current global active run count
- bounded recent terminal history for `/agent-status`

Admission behavior:

- If the conversation is idle and global active count has capacity, start the
  task immediately.
- If the conversation is busy, enqueue the task behind that conversation.
- If the conversation is idle but global active count is full, enqueue the task
  for that conversation and mark the conversation ready.
- If the global waiting queue capacity is full, reject the task with a busy
  reply and do not send a reaction.

Promotion behavior:

- When any task finishes, clear its conversation slot.
- If the same conversation has queued work and global capacity exists, start its
  next task.
- Then fill remaining global capacity from `ready_conversations` in FIFO order.
- A conversation can only appear once in `ready_conversations`.

`MAX_QUEUE_DEPTH` remains as a simple global waiting-task cap. Its meaning is
only "how many tasks may wait"; it is no longer tied to rate limiting.

## Feedback Behavior

For a task that starts immediately:

- Send a best-effort random reaction when execution actually starts.
- Do not send a separate queued text reply.
- Run Claude in the background and return the platform webhook response quickly.

For a task that must wait:

- Do not send a reaction yet.
- Send a text reply saying how many tasks are ahead.
- When the task later starts, send the random reaction at that time.

The queue count is calculated when the task is admitted:

```text
if the conversation is busy:
  tasks ahead = 1 running task in this conversation
              + queued tasks already ahead in this conversation

else if only global concurrency is full:
  tasks ahead = currently active global tasks
              + ready conversation head tasks already ahead in global FIFO
```

This keeps the first waiting task behind a running same-conversation task at
"1 task ahead", while also avoiding a misleading "0 tasks ahead" when the only
blocker is global concurrency.

For a queue-full rejection:

- Send a busy text reply.
- Do not enqueue.
- Do not send a reaction.

Reaction failure is logged and ignored. It must not block task execution.

QQ can use platform reactions. WeChatPadPro may no-op until a stable reaction
or equivalent feedback primitive exists.

## Execution Flow

For platform ingress:

1. Decode platform payload to `CanonicalEvent`.
2. Mirror history as today.
3. Resolve trigger.
4. If ignored, return accepted.
5. If `/agent-status`, send status synchronously as today.
6. If runnable, build `RunRequest` and admit it to the scheduler.
7. Send queued/busy feedback when needed.
8. Return the webhook response without waiting for Claude.

For task execution:

1. Scheduler starts a background task.
2. The start path sends random reaction best-effort.
3. Run `runner.run(request, validated)` using the existing `DockerRunner`.
4. Normalize Claude output with the existing `normalize_agent_output`.
5. Deliver the final `OutboundPlan` using the original `CanonicalEvent`.
6. Record terminal status and promote more queued work.

This preserves the current final-output protocol. Claude still replies by
printing normal text plus optional `dogbot-action` blocks, and `agent-runner`
still sends those only after Claude exits.

## Timeout and Rate Limit Changes

The scheduler removes minute-level rate limiting:

- `GLOBAL_RATE_LIMIT_PER_MINUTE`
- `USER_RATE_LIMIT_PER_MINUTE`
- `CONVERSATION_RATE_LIMIT_PER_MINUTE`

The execution layer stops applying elapsed wall-clock timeout to Claude runs.
`DEFAULT_TIMEOUT_SECS` and `MAX_TIMEOUT_SECS` become obsolete for agent
execution. The `timeout_secs` field can remain in request models during the
transition but is ignored by scheduler-backed platform runs.

This means a stuck foreground Claude run can occupy its conversation slot
indefinitely. That is accepted for this phase because `/agent-cancel` is
explicitly out of scope. Operators can still restart `agent-runner` or the
Claude container manually if needed.

## Long-Task Prompt Guidance

`claude-prompt/CLAUDE.md` should add operational guidance:

- For benchmarks, long tests, model training, long builds, crawlers, or any
  command likely to take a long time, prefer starting it in the background.
- Use a durable log path under `/workspace`, for example:

```bash
nohup <command> > .run/logs/<name>.log 2>&1 &
```

- Return early with what was started, where logs are written, and how the user
  can ask for a later status check.
- Do not keep the foreground agent turn blocked on long-running work unless the
  user explicitly asks to wait for completion.

This is prompt guidance only. A future phase may add a first-class job-control
skill.

## Status

`/agent-status` should continue to work and should be expanded to show:

- active task count and `MAX_CONCURRENT_RUNS`
- running task summaries by conversation
- total waiting task count and `MAX_QUEUE_DEPTH`
- recent terminal task summaries

This phase does not add `/agent-queue` or `/agent-cancel`.

## Error Handling

- Queue full: reply busy text, return accepted to platform if the rejection
  reply was attempted.
- Reaction failure: log warning, continue.
- Claude run failure: deliver an error reply using existing run-result mapping
  where possible, record terminal failure, promote next tasks.
- Output normalization failure: deliver or log an internal error using existing
  handling, record terminal failure, promote next tasks.
- Delivery failure: log and record terminal failure; do not block scheduler
  promotion.

## Testing Strategy

Unit tests:

- same-conversation tasks are serialized
- different conversations can run up to `MAX_CONCURRENT_RUNS`
- queue count covers same-conversation blockers and global-concurrency blockers
- queue full rejects without reaction
- finishing a task promotes the next eligible queued task
- minute-level limiter is not consulted

Integration tests:

- platform webhook returns before a blocking fake runner completes
- immediate-start task sends reaction when it starts
- queued task replies with position and sends no reaction until promotion
- final output is delivered after background completion
- `/agent-status` reports running and queued state

Config tests:

- `MAX_CONCURRENT_RUNS` and `MAX_QUEUE_DEPTH` remain parsed
- rate-limit env vars are removed or ignored
- timeout env vars are removed from examples and no longer required

## Migration Notes

Deployment examples should simplify the scheduling section to:

```text
MAX_CONCURRENT_RUNS=<small integer>
MAX_QUEUE_DEPTH=<waiting task cap>
```

The old rate-limit variables should be removed from `dogbot.env.example`.
Timeout variables should also be removed from examples once implementation no
longer uses them for Claude execution.

Existing external `/v1/runs` callers may still receive a synchronous response if
that endpoint remains for tests or local tooling. The platform ingress path is
the priority and must become asynchronous.
