<!-- GSD:project-start source:PROJECT.md -->
## Project

**Hephaestus** (formerly Blacksmith)

A unified ONNX model inference runtime in Rust. Single container that loads, serves, and manages any ONNX-compatible model — classifiers, embeddings, seq2seq, token classifiers. Pairs with a Python Forge service for converting non-ONNX HuggingFace models.

**Core Value:** A single Rust binary that takes a model name, resolves it to ONNX files (S3 cache → HuggingFace → Forge conversion), and serves inference with full pre/post-processing — replacing every per-model Python runtime in the Minerva cluster.

### Constraints

- **Language**: Rust only, 2024 edition & workspace resolver 3
- **Rules compliance**: Every file must adhere to all rules in `rules/`.
- **Code Convention**: Traits must follow John Ousterhout's "deep module" principle — expose a minimal interface (1-3 methods) that hides significant implementation complexity. Callers should never need to understand internals. Prefer one `process()` over separate `tokenize()`, `infer()`, `decode()`. Prefer one `resolve()` over separate `check_s3()`, `check_hf()`, `call_forge()`. If a trait has more than 3 required methods, it is probably too shallow — reconsider the abstraction boundary.

<!-- GSD:project-end -->

<!-- GSD:stack-start source:STACK.md -->
## Technology Stack

Technology stack not yet documented. Will populate after codebase mapping or first phase.
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->
