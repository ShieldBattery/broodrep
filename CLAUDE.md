# CLAUDE.md

## Project Overview

This is a pure Rust library called `broodrep` for reading StarCraft 1 replay files, supporting all versions. The project uses a Cargo workspace structure with three main components:

- `broodrep/` - The core library crate for parsing StarCraft replay files
- `broodrep-cli/` - A command-line interface that provides detailed replay information
- `broodrep-wasm/` - WebAssembly bindings for browser and Node.js usage

## Development Commands

### Building
```bash
cargo build                    # Build all workspace members
cargo build -p broodrep       # Build only the library
cargo build -p broodrep-cli   # Build only the CLI
cargo build -p broodrep-wasm  # Build only the WASM bindings
```

### Testing
```bash
cargo test                     # Run all tests in workspace
cargo test -p broodrep        # Test only the library
cargo test -p broodrep-wasm   # Test WASM bindings
```

### Running
```bash
cargo run -p broodrep-cli <replay_file>  # Parse and display replay info
```

### WASM Development
```bash
# Build WASM package for browser/Node.js
cd broodrep-wasm/ && pnpm run build
```

## Architecture

- Uses Rust 2024 edition
- Is warning-free and clippy lint-free
- Supports all StarCraft replay formats (Legacy pre-1.18, Modern 1.18-1.21, Modern 1.21+)

### Core Features

**Library (`broodrep/`)**:
- Comprehensive replay parsing with format detection
- Security protections against compression bomb attacks via `DecompressionConfig`
- Supports parsing of game metadata, player information, map details, and timing
- Modular design with separate compression handling
- Rich error handling and validation

**CLI (`broodrep-cli/`)**:
- Uses `clap` for argument parsing
- Displays comprehensive replay information including:
  - Game details (format, engine, duration, start time)
  - Map information (name, dimensions)
  - Game settings (speed, type, host)
  - Player and observer lists with races and teams

**WASM (`broodrep-wasm/`)**:
- WebAssembly bindings for web and Node.js environments
- JavaScript-friendly API with serialized data structures
- Configurable decompression options for security
- Comprehensive test coverage

## Dependencies

Key dependencies across the workspace:
- `byteorder` - Binary data reading
- `chrono` - Date/time handling
- `explode` - Legacy decompression format
- `flate2` - Modern zlib decompression
- `thiserror` - Error handling
- `clap` - CLI argument parsing
- `wasm-bindgen` - WebAssembly bindings
- `serde` - Serialization for WASM interface
