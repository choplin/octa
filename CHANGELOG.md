# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-05

### Added

- Local, repository-scoped Issue tracking backed by SQLite.
- Issue comments, configurable states and labels, blocking and related relations, parent-child grouping, and atomic leases.
- Finite Projects, ordered Milestones, and repository selection across linked worktrees.
- Human-readable and JSON CLI output, read-only GraphQL queries, and a read-only Issue TUI.
- Issue and comment input from files or standard input, plus logical database backup and integrity-checked restore scripts.
- Installation through the `octa-cli` crate and checksummed GitHub Release binaries for Apple Silicon macOS, Intel macOS, and x86_64 Linux.

[Unreleased]: https://github.com/choplin/octa/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/choplin/octa/releases/tag/v0.1.0
