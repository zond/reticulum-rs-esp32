# Agent Instructions

## Development Workflow

### Before Every Commit

1. Run tests and linting (see README.md for commands)
2. Format code: `cargo fmt`
3. Ensure README.md and CLAUDE.md are up to date
4. Check if any files in the docs directory need update
5. Review with the code-simplifier agent
6. Review with the rust-code-guardian agent

### Commit Standards

- Keep commits reasonably small and focused
- Each commit should be well-tested
- Write clear, descriptive commit messages
- Prefer many small commits over large monolithic ones

## Project Context

Key constraints to keep in mind:
- 512KB SRAM - be mindful of allocations
- ESP-IDF provides std environment (not bare metal no_std)
- BLE mesh is not upstream in reticulum-rs - needs custom implementation
- See README.md for build commands and project dependencies
