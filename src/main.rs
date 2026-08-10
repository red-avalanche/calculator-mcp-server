// `use` brings external crate items into scope. rmcp is the official Rust MCP
// SDK (Model Context Protocol). The braces destructure specific items from
// the rmcp crate's module tree.
use rmcp::{
    // `Parameters<T>` is a wrapper type that extracts and deserializes the
    // JSON arguments from an incoming tool call into a Rust struct `T`.
    handler::server::wrapper::Parameters,
    // `CallToolResult` is the return type for tool calls (success or error).
    // `ContentBlock` is an enum representing content in a response (text,
    // image, etc.). `ContentBlock::text(...)` creates a text variant.
    model::{CallToolResult, ContentBlock},
    // `schemars` is re-exported by rmcp — it generates JSON Schemas from
    // Rust types, which MCP clients use to understand what arguments a
    // tool accepts. The `JsonSchema` derive macro comes from here.
    schemars,
    // `tool`, `tool_handler`, `tool_router` are attribute macros (proc-macros)
    // that generate boilerplate code at compile time:
    //   - `#[tool(name = "...")]` registers a method as an MCP tool
    //   - `#[tool_router]` generates a ToolRouter from all #[tool] methods
    //   - `#[tool_handler]` implements the ServerHandler trait
    tool,
    tool_handler,
    tool_router,
    // `stdio()` returns a transport that reads JSON-RPC from stdin and
    // writes JSON-RPC to stdout. This is the only transport we support.
    transport::stdio,
    // `ServerHandler` is the trait a server must implement to handle MCP
    // requests. `ServiceExt` provides the `.serve()` method that starts
    // the server.
    ServerHandler,
    ServiceExt,
};

// `mod tools;` declares a module. Rust looks for `src/tools/mod.rs` (or
// `src/tools.rs`). This makes `tools::calculate`, `tools::symbolic`, etc.
// available throughout this file.
mod tools;

// `#[derive(...)]` automatically implements the listed traits for this
// struct. Each trait serves a purpose:
//   - `Debug`: allows formatting with `{:?}` (for logging/error messages)
//   - `serde::Deserialize`: allows deserializing from JSON (needed by rmcp
//     to parse incoming tool arguments)
//   - `schemars::JsonSchema`: generates a JSON Schema for this struct,
//     which MCP clients see as the tool's input specification
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
// An empty struct — the version tool takes no arguments. In Rust, empty
// structs are written as `struct Name {}` or `struct Name;`.
struct VersionRequest {}

// `Clone` allows creating a copy of the value via `.clone()`. rmcp
// requires the server type to be `Clone` so it can be shared across
// async tasks.
#[derive(Clone)]
struct Calculator {}

// `#[tool_router]` is a proc-macro that transforms this `impl` block. It
// generates a `ToolRouter` — a lookup table that maps tool names to their
// handler methods. Every method annotated with `#[tool(name = "...")]`
// inside this block becomes a registered MCP tool.
#[tool_router]
impl Calculator {
    /// Creates a new Calculator instance.
    ///
    /// The `#[tool_router]` macro also generates a `tool_router()` method
    /// (not shown here) that builds the router from all `#[tool]`-annotated
    /// methods. We call `Self {}` because `Calculator` has no fields.
    fn new() -> Self {
        Self {}
    }

    /// Returns the version of the calculator server.
    #[tool(name = "version")]
    // `&self` is a shared reference to the Calculator instance — it's like
    // `self` in Python but borrowed (not owned). The `&` means the method
    // doesn't take ownership and can't mutate the struct.
    // `_req` is prefixed with `_` to suppress the "unused variable" warning;
    // the version tool ignores its arguments.
    fn version(&self, _req: Parameters<VersionRequest>) -> Result<CallToolResult, rmcp::ErrorData> {
        // `Ok(...)` wraps the value in a `Result::Ok` variant. `Result` is
        // Rust's error-handling type — it's either `Ok(value)` or `Err(error)`.
        // `rmcp::ErrorData` is the protocol-level error type (used for
        // malformed requests, not computation errors).
        Ok(CallToolResult::success(vec![ContentBlock::text(
            // `env!("CARGO_PKG_VERSION")` is a compile-time macro that reads
            // the `version` field from Cargo.toml. It returns a `&'static str`.
            // `json!({...})` builds a `serde_json::Value` at compile time.
            serde_json::json!({"result": env!("CARGO_PKG_VERSION")}).to_string(),
        )]))
    }

    /// Evaluates a mathematical expression (e.g. "2 + 3 * 4", "sin(pi/2)").
    #[tool(name = "calculate")]
    fn calculate(
        &self,
        // `Parameters(req)` destructures the wrapper: rmcp deserializes the
        // incoming JSON arguments into `CalculateRequest`, then `Parameters`
        // wraps it. The pattern `Parameters(req)` extracts the inner struct.
        Parameters(req): Parameters<tools::calculate::CalculateRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Delegates to the pure function in the tools module. The `?`
        // operator would propagate an `Err`, but here we return the result
        // directly since `calculate()` already returns the right type.
        tools::calculate::calculate(&req.expression)
    }

    /// Computes the symbolic derivative of an expression with respect to a
    /// variable (default "x"). Uses `**` for powers in input; output uses `^`.
    #[tool(name = "differentiate")]
    fn differentiate(
        &self,
        Parameters(req): Parameters<tools::symbolic::DifferentiateRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::symbolic::differentiate_tool(req)
    }

    /// Computes the symbolic indefinite integral of an expression. Output
    /// uses `ln()` for natural log (not `log()`), and `^` for powers.
    #[tool(name = "integrate")]
    fn integrate(
        &self,
        Parameters(req): Parameters<tools::symbolic::IntegrateRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::symbolic::integrate_tool(req)
    }

    /// Solves an algebraic equation for x. The equation must contain exactly
    /// one `=` sign (e.g. "x**2 - 5*x + 6 = 0").
    #[tool(name = "solve_equation")]
    fn solve_equation(
        &self,
        Parameters(req): Parameters<tools::symbolic::SolveEquationRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::symbolic::solve_equation_tool(req)
    }

    /// Expands an algebraic expression (e.g. "(x + 1)**2" → "1 + x^2 + 2*x").
    #[tool(name = "expand")]
    fn expand(
        &self,
        Parameters(req): Parameters<tools::symbolic::ExpandRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::symbolic::expand_tool(req)
    }

    /// Computes the summation of an expression over an integer range
    /// (e.g. sum of x**2 from 0 to 10 = 385).
    #[tool(name = "summation")]
    fn summation(
        &self,
        Parameters(req): Parameters<tools::symbolic::SummationRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::symbolic::summation_tool(req)
    }

    /// Plots a function y = f(x) over a range and returns a PNG image.
    #[tool(name = "plot_function")]
    fn plot_function(
        &self,
        Parameters(req): Parameters<tools::symbolic::PlotRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::symbolic::plot_tool(req)
    }

    /// Computes the arithmetic mean of a list of numbers.
    #[tool(name = "mean")]
    fn mean(
        &self,
        Parameters(req): Parameters<tools::stats::MeanRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::stats::mean_tool(req)
    }

    /// Computes the population variance (ddof=0, matching numpy's default).
    #[tool(name = "variance")]
    fn variance(
        &self,
        Parameters(req): Parameters<tools::stats::VarianceRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::stats::variance_tool(req)
    }

    /// Computes the population standard deviation (ddof=0).
    #[tool(name = "standard_deviation")]
    fn standard_deviation(
        &self,
        Parameters(req): Parameters<tools::stats::StandardDeviationRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::stats::standard_deviation_tool(req)
    }

    /// Computes the median of a list of numbers.
    #[tool(name = "median")]
    fn median(
        &self,
        Parameters(req): Parameters<tools::stats::MedianRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::stats::median_tool(req)
    }

    /// Computes the mode (most frequent value). Ties are broken by
    /// returning the smallest value.
    #[tool(name = "mode")]
    fn mode(
        &self,
        Parameters(req): Parameters<tools::stats::ModeRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::stats::mode_tool(req)
    }

    /// Computes the Pearson correlation coefficient between two arrays.
    #[tool(name = "correlation_coefficient")]
    fn correlation_coefficient(
        &self,
        Parameters(req): Parameters<tools::stats::CorrelationRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::stats::correlation_tool(req)
    }

    /// Performs linear regression on a list of (x, y) points.
    /// Returns slope and intercept.
    #[tool(name = "linear_regression")]
    fn linear_regression(
        &self,
        Parameters(req): Parameters<tools::stats::LinearRegressionRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::stats::linear_regression_tool(req)
    }

    /// Computes the confidence interval for the mean of a dataset
    /// using the t-distribution. Default confidence level is 0.95.
    #[tool(name = "confidence_interval")]
    fn confidence_interval(
        &self,
        Parameters(req): Parameters<tools::stats::ConfidenceIntervalRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::stats::confidence_interval_tool(req)
    }

    /// Adds two matrices element-wise.
    #[tool(name = "matrix_addition")]
    fn matrix_addition(
        &self,
        Parameters(req): Parameters<tools::linalg::MatrixAdditionRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::linalg::matrix_addition_tool(req)
    }

    /// Multiplies two matrices (matrix product, not element-wise).
    #[tool(name = "matrix_multiplication")]
    fn matrix_multiplication(
        &self,
        Parameters(req): Parameters<tools::linalg::MatrixMultiplicationRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::linalg::matrix_multiplication_tool(req)
    }

    /// Transposes a matrix (swaps rows and columns).
    #[tool(name = "matrix_transpose")]
    fn matrix_transpose(
        &self,
        Parameters(req): Parameters<tools::linalg::MatrixTransposeRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::linalg::matrix_transpose_tool(req)
    }

    /// Computes the determinant of a square matrix. Result is rounded
    /// to 10 decimal places (matching the Python server's behavior).
    #[tool(name = "matrix_determinant")]
    fn matrix_determinant(
        &self,
        Parameters(req): Parameters<tools::linalg::MatrixDeterminantRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::linalg::matrix_determinant_tool(req)
    }

    /// Computes the dot product of two vectors.
    #[tool(name = "vector_dot_product")]
    fn vector_dot_product(
        &self,
        Parameters(req): Parameters<tools::linalg::VectorDotProductRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::linalg::vector_dot_product_tool(req)
    }

    /// Computes the cross product of two 3D vectors. Returns an error
    /// if either vector is not exactly 3-dimensional.
    #[tool(name = "vector_cross_product")]
    fn vector_cross_product(
        &self,
        Parameters(req): Parameters<tools::linalg::VectorCrossProductRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::linalg::vector_cross_product_tool(req)
    }

    /// Computes the magnitude (Euclidean norm) of a vector.
    #[tool(name = "vector_magnitude")]
    fn vector_magnitude(
        &self,
        Parameters(req): Parameters<tools::linalg::VectorMagnitudeRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::linalg::vector_magnitude_tool(req)
    }
}

// `#[tool_handler]` is a proc-macro that implements the `ServerHandler` trait
// for `Calculator`. It generates a `call_tool` method that delegates to the
// `ToolRouter` (built by `#[tool_router]` above), dispatching incoming tool
// calls to the correct `#[tool]`-annotated method.
#[tool_handler]
impl ServerHandler for Calculator {
    /// Returns server metadata that MCP clients query during the
    /// initialization handshake. This tells the client what capabilities
    /// the server supports (tools, resources, prompts) and provides
    /// human-readable instructions.
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(
            // Enable tool capabilities — this server provides tools.
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        // `from_build_env()` reads the crate name and version from Cargo.toml
        // at compile time, so the server identifies itself correctly.
        .with_server_info(rmcp::model::Implementation::from_build_env())
        // Instructions shown to the LLM client when it connects. This helps
        // the LLM understand what the server can do.
        .with_instructions("Mathematical calculator: symbolic math, statistics, matrices.")
    }
}

// `#[tokio::main]` is a proc-macro that transforms `async fn main()` into a
// synchronous `fn main()` that creates a Tokio async runtime and blocks on
// the async body. Tokio is Rust's most popular async runtime (like asyncio
// in Python). Without this macro, `main` can't be `async`.
#[tokio::main]
// `async fn main() -> anyhow::Result<()>` means:
//   - `async`: the function is asynchronous (can use `.await`)
//   - `anyhow::Result<()>`: returns a Result whose Ok value is `()` (the unit
//     type, like `void` or `None` in Python) and whose error type is
//     `anyhow::Error` (a generic error type that can wrap any error).
// The `?` operator (used below) automatically returns `Err(e)` if a
// `Result` is `Err`, converting the error via `From`.
async fn main() -> anyhow::Result<()> {
    // Process command-line arguments. `std::env::args()` returns an iterator
    // over the arguments. The first element is always the program name, so
    // `.skip(1)` skips it. We accept `--stdio` (ignored — stdio is the only
    // transport), `--help`, and `--version`. Unknown args cause exit(2).
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--stdio" => {} // Accepted for Python compatibility; stdio is the only transport.
            "--help" => {
                // `eprintln!` prints to stderr (NOT stdout — stdout is the
                // JSON-RPC channel and must never be polluted).
                eprintln!("Usage: calculator-mcp-server [--stdio] [--help] [--version]");
                std::process::exit(0);
            }
            "--version" => {
                eprintln!("{}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", arg);
                eprintln!("Usage: calculator-mcp-server [--stdio] [--help] [--version]");
                std::process::exit(2);
            }
        }
    }

    // Initialize the tracing logging subscriber. Tracing is Rust's
    // structured logging framework (like Python's `logging` module).
    // CRITICAL: logs go to stderr, NOT stdout. stdout is the JSON-RPC
    // transport — any stray output on stdout corrupts the MCP protocol.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr) // Route logs to stderr.
        .with_ansi(false) // Disable ANSI color codes (they corrupt JSON-RPC on stdout).
        .init(); // Install this subscriber as the global default.

    // Start the MCP server. `Calculator::new()` creates the server instance.
    // `.serve(stdio())` consumes the server and returns a running service
    // handle. `stdio()` is the transport (stdin for input, stdout for output).
    // `.await` yields control to the async runtime until the service is ready.
    // `?` propagates any startup error as an `Err` return from `main`.
    let service = Calculator::new().serve(stdio()).await?;

    // Block until the server shuts down (stdin closes or a fatal error).
    // `.waiting()` returns a future that completes when the service stops.
    // `.await` suspends the function until that future is ready.
    service.waiting().await?;
    // Return `Ok(())` to indicate successful shutdown. `()` is the unit type
    // (like `void` — it has exactly one value and carries no data).
    Ok(())
}
