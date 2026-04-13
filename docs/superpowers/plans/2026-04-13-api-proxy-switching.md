# API Proxy Switching Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move real upstream model tokens out of the Claude container into a host-side Rust `api-proxy`, while keeping Claude Code in full-permission mode and letting the host switch upstream providers without changing container config.

**Architecture:** Add a dedicated host-local Rust `api-proxy` that exposes a stable Anthropic-compatible endpoint to the Claude container. The proxy reads host-side provider configuration, injects the active provider token and model, and forwards requests upstream. `agent-runner` and the Claude container stop receiving real model secrets and instead always point at the host proxy with a non-secret local auth token.

**Tech Stack:** Rust (`axum`, `reqwest`, `serde`, `tokio`), shell scripts, existing Docker + `agent-runner` stack

---
