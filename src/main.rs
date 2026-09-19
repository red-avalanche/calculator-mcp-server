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

    /// Rounds a number to a specified number of decimal places. Supports
    /// modes: "half_up" (half away from zero, default), "half_even" (banker's
    /// rounding), "floor", "ceil", "trunc". Decimals may be negative (e.g.
    /// -2 rounds to hundreds). NOTE: f64 binary representation means some
    /// values can't be exact (e.g. 2.675 -> 2.67 at 2 decimal places).
    #[tool(name = "round_number")]
    fn round_number(
        &self,
        Parameters(req): Parameters<tools::precision::RoundNumberRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::precision::round_number_tool(req)
    }

    /// Formats a number to a specified number of significant figures (1-17).
    /// Returns a string to preserve trailing zeros (e.g. 2.500, not 2.5).
    #[tool(name = "significant_figures")]
    fn significant_figures(
        &self,
        Parameters(req): Parameters<tools::precision::SignificantFiguresRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::precision::significant_figures_tool(req)
    }

    /// Formats a number in scientific (mantissa in [1, 10), e.g. 1.23e4) or
    /// engineering (exponent multiple of 3, mantissa in [1, 1000), e.g. 12.3e3)
    /// notation. Optional sig_figs controls mantissa precision (default 6).
    #[tool(name = "format_notation")]
    fn format_notation(
        &self,
        Parameters(req): Parameters<tools::precision::FormatNotationRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::precision::format_notation_tool(req)
    }

    /// Converts a decimal number to a fraction using continued fractions.
    /// Returns numerator, denominator, mixed number string, and exact flag.
    /// Optional max_denominator for best approximation (e.g. 0.333 with
    /// max_denominator >= 3 -> 1/3).
    #[tool(name = "to_fraction")]
    fn to_fraction(
        &self,
        Parameters(req): Parameters<tools::precision::ToFractionRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::precision::to_fraction_tool(req)
    }

    /// Converts an integer between bases (2, 8, 10, 16). Accepts 0x/0o/0b
    /// prefixes for auto-detection; bare values are decimal. Returns all
    /// four representations (decimal, hex, octal, binary) plus the result in
    /// the requested base. Negative values use sign-magnitude form.
    #[tool(name = "base_convert")]
    fn base_convert(
        &self,
        Parameters(req): Parameters<tools::programmer::BaseConvertRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::programmer::base_convert_tool(req)
    }

    /// Performs bitwise operations: "and", "or", "xor", "not", "shl", "shr".
    /// Values are masked to bit_width (8, 16, 32, 64; default 64). When
    /// signed=true, values are interpreted as two's complement (e.g. 0xFF
    /// as signed 8-bit = -1). shl drops shifted-out bits; shr is arithmetic
    /// when signed, logical when unsigned. Shift count must be in
    /// [0, bit_width).
    #[tool(name = "bitwise")]
    fn bitwise(
        &self,
        Parameters(req): Parameters<tools::programmer::BitwiseRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::programmer::bitwise_tool(req)
    }

    /// Converts a value from one unit to another. Supported quantities:
    /// length (m, km, cm, mm, mi, yd, ft, in, nmi), mass (kg, g, mg, t, lb,
    /// oz, st), temperature (C, F, K), volume (l, ml, m3, gal, qt, pt, cup,
    /// floz), speed (m/s, km/h, mph, kn), area (m2, km2, ha, acre, ft2),
    /// duration (ms, s, min, h, d, wk), data (B, KB, MB, GB, TB, KiB, MiB,
    /// GiB, TiB, bit), pressure (Pa, kPa, bar, atm, psi, mmHg), energy (J,
    /// kJ, cal, kcal, Wh, kWh, BTU), power (W, kW, MW, hp), force (N, kN,
    /// lbf), angle (deg, rad, grad). KB=1000 bytes, KiB=1024 bytes.
    #[tool(name = "convert_unit")]
    fn convert_unit(
        &self,
        Parameters(req): Parameters<tools::units::ConvertUnitRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::units::convert_unit_tool(req)
    }

    /// Performs arithmetic on two quantities (add or subtract). Both
    /// operands must be the same quantity (e.g. km + m). Temperature is not
    /// supported (use convert_unit instead). Optional result_unit defaults
    /// to the left operand's unit.
    #[tool(name = "quantity_arithmetic")]
    fn quantity_arithmetic(
        &self,
        Parameters(req): Parameters<tools::units::QuantityArithmeticRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::units::quantity_arithmetic_tool(req)
    }

    /// Adds a duration to a date. Years and months use clamped calendar
    /// arithmetic (e.g. Jan 31 + 1 month -> Feb 28; Feb 29 + 1 year -> Feb 28).
    /// Weeks/days/hours/minutes/seconds use exact time arithmetic. All values
    /// may be negative. Date-only input yields date-only output unless time
    /// units (hours/minutes/seconds) are used. Accepted input formats:
    /// YYYY-MM-DD, YYYY-MM-DD HH:MM:SS, YYYY-MM-DDTHH:MM:SS.
    #[tool(name = "date_add")]
    fn date_add(
        &self,
        Parameters(req): Parameters<tools::datetime::DateAddRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::datetime::date_add_tool(req)
    }

    /// Computes the difference between two dates. Unit "auto" (default)
    /// returns a {years, months, days, total_days, sign} breakdown where
    /// years/months count only complete clamped calendar anniversaries.
    /// Other units: "years", "months", "weeks", "days", "hours", "minutes",
    /// "seconds" (all signed). For finer granularity than whole months/years,
    /// use days or hours. Datetime inputs also include total_seconds.
    #[tool(name = "date_diff")]
    fn date_diff(
        &self,
        Parameters(req): Parameters<tools::datetime::DateDiffRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::datetime::date_diff_tool(req)
    }

    /// Returns calendar information about a date: weekday name, ISO week
    /// number, day of year, days in month, leap year flag, and days
    /// remaining in the year.
    #[tool(name = "date_info")]
    fn date_info(
        &self,
        Parameters(req): Parameters<tools::datetime::DateInfoRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::datetime::date_info_tool(req)
    }

    /// Computes percentage-related calculations. Modes: "percent_of" (b% of
    /// a), "what_percent" (a is what % of b), "percent_change" (from a to b),
    /// "increase" (a increased by b%), "decrease" (a decreased by b%).
    #[tool(name = "percentage")]
    fn percentage(
        &self,
        Parameters(req): Parameters<tools::finance::PercentageRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::finance::percentage_tool(req)
    }

    /// Computes compound interest: future value, total interest, and
    /// effective annual rate. compounds_per_year=0 means continuous
    /// compounding (Pe^rt). Default compounds_per_year is 12.
    #[tool(name = "compound_interest")]
    fn compound_interest(
        &self,
        Parameters(req): Parameters<tools::finance::CompoundInterestRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::finance::compound_interest_tool(req)
    }

    /// Computes loan payment (amortization): payment amount, number of
    /// payments, total paid, and total interest. payments_per_year defaults
    /// to 12 (monthly). Zero-interest loans are handled (payment = P/n).
    #[tool(name = "loan_payment")]
    fn loan_payment(
        &self,
        Parameters(req): Parameters<tools::finance::LoanPaymentRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::finance::loan_payment_tool(req)
    }

    /// Computes net present value (NPV). cashflows[0] is at t=0 (not
    /// discounted). NOTE: this differs from Excel's NPV, which discounts
    /// the first cash flow by one period.
    #[tool(name = "net_present_value")]
    fn net_present_value(
        &self,
        Parameters(req): Parameters<tools::finance::NetPresentValueRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::finance::net_present_value_tool(req)
    }

    /// Computes the internal rate of return (IRR) using Newton-Raphson with
    /// a bisection fallback. Cashflows must have at least one sign change.
    /// Returns the rate nearest the initial guess (default 10%). Multiple
    /// IRRs are possible for non-conventional cashflows.
    #[tool(name = "internal_rate_of_return")]
    fn internal_rate_of_return(
        &self,
        Parameters(req): Parameters<tools::finance::IrrRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::finance::internal_rate_of_return_tool(req)
    }

    /// Computes return on investment (ROI): roi_pct = (gain - cost) / cost
    /// * 100, and net_gain = gain - cost. Cost must not be zero.
    #[tool(name = "return_on_investment")]
    fn return_on_investment(
        &self,
        Parameters(req): Parameters<tools::finance::RoiRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::finance::return_on_investment_tool(req)
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
        .with_instructions("Mathematical calculator: symbolic math, statistics, matrices, units, dates, programmer/integer, finance, precision.")
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
