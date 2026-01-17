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

1. Run tests (both host and qemu, and if available esp32) and linting (see README.md for commands)
2. Format code: `cargo fmt`
3. Ensure README.md and CLAUDE.md are up to date
4. Check if any files in the docs directory need update
5. If available, review with code review agents (code-simplifier, rust-code-guardian)
6. Fix any high priority issues identified by code review before committing
7. Add low/medium priority improvements to docs/future-work.md or TODO comments

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

## Local Device Configuration

The file `.local-devices` (gitignored) stores machine-specific device paths for two-device
LoRa testing. When this file exists, use the ports defined there:

```bash
# Source the file to get RNODE_PORT and TEST_PORT
source .local-devices

# Flash our firmware to the test device
PORT=$TEST_PORT cargo flash-esp32

# The RNode device runs official RNode firmware for Python Reticulum
```

See [docs/lora-testing-strategy.md](docs/lora-testing-strategy.md) for the full two-device
testing setup.

## Key Documentation

| Document | Contents |
|----------|----------|
| [README.md](README.md) | Prerequisites, build commands, tool locations |
| [docs/qemu-setup.md](docs/qemu-setup.md) | QEMU installation and usage |
| [docs/research-findings.md](docs/research-findings.md) | Build configuration rationale |
| [docs/implementation-guide.md](docs/implementation-guide.md) | Feature implementation plans |
| [docs/testing-strategy.md](docs/testing-strategy.md) | Testing system, `#[esp32_test]` usage |
| [docs/lora-testing-strategy.md](docs/lora-testing-strategy.md) | Two-device LoRa testing with RNode |
| [docs/scalable-routing-proposal.md](docs/scalable-routing-proposal.md) | DHT-based routing for global scale |
| [docs/future-work.md](docs/future-work.md) | Planned improvements and TODOs |
