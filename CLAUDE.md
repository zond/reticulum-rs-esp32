# Agent Instructions

## Development Workflow

Run the `date` command to understand what date it is today.

### Documentation ideals

README.md contains all project overview, build instructions, and basic know how necessary
to initially understand what the project is about.

CLAUDE.md contains all information an agent needs to work in the project that doesn't fit
in README.md.

docs/* contains specialized knowledge, TODO files, research results, future ideas, and
anything else that is documents but don't fit in README.md or CLAUDE.md.

README.md and CLAUDE.md should refer to docs/* files when useful.

### Before Every Commit

1. Run tests (both host and qemu, and if available device) and linting (see README.md for commands)
2. Format code: `cargo fmt`
3. Ensure README.md and CLAUDE.md are up to date
4. Check if any files in the docs directory need update
5. Review with the code-simplifier:code-simplifier agent
6. Review with the rust-code-guardian agent
7. If the code-simplifier:code-simplifier or rust-code-guardian identifies reasonable
   things to fix in the future, please add to a docs file or TODO
   comments.

### Commit Standards

- Keep commits reasonably small and focused
- Each commit should be well-tested
- Write clear, descriptive commit messages
- Prefer many small commits over large monolithic ones
- Make sure that no code commited in the project refers to absolute paths, or paths
  outside the project directory. Documentation about how to install or manage deps
  may provide examples or instructions that refer to absolute paths or paths outside
  the project directory.
- Never use Rust unsafe code unless it's really necessary, and very well motivated
  in comments.

## Project Context

Key constraints to keep in mind:
- 512KB SRAM - be mindful of allocations
- ESP-IDF provides std environment (not bare metal no_std)
- BLE mesh is not upstream in reticulum-rs - needs custom implementation

For build commands, tool locations, and setup instructions, see [README.md](README.md).

## Key Documentation

| Document | Contents |
|----------|----------|
| [README.md](README.md) | Prerequisites, build commands, tool locations |
| [docs/qemu-setup.md](docs/qemu-setup.md) | QEMU installation and usage |
| [docs/research-findings.md](docs/research-findings.md) | Build configuration rationale |
| [docs/implementation-guide.md](docs/implementation-guide.md) | Feature implementation plans |
| [docs/testing-strategy.md](docs/testing-strategy.md) | Testing system, `#[esp32_test]` usage |
| [docs/scalable-routing-proposal.md](docs/scalable-routing-proposal.md) | DHT-based routing for global scale |
| [docs/future-work.md](docs/future-work.md) | Planned improvements and TODOs |
