# AGENTS.md

## Build & Test Commands

- **Build**: `cargo build`
- **Test**: `cargo test`
- **Format check**: `cargo fmt --check`
- **Lint**: `cargo clippy -- -D warnings`
- **Smoke test**: `bash scripts/smoke_test.sh`
- **All checks**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test && bash scripts/smoke_test.sh`
- **Release build**: `cargo build --release`
- **Musl build**: `cargo build --target x86_64-unknown-linux-musl`

## Conventions

### stdout is the JSON-RPC channel
NEVER write to stdout — it's the MCP protocol channel. All logging goes to stderr via `tracing_subscriber::fmt().with_writer(std::io::stderr)`.

### Error handling
- Tool computation errors → `Ok(CallToolResult::error(...))` with `{"error": "message"}` JSON body
- Protocol errors → `Err(McpError::invalid_params(...))`
- Success → `Ok(CallToolResult::success(...))` with the result JSON

### schemars must be 1.x
The `schemars` crate must be version 1.x to match rmcp 3.x's `JsonSchema` trait.

### musl rules
- All dependencies must be pure-Rust (no C build scripts)
- This enables static musl linking for Linux release binaries
- Banned crates: `symbolica`, `symbolica-integrate` (non-commercial license), `symengine` (C++ deps), `rug` (GMP/MPFR C deps)

### Tool registration
Use `#[tool_router]` and `#[tool_handler]` macros from rmcp 3.x. Do NOT use the old `#[tool(tool_box)]` pattern.

## Project Structure

```
src/
  main.rs            # Server struct, tool_router, ServerHandler, main()
  tools/
    mod.rs           # Shared helpers: ok_json(), err_json()
    calculate.rs     # Expression evaluation (fasteval)
    symbolic.rs      # Symbolic math (symb_anafis, thales, mathcore, mathhook-core)
    stats.rs         # Statistics (statrs)
    linalg.rs        # Linear algebra (nalgebra)
scripts/
  smoke_test.sh      # MCP stdio smoke test
.github/workflows/
  ci.yml             # CI checks
  release.yml        # Multi-arch release builds
```

## CAS Crate Selection

- **differentiate**: `symb_anafis` 0.8.1 (clean output, proper errors)
- **integrate**: `thales` 0.4.3 (handles 1/x, needs `.simplify()`)
- **solve_equation**: `mathcore` 0.3.1 (clean results)
- **expand**: `mathhook-core` 0.2.0 (only crate with expand)
- **factorize**: NOT PORTED (no crate passes — see UPSTREAM.md)
- **summation/plot**: Direct implementation (no CAS needed)