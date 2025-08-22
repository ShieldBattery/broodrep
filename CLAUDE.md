# CLAUDE.md

## Project Overview

This is a pure Rust library called `broodrep` for reading StarCraft 1 replay files, supporting all versions. The project uses a Cargo workspace structure with two main components:

- `broodrep/` - The core library crate
- `broodrep-cli/` - A command-line interface that depends on the library

## Development Commands

### Building
```bash
cargo build                    # Build all workspace members
cargo build -p broodrep       # Build only the library
cargo build -p broodrep-cli   # Build only the CLI
```

### Testing
```bash
cargo test                     # Run all tests in workspace
cargo test -p broodrep        # Test only the library
```

### Running
```bash
cargo run -p broodrep-cli      # Run the CLI application
```

## Architecture

- The project is currently in early development.
- Uses Rust 2024 edition
- Is warning-free (and clippy lint free)

The workspace is structured as a typical Rust project where the CLI tool depends on the core library for replay parsing functionality.