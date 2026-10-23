---
type: Architecture Decision
title: Question box through OpenRouter
description: Why the question box uses a hosted language model through OpenRouter's OpenAI-compatible API, with read-only tools, instead of a local model or a vendor SDK.
tags: [decision, language-model, openrouter]
generated: { by: "human:abenamer", at: "2026-10-23T09:00:00Z" }
---

# ADR 0005 — Question box through OpenRouter

## Context

The maintainer wants a StatMuse-like box that answers questions about the stored seasons in plain language. The application is self-hosted on one computer, often without a GPU. A local model (Qwen through Ollama) would need a GPU-dependent install and would answer slowly on a CPU; a vendor SDK ties the application to one provider.

## Decision

* One port, `LanguageModel`, with one method: send a conversation and tool definitions, receive text or tool calls. One adapter, `OpenRouterModel`, speaks the OpenAI-compatible chat API, so the base URL and the model are configuration and any compatible service works.
* The model reads the database through seven read-only tools that the use case runs against the existing store ports. The model has no write path and no SQL.
* The box answers only questions about the stored data. The system prompt forbids everything else, and the use case replaces an answer without a lookup by a fixed refusal, so the model's own knowledge never reaches the page.
* Every question is bounded: model calls, tool calls, result size, time and concurrency.
* The default model is a small, cheap hosted model; the operator chooses another by configuration.

## Consequences

* The question and the tool results leave the computer. The documentation says so, and the box is off until a key is set.
* The feature costs tokens per question, shown under each answer, instead of local hardware.
* A new tool is a function in `application::ask` and a JSON Schema; no change to the adapter.
* Answers depend on the model reading the tool results correctly. The lookup list under the answer makes the source of a number visible.

## Alternatives

* A local model through Ollama was rejected for now: the install depends on the GPU and the CPU path is slow. The port allows an Ollama adapter later, since Ollama speaks the same API.
* A vendor SDK was rejected: it ties the application to one provider for one small feature.
* Free text to SQL was rejected: it opens a write and a cost path that the fixed tools do not.
