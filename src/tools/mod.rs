// Module declarations — `pub mod` makes these submodules visible to the parent
// crate (src/main.rs). Each file in src/tools/ is a module; the filename IS the
// module name (e.g. src/tools/calculate.rs → `tools::calculate`).
pub mod calculate;
pub mod linalg;
pub mod stats;
pub mod symbolic;

// `use` brings types into scope so we don't have to write full paths.
// `rmcp::model` contains the core MCP protocol types.
use rmcp::model::{CallToolResult, ContentBlock};
// `serde_json::json!` is a macro that builds a `serde_json::Value` from a
// Rust expression that looks like a JSON literal. It runs at compile time.
use serde_json::json;

/// Wraps any serializable value as a successful MCP tool response.
///
/// `impl serde::Serialize` is a trait bound — it means "any type that can
/// be converted to JSON." The function accepts structs, numbers, strings,
/// Vecs, etc. — anything that derives or implements `serde::Serialize`.
///
/// Returns `CallToolResult`, which is the type rmcp uses for tool call
/// responses. `CallToolResult::success(...)` constructs a success variant
/// containing a list of `ContentBlock`s (text, image, etc.).
pub fn ok_json(result: impl serde::Serialize) -> CallToolResult {
    // `json!(result)` serializes the value to a `serde_json::Value`,
    // then `.to_string()` converts it to a JSON string.
    // `vec![...]` creates a growable heap-allocated array (like a Python list).
    // `ContentBlock::text(...)` wraps a string as a text content block.
    CallToolResult::success(vec![ContentBlock::text(json!(result).to_string())])
}

/// Wraps an error message as a failed MCP tool response.
///
/// The error is returned as a *successful* `CallToolResult` with an
/// `isError: true` flag — this is intentional. MCP distinguishes between
/// "the tool ran but the computation failed" (Ok with error content) and
/// "the protocol request was malformed" (an Err at the transport level).
/// Computation errors (e.g. "empty array", "division by zero") use this
/// function so the error message is shown to the user.
pub fn err_json(error: &str) -> CallToolResult {
    // `&str` is a string slice — a borrowed reference to string data.
    // It doesn't own the data; it just points to it. This is Rust's
    // primary string type for function parameters.
    CallToolResult::error(vec![ContentBlock::text(
        json!({"error": error}).to_string(),
    )])
}
