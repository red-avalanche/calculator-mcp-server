# Mathematical Calculator MCP Server

A single-binary Rust MCP (Model Context Protocol) server that provides mathematical calculation capabilities, including symbolic math, statistics, matrix operations, unit conversion, date/time calculations, programmer/integer operations, financial calculations, and precision utilities.

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
- **Unit Conversion & Quantity Arithmetic**:
  - Convert between units (`convert_unit`): length, mass, temperature,
    volume, speed, area, duration, data size (KB vs KiB), pressure, energy,
    power, force, angle
  - Arithmetic on quantities (`quantity_arithmetic`): e.g. 2 km + 500 m
- **Date/Time Calculations** (no timezones):
  - Add/subtract durations with clamped calendar arithmetic (`date_add`)
  - Date differences in whole months/years or exact units (`date_diff`)
  - Calendar info: weekday, ISO week, day of year, leap year (`date_info`)
- **Programmer/Integer Calculations**:
  - Base conversion: hex, octal, binary, decimal (`base_convert`)
  - Bitwise operations with bit-width and signed/unsigned support (`bitwise`)
- **Percentage & Financial Calculations**:
  - Percentage operations (`percentage`)
  - Compound interest (`compound_interest`)
  - Loan payment/amortization (`loan_payment`)
  - Net present value (`net_present_value`)
  - Internal rate of return (`internal_rate_of_return`)
  - Return on investment (`return_on_investment`)
- **Precision & Representation Utilities**:
  - Rounding with multiple modes (`round_number`)
  - Significant figures (`significant_figures`)
  - Scientific/engineering notation (`format_notation`)
  - Decimal-to-fraction conversion (`to_fraction`)

> **Note**: `factorize` is not yet ported — see [UPSTREAM.md](UPSTREAM.md) for details.

## Installation

### Linux/macOS

```bash
# Download the latest release
curl -fsSL https://github.com/huhabla/calculator-mcp-server/releases/latest/download/calculator-mcp-server-v1.1.0-x86_64-unknown-linux-musl.tar.gz | tar xz
sudo install calculator-mcp-server-v1.1.0-x86_64-unknown-linux-musl/calculator-mcp-server /usr/local/bin/

# Verify checksum
sha256sum -c SHA256SUMS.txt
```

### Windows (PowerShell)

```powershell
Invoke-WebRequest -Uri "https://github.com/huhabla/calculator-mcp-server/releases/latest/download/calculator-mcp-server-v1.1.0-x86_64-pc-windows-msvc.zip" -OutFile "calculator-mcp-server.zip"
Expand-Archive calculator-mcp-server.zip -DestinationPath .
```

### macOS

For macOS, download the universal binary:
```bash
curl -fsSL https://github.com/huhabla/calculator-mcp-server/releases/latest/download/calculator-mcp-server-v1.1.0-universal-apple-darwin.tar.gz | tar xz
sudo install calculator-mcp-server-v1.1.0-universal-apple-darwin/calculator-mcp-server /usr/local/bin/
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