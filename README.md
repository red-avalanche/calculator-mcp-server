# Mathematical Calculator MCP Server

A single-binary Rust MCP (Model Context Protocol) server that provides mathematical calculation capabilities, including symbolic math, statistics, and matrix operations.

## Features

- **Basic Calculations**: Evaluate mathematical expressions safely (`calculate`)
- **Symbolic Mathematics**:
  - Solve equations (`solve_equation`)
  - Calculate derivatives (`differentiate`)
  - Compute integrals (`integrate`)
  - Expand expressions (`expand`)
  - Summation over ranges (`summation`)
  - Plot functions (`plot_function`)
- **Statistical Analysis**:
  - Mean, median, mode, variance, standard deviation
  - Correlation coefficient, linear regression
  - Confidence intervals
- **Matrix & Vector Operations**:
  - Matrix addition, multiplication, transpose, determinant
  - Vector dot product, cross product, magnitude

> **Note**: `factorize` is not yet ported — see [UPSTREAM.md](UPSTREAM.md) for details.

## Installation

### Linux/macOS

```bash
# Download the latest release
curl -fsSL https://github.com/huhabla/calculator-mcp-server/releases/latest/download/calculator-mcp-server-v1.0.0-x86_64-unknown-linux-musl.tar.gz | tar xz
sudo install calculator-mcp-server-v1.0.0-x86_64-unknown-linux-musl/calculator-mcp-server /usr/local/bin/

# Verify checksum
sha256sum -c SHA256SUMS.txt
```

### Windows (PowerShell)

```powershell
Invoke-WebRequest -Uri "https://github.com/huhabla/calculator-mcp-server/releases/latest/download/calculator-mcp-server-v1.0.0-x86_64-pc-windows-msvc.zip" -OutFile "calculator-mcp-server.zip"
Expand-Archive calculator-mcp-server.zip -DestinationPath .
```

### macOS

For macOS, download the universal binary:
```bash
curl -fsSL https://github.com/huhabla/calculator-mcp-server/releases/latest/download/calculator-mcp-server-v1.0.0-universal-apple-darwin.tar.gz | tar xz
sudo install calculator-mcp-server-v1.0.0-universal-apple-darwin/calculator-mcp-server /usr/local/bin/
```

If you downloaded via browser and get a Gatekeeper warning:
```bash
xattr -d com.apple.quarantine /usr/local/bin/calculator-mcp-server
```

## Integration with Claude Desktop

Add the server to your Claude Desktop configuration file:

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`  
**Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "calculator": {
      "command": "/usr/local/bin/calculator-mcp-server",
      "args": ["--stdio"]
    }
  }
}
```

The `--stdio` flag is accepted for compatibility but not required (stdio is the only transport).

## Development

### Prerequisites

- Rust stable (1.88+)

### Building

```bash
cargo build
cargo test
```

### Running the smoke test

```bash
bash scripts/smoke_test.sh
```

### Interactive debugging

```bash
npx @modelcontextprotocol/inspector ./target/debug/calculator-mcp-server
```

### Release process

1. Bump the version in `Cargo.toml`
2. Commit and tag: `git tag vX.Y.Z`
3. Push the tag: `git push origin vX.Y.Z`
4. The GitHub Actions release workflow will build and publish binaries automatically.

## License

This project is licensed under the MIT License — see the LICENSE file for details.