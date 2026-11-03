# AGENTS.md

## Operating principles

- Working code only. Plausibility is not correctness; verify before reporting
  done.
- Never fabricate file paths, APIs, commit hashes, command output, or test
  results. Read the file, run the command, or say what is unknown.
- Say when a premise appears wrong before implementing around it.
- Ask before proceeding only when a request has multiple plausible
  interpretations and the choice materially affects the result.
- Touch only what the task requires. Avoid drive-by refactors, formatting, or
  cleanup.

## Command execution

- Prefer running code, tests, linters, and type checks over guessing.
- Run recurring commands (tests, lint, run, mockup rendering) through mise
  tasks in `.mise.toml`. When a command is needed repeatedly and has no task,
  add one.
- Read complete errors, logs, and stack traces before fixing them.

## Before editing

- State the plan or success criteria before editing. For non-trivial work,
  include the verification you expect to run.
- Read the files you will touch and the nearby callers, consumers, or docs that
  define their behavior.
- Match existing project patterns, naming, layout, and style even if a different
  approach would be appealing in a new project.
- Resolve ambiguity by reading code or running commands when practical; surface
  assumptions out loud when they affect the result.

## Editing

- Keep credentials, tokens, sessions, history, caches, logs, and runtime
  databases out of this repository.
- Put reusable workflows in `.agents/skills/<name>/SKILL.md`.
- Put project maintenance instructions in this file.
- Use the minimum code or documentation change that solves the stated problem.
- Do not add speculative features, abstractions, configurability, or hooks.
- Do clean up orphans created by your own change, such as unused imports or
  obsolete helper functions.
- Do not delete pre-existing dead code unless asked; mention it in the summary
  if it matters.

## Committing

- Commit only changes you made, one logical change per commit. Report
  unrelated uncommitted or untracked changes instead of committing them.

## Documentation routing

Project docs live in `spec/` and `plan/` and are not loaded automatically.
Read only what the task needs:

- `spec/index.md` first, then only the spec files the task touches. The spec
  describes the agreed behavior; a project starts by writing a small spec.
- `plan/changes/` for changes in progress (every file except `template.md`).
- `plan/backlog.md` for prioritized bugs, features, and chores.
- `plan/roadmap.md` for initiatives too large for one change.

Git is the history: do not keep archives of finished work in `spec/` or
`plan/`.

## Changes

- Small fix: write the failing test, fix, commit. No change file.
- Work spanning several steps or sessions: copy `plan/changes/template.md` to
  `plan/changes/<short-name>.md`, edit the spec directly, write tests for the
  requirement IDs touched, then implement.
- A change is done when the tests for its requirement IDs pass. Delete its
  change file and its backlog item in the same commit.

## Testing

This project is developed test-first. Tests are the deliverable, not an
afterthought. Each test references the requirement ID it verifies.

## Verification

- Run the smallest meaningful verification during iteration and the requested or
  relevant final verification before reporting done.
- If verification fails, fix the cause instead of weakening the check.
- For UI or visual changes, verify visually with screenshots or equivalent
  rendered output.

## Maintenance

- Keep this file short enough to follow. Add rules only when they prevent a real
  repeat mistake or document durable project behavior.
- When the user corrects an approach, tighten the relevant rule instead of
  appending a vague warning.
