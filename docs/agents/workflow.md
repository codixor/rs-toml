# Task workflow

## Task brief (start of every L1–L3 task)

Write it at the top of `docs/tasks/<id>.md` before any code. If a field
cannot be filled from the request, ask one clear question.

```markdown
# <id>: <title>
Level: L0 | L1 | L2 | L3
Goal: one sentence, the outcome, not the method
Acceptance criteria:
- [ ] observable result 1 (test, command output, behaviour)
- [ ] observable result 2
Out of scope: what not to touch
Likely files: paths
Risks: public API, data, security, performance
```

## Task log (L2–L3)

```markdown
## Plan
## Progress
- [x] step done — evidence (test name, command output)
- [ ] next step
## Decisions
- decision — why
## Pre-CI gate
<gate report>
```

## Lessons

When the human corrects you, or the same mistake happens twice, add one line
to `docs/agent-lessons.md`:

```
- <rule> — <why, with the incident or file>
```

## Self-review before handing to Flash

- [ ] no new `unwrap()` / `expect()` / `panic!` in production code
- [ ] no needless `clone()`; borrows where clear
- [ ] no blocking calls (`std::fs`, `std::thread::sleep`, sync locks held across `.await`) in async code
- [ ] errors carry context (what failed, with which input)
- [ ] every `unsafe` block has a `// SAFETY:` comment
- [ ] new public items have `///` docs
- [ ] tests cover the change; bug fixes have a regression test
- [ ] no secrets, debug prints or leftover TODOs
- [ ] task brief acceptance criteria all met
