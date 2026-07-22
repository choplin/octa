# octa

> GitHub-style collaboration, fully local.

## Overview

octa brings GitHub-style Issue, Pull Request, and Wiki collaboration to a local
repository, without a full web UI. It manages these entities and keeps
discussions per repository, shared across multiple worktrees. The collaboration
data is kept in a single user-global store (under the XDG data directory) and
scoped logically per repository — the current repository by default, with a flag
to work across repositories. It is not committed into the repository and does
not travel with it. octa is used from a CLI and through skills for AI agents.

## Motivation

When developing with AI agents, multiple agents are run in parallel using
separate worktrees. Traditional team collaboration relied on GitHub Issues and
Pull Requests. octa aims to reproduce that collaboration model locally — held in
a local, per-repository-scoped store, with no external service and no full UI.

The name nods to GitHub's Octocat: octa keeps that GitHub-style collaboration
local. The octopus's many arms also echo the parallel agents and worktrees
working at once. Read as a backronym, it spells out the job too — an On-repo
Collaboration Tool for Agents. At its core, an Issue or a Pull Request is just a
numbered discussion thread.

## Notes

- Scope is specifically the Issue / Pull Request / Wiki collaboration layer —
  not GitHub as a whole.
- Issues, pull requests, wiki, and discussions should be accessible across all
  worktrees of a repository.
- Surfaces: a CLI, plus skills to make it easy to use from AI agents. No full
  web UI.
- Storage: data is kept in a single global SQLite database (under the XDG data
  directory) and scoped logically per repository via a `repo_id` column — the
  current repository by default, resolved from the git common directory so every
  worktree shares it, with `--repo`/`--all-repos` for cross-repository views. It
  is not committed into any repository.
