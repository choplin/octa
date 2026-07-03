# octa

> GitHub-style collaboration, fully local.

## Overview

octa brings GitHub-style Issue and Pull Request collaboration to a local
repository, without a full web UI. It manages issues and keeps discussions per
repository, shared across multiple worktrees. The collaboration data lives in
the repository itself, and it is used from a CLI and through skills for AI
agents.

## Motivation

When developing with AI agents, multiple agents are run in parallel using
separate worktrees. Traditional team collaboration relied on GitHub Issues and
Pull Requests. octa aims to reproduce that collaboration model locally —
contained within the repository, with no external service and no full UI.

The name nods to GitHub's Octocat: octa keeps that GitHub-style collaboration
local. The octopus's many arms also echo the parallel agents and worktrees
working at once. Read as a backronym, it spells out the job too — an On-repo
Collaboration Tool for Agents. At its core, an Issue or a Pull Request is just a
numbered discussion thread.

## Notes

- Scope is specifically the Issue / Pull Request collaboration layer — not
  GitHub as a whole.
- Issues and discussions should be accessible across all worktrees of a
  repository.
- Surfaces: a CLI, plus skills to make it easy to use from AI agents. No full
  web UI.
- Storage idea (not finalized): keep data in SQLite under the main worktree's
  `.git` directory.
