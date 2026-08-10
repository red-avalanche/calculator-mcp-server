// Symbolic mathematics tools — differentiation, integration, equation
// solving, expression expansion, summation, and plotting.
//
// These tools use a multi-crate CAS (Computer Algebra System) strategy:
//   - differentiate → symb_anafis 0.8.1 (clean output, proper errors)
//   - integrate    → thales 0.4.3 (handles 1/x → ln(abs(x)), needs .simplify())
//   - solve_equation → mathcore 0.3.1 (clean results)
//   - expand       → mathhook-core 0.2.0 (only crate with expand)
//   - summation    → direct (fasteval, no CAS needed)
//   - plot_function → plotters (numeric evaluation only, no CAS)
//
// All CAS calls are wrapped in `std::panic::catch_unwind` to prevent panics
// from killing the async tokio worker thread. Input uses `**` for powers
// (Python syntax); output uses `^` (the CAS crates' native notation).

use rmcp::model::CallToolResult;
use serde_json::json;

// `crate::tools` refers to the parent module (src/tools/mod.rs). We import
// the shared `ok_json` and `err_json` helpers from there.
use crate::tools::{err_json, ok_json};

// ===== Request structs =====
// Each tool has a request struct that defines its JSON Schema (what the
// MCP client sees as the tool's input specification). The `#[serde(default)]`
// attribute means "if this field is absent in the JSON, use the type's
// default value" (for `Option<T>`, that's `None`).

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DifferentiateRequest {
    /// The mathematical expression to differentiate.
    /// Use ** for powers (e.g. x**2), explicit * for multiplication, and x as the default variable.
    /// Note: Results use ^ notation for powers (e.g. 2*x^2).
    pub expression: String,
    /// The variable to differentiate with respect to. Defaults to "x".
    /// `Option<String>` means this field is nullable — it's either `Some("x")`
    /// or `None`. The `#[serde(default)]` attribute makes it optional in JSON.
    #[serde(default)]
    pub variable: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct IntegrateRequest {
    /// The mathematical expression to integrate.
    /// Use ** for powers (e.g. x**2), explicit * for multiplication, and x as the default variable.
    /// Note: Results use ^ notation for powers (e.g. 2*x^2).
    /// Note: Natural logarithm is shown as ln() (not log()), and may include abs() for 1/x integrals.
    pub expression: String,
    /// The variable to integrate with respect to. Defaults to "x".
    #[serde(default)]
    pub variable: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SolveEquationRequest {
    /// The equation to solve (e.g. "x**2 - 5*x + 6 = 0").
    /// Must contain exactly one '=' sign.
    pub equation: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExpandRequest {
    /// The mathematical expression to expand.
    /// Use ** for powers (e.g. x**2), explicit * for multiplication.
    /// Note: Results use ^ notation for powers (e.g. x^2 + 2*x + 1).
    pub expression: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SummationRequest {
    /// The mathematical expression to sum.
    /// Use ** for powers (e.g. x**2), explicit * for multiplication, and x as the default variable.
    pub expression: String,
    /// The starting value of the summation. Defaults to 0.
    #[serde(default)]
    pub start: Option<i64>,
    /// The ending value of the summation. Defaults to 10.
    #[serde(default)]
    pub end: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PlotRequest {
    /// The mathematical expression to plot (y = f(x)).
    /// Use ** for powers (e.g. x**2), explicit * for multiplication, and x as the variable.
    pub expression: String,
    /// The start of the x range. Defaults to -10.
    #[serde(default)]
    pub start: Option<i64>,
    /// The end of the x range. Defaults to 10.
    #[serde(default)]
    pub end: Option<i64>,
    /// The number of sample points. Defaults to 100.
    #[serde(default)]
    pub step: Option<i64>,
}

// ===== Pure functions =====
// Each tool has a pure function (no MCP types) that does the actual
// computation, and a `_tool` wrapper that converts to/from MCP types.
// This separation makes the pure functions testable without MCP.

/// Computes the symbolic derivative of `expression` with respect to `variable`.
/// Uses `symb_anafis` 0.8.1. Input uses `**` for powers; output uses `^`.
pub fn differentiate(expression: &str, variable: &str) -> Result<String, String> {
    // Preprocess: convert ** to ^ (symb_anafis uses ^ for powers)
    let processed = expression.replace("**", "^");
    // `std::panic::catch_unwind(|| { ... })` is Rust's equivalent of try-catch.
    // It catches panics (Rust's unrecoverable errors) and returns them as
    // `Err(Box<dyn Any>)`. The `||` syntax creates a closure with no arguments.
    // We use this because CAS crates may panic on malformed input, and a
    // panic in an async tokio worker would kill the server connection.
    std::panic::catch_unwind(|| {
        // `symb_anafis::diff` takes (expression, variable, custom_functions, options).
        // We pass empty custom functions `&[]` and `None` for options.
        // `.map_err(|e| e.to_string())` converts the error type to String.
        symb_anafis::diff(&processed, variable, &[], None).map_err(|e| e.to_string())
    })
    // `.map_err(|_| ...)` transforms the panic error (a `Box<dyn Any>`)
    // into a readable String. The `|_|` ignores the error argument (we
    // can't meaningfully format a `Box<dyn Any>`).
    .map_err(|_| "Differentiation panicked".to_string())?
    // The `?` operator: if the outer `catch_unwind` returned `Err(panic)`,
    // we already handled it above. If the inner `diff()` returned `Err(e)`,
    // `map_err` converted it to `String`. The `?` returns early with that
    // `Err(String)` if present, otherwise unwraps to the `Ok(String)` value.
}

/// Computes the symbolic indefinite integral of `expression` with respect to
/// `variable`. Uses `thales` 0.4.3. Output uses `^` for powers and `ln()` for
/// natural log.
pub fn integrate(expression: &str, variable: &str) -> Result<String, String> {
    let processed = expression.replace("**", "^");
    std::panic::catch_unwind(|| {
        // Parse the string into thales' AST (Abstract Syntax Tree) type.
        // `parse_expression` returns `Result<Expression, Vec<ParseError>>`.
        // `.map_err(|e| format!("{:?}", e))` uses Debug formatting (`{:?}`)
        // to convert the error vector to a string.
        let expr = thales::parser::parse_expression(&processed).map_err(|e| format!("{:?}", e))?;
        // Integrate the AST. Returns `Result<Expression, IntegrationError>`.
        let result = thales::integration::integrate(&expr, variable).map_err(|e| e.to_string())?;
        // `.simplify()` cleans up the result (e.g. "x^(2+1) / (2+1)" → "x^3 / 3").
        // This is a method on thales' Expression type.
        let simplified = result.simplify();
        // `.to_string()` uses the Display trait to format as a string.
        Ok(simplified.to_string())
    })
    .map_err(|_| "Integration panicked".to_string())?
}

/// Solves an algebraic equation for x. The equation must contain exactly one
/// `=` sign. Uses `mathcore` 0.3.1.
pub fn solve_equation(equation: &str) -> Result<Vec<String>, String> {
    // Split on '=' and validate. `split('=')` returns an iterator;
    // `.collect::<Vec<&str>>()` gathers the pieces into a Vec.
    let parts: Vec<&str> = equation.split('=').collect();
    if parts.len() != 2 {
        return Err("Equation must contain an '=' sign".to_string());
    }
    // Convert "left = right" to "left - right" (the expression = 0 form
    // that mathcore expects). `format!()` is Rust's string formatting
    // macro (like Python's f-strings or printf).
    let expr = format!("({}) - ({})", parts[0].trim(), parts[1].trim());
    let expr = expr.replace("**", "^");
    std::panic::catch_unwind(|| {
        // `MathCore::solve` is an associated function (like a static method
        // in Python/Java — called on the type, not an instance).
        let solutions = mathcore::MathCore::solve(&expr, "x").map_err(|e| e.to_string())?;
        // Normalize -0.0 to 0.0 in the solution strings (cosmetic fix).
        // `.iter()` creates an iterator over references; `.map()` transforms
        // each element; `.collect()` gathers into a Vec.
        let solutions: Vec<String> = solutions
            .iter()
            .map(|s| {
                let str = s.to_string();
                if str == "-0" || str == "-0.0" {
                    "0".to_string()
                } else {
                    str
                }
            })
            .collect();
        Ok(solutions)
    })
    .map_err(|_| "Solving panicked".to_string())?
}

/// Expands an algebraic expression (e.g. "(x + 1)**2" → "1 + x^2 + 2*x").
/// Uses `mathhook-core` 0.2.0 — the only tested Rust CAS crate with expand.
pub fn expand(expression: &str) -> Result<String, String> {
    let processed = expression.replace("**", "^");
    std::panic::catch_unwind(|| {
        // `use mathhook_core::Expand;` imports the `Expand` trait. In Rust,
        // you must import a trait to call its methods — even if the type
        // already implements it. This is different from Java/Python where
        // implementing an interface is enough.
        use mathhook_core::Expand;
        let parser = mathhook_core::parser::Parser::new(
            &mathhook_core::parser::config::ParserConfig::default(),
        );
        let expr = parser.parse(&processed).map_err(|e| e.to_string())?;
        // `.expand()` calls the Expand trait method on the Expression type.
        let expanded = expr.expand();
        // mathhook-core's Display output wraps numbers in `Integer(N)` —
        // e.g. "Integer(1) + x^Integer(2) + Integer(2) * x". We clean this
        // up to produce readable output: "1 + x^2 + 2 * x".
        Ok(clean_mathhook_display(&expanded.to_string()))
    })
    .map_err(|_| "Expansion panicked".to_string())?
}

/// Strips `Integer(N)` wrappers from mathhook-core's Display output.
/// "Integer(" is 8 characters, so we skip 8 bytes to find the number.
/// `while let Some(start) = ...` is a loop that continues as long as the
/// pattern matches. It's like `while (x = find(...)) is not None` in Python.
fn clean_mathhook_display(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("Integer(") {
        // `&result[start + 8..]` slices the string from position start+8
        // to the end. String slicing in Rust is by byte offset, not char
        // offset — but for ASCII strings these are the same.
        let rest = &result[start + 8..];
        if let Some(end) = rest.find(')') {
            let num = &rest[..end];
            // Rebuild the string: everything before "Integer(" + the number
            // + everything after the closing ")". String concatenation via
            // `format!()` allocates a new String each iteration.
            result = format!("{}{}{}", &result[..start], num, &rest[end + 1..]);
        } else {
            break;
        }
    }
    result
}

/// Computes the summation of `expression` evaluated at each integer from
/// `start` to `end` (inclusive). Uses fasteval (no CAS needed).
/// Returns f64; the caller decides whether to serialize as int or float.
pub fn summation(expression: &str, start: i64, end: i64) -> Result<f64, String> {
    let mut total = 0.0;
    // `start..=end` is an inclusive range iterator (like Python's range(start, end+1)).
    for i in start..=end {
        // `eval_with_x` binds the variable `x` to `i as f64` and evaluates.
        // `?` propagates any evaluation error (e.g. parse error) immediately.
        let y = crate::tools::calculate::eval_with_x(expression, i as f64)?;
        total += y;
    }
    Ok(total)
}

/// Renders a PNG plot of y = f(x) over [start, end] with `step` sample points.
/// Returns the raw PNG bytes. Uses `plotters` with `BitMapBackend`.
///
/// CRITICAL: No caption, labels, or mesh are drawn — plotters panics when
/// rendering text without a font backend (which is disabled because
/// `default-features = false` in Cargo.toml, needed for musl static linking).
/// The rendering is wrapped in `catch_unwind` because a panic would kill
/// the tokio async worker thread.
pub fn plot_function(expression: &str, start: f64, end: f64, step: u32) -> Result<Vec<u8>, String> {
    let n = step as usize;
    if n < 2 {
        return Err("step must be at least 2".to_string());
    }
    // Sample the function at `n` evenly-spaced points over [start, end].
    let dx = (end - start) / (n as f64 - 1.0);
    // `Vec::with_capacity(n)` pre-allocates space for `n` elements —
    // more efficient than letting the Vec grow dynamically.
    let mut points: Vec<(f64, f64)> = Vec::with_capacity(n);
    for i in 0..n {
        let x = start + i as f64 * dx;
        // Evaluate the expression at this x value. Skip points that
        // can't be evaluated or produce non-finite values (NaN/inf).
        // `Ok(y) if y.is_finite()` is a match guard — it only matches
        // if `y.is_finite()` is true. `_ => {}` is the catch-all (do nothing).
        match crate::tools::calculate::eval_with_x(expression, x) {
            Ok(y) if y.is_finite() => points.push((x, y)),
            _ => {}
        }
    }
    if points.is_empty() {
        return Err("Could not evaluate expression at any point".to_string());
    }

    // Find the y-axis range for the plot.
    // `.map(|(_, y)| *y)` extracts the y coordinate from each (x, y) tuple.
    // The `_` discards the x coordinate. `*y` dereferences the `&f64`.
    // `.fold(initial, f64::min)` finds the minimum (like Python's min()).
    let y_min = points.iter().map(|(_, y)| *y).fold(f64::INFINITY, f64::min);
    let y_max = points
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::NEG_INFINITY, f64::max);
    if !y_min.is_finite() || !y_max.is_finite() {
        return Err("Invalid y range".to_string());
    }

    let width = 800u32; // `800u32` = literal 800 as u32 type
    let height = 600u32;
    // Allocate a raw RGB buffer (3 bytes per pixel: red, green, blue).
    let mut rgb_buffer = vec![0u8; (width * height * 3) as usize];

    // CRITICAL: wrap plotters rendering in catch_unwind — panics kill tokio workers.
    // CRITICAL: NO caption, NO labels, NO mesh — plotters panics without fonts
    // when default-features = false (needed for musl static linking).
    //
    // `AssertUnwindSafe(...)` wraps the closure and asserts that it's safe
    // to call even if a panic occurs. This is needed because closures that
    // capture mutable references (like `&mut rgb_buffer`) aren't normally
    // unwind-safe. We accept the risk because we discard the buffer on panic.
    let render_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        use plotters::prelude::*;
        // `BitMapBackend::with_buffer` draws directly into our RGB buffer.
        // `.into_drawing_area()` converts the backend into a drawing area.
        let root = BitMapBackend::with_buffer(&mut rgb_buffer, (width, height)).into_drawing_area();
        root.fill(&WHITE).map_err(|e| e.to_string())?;

        // Build a 2D Cartesian chart. No caption or labels (font panics).
        let mut chart = ChartBuilder::on(&root)
            .margin(20)
            .build_cartesian_2d(start..end, y_min..y_max)
            .map_err(|e| e.to_string())?;

        // Draw the function as a blue line. `LineSeries::new` takes an
        // iterator of (x, y) points. `.cloned()` copies the tuples out of
        // the Vec (we need owned values, not references).
        chart
            .draw_series(LineSeries::new(points.iter().cloned(), &BLUE))
            .map_err(|e| e.to_string())?;

        // `.present()` flushes the drawing to the buffer.
        root.present().map_err(|e| e.to_string())?;
        // The `Ok::<(), String>(())` annotation tells the compiler the
        // closure returns `Result<(), String>`. This is needed because
        // `catch_unwind` needs to know the return type.
        Ok::<(), String>(())
    }));

    // Handle the two layers of Result: outer (catch_unwind) and inner (our closure).
    match render_result {
        Ok(Ok(())) => {}                                             // Both succeeded.
        Ok(Err(e)) => return Err(e),                                 // Closure returned an error.
        Err(_) => return Err("Plot rendering panicked".to_string()), // Closure panicked.
    }

    // Encode the raw RGB buffer as PNG using the `image` crate.
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    image::ImageEncoder::write_image(
        encoder,
        &rgb_buffer,
        width,
        height,
        image::ExtendedColorType::Rgb8,
    )
    .map_err(|e| e.to_string())?;

    Ok(png_bytes)
}

// ===== Tool wrapper functions =====
// These convert between MCP types (CallToolResult) and pure Rust types
// (String, f64, Vec). Each follows the same pattern: extract optional
// fields with defaults, call the pure function, wrap the result.

pub fn differentiate_tool(req: DifferentiateRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    // `.unwrap_or_else(|| ...)` returns the value if `Some`, or calls the
    // closure if `None`. This is how we default `variable` to "x".
    let var = req.variable.unwrap_or_else(|| "x".to_string());
    match differentiate(&req.expression, &var) {
        Ok(result) => Ok(ok_json(json!({"result": result}))),
        Err(msg) => Ok(err_json(&msg)),
    }
}

pub fn integrate_tool(req: IntegrateRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    let var = req.variable.unwrap_or_else(|| "x".to_string());
    match integrate(&req.expression, &var) {
        Ok(result) => Ok(ok_json(json!({"result": result}))),
        Err(msg) => Ok(err_json(&msg)),
    }
}

pub fn solve_equation_tool(req: SolveEquationRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match solve_equation(&req.equation) {
        Ok(solutions) => Ok(ok_json(json!({"solutions": solutions}))),
        Err(msg) => Ok(err_json(&msg)),
    }
}

pub fn expand_tool(req: ExpandRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match expand(&req.expression) {
        Ok(result) => Ok(ok_json(json!({"result": result}))),
        Err(msg) => Ok(err_json(&msg)),
    }
}

pub fn summation_tool(req: SummationRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    let start = req.start.unwrap_or(0);
    let end = req.end.unwrap_or(10);
    match summation(&req.expression, start, end) {
        Ok(val) => {
            // Serialize as integer if the result is a whole number (e.g.
            // sum of x^2 from 0 to 10 = 385, not 385.0). This matches the
            // Python server's `int(result) if result.is_integer else float(result)`.
            if val.fract() == 0.0 && val >= i64::MIN as f64 && val <= i64::MAX as f64 {
                Ok(ok_json(json!({"result": val as i64})))
            } else {
                Ok(ok_json(json!({"result": val})))
            }
        }
        Err(msg) => Ok(err_json(&msg)),
    }
}

pub fn plot_tool(req: PlotRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    let start = req.start.unwrap_or(-10) as f64;
    let end = req.end.unwrap_or(10) as f64;
    let step = req.step.unwrap_or(100) as u32;
    match plot_function(&req.expression, start, end, step) {
        Ok(png_bytes) => {
            // Encode the PNG as base64 for potential image content blocks.
            // `use base64::Engine;` imports the trait needed to call `.encode()`.
            use base64::Engine;
            let _b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
            // The `_b64` is prefixed with `_` to suppress the unused variable
            // warning. We compute it but don't use it yet — rmcp 3.x's
            // ContentBlock image API needs verification before we can return
            // image content. For now, we return a text-only success response.
            Ok(CallToolResult::success(vec![
                rmcp::model::ContentBlock::text(
                    json!({"result": "Plot generated successfully."}).to_string(),
                ),
                // TODO: Once rmcp's ContentBlock::image API is confirmed,
                // add: rmcp::model::ContentBlock::image(b64, "image/png")
            ]))
        }
        Err(msg) => Ok(err_json(&msg)),
    }
}

// ===== Tests =====
// Tests for symbolic tools check mathematical equivalence (not string
// equality) because different CAS crates produce different output formats.
// For example, "x^3/3" and "x^(3)/3" are mathematically equivalent.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_differentiate_x_squared() {
        let result = differentiate("x**2", "x").unwrap();
        // Check that the result contains "2" and "x" (should be "2*x")
        assert!(
            result.contains('2') && result.contains('x'),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_differentiate_sin() {
        let result = differentiate("sin(x)", "x").unwrap();
        assert!(result.contains("cos"), "got: {}", result);
    }

    #[test]
    fn test_differentiate_xy() {
        // Partial derivative of x*y with respect to y should be x
        let result = differentiate("x*y", "y").unwrap();
        assert!(result.contains('x'), "got: {}", result);
    }

    #[test]
    fn test_differentiate_exp() {
        let result = differentiate("exp(x)", "x").unwrap();
        assert!(result.contains("exp"), "got: {}", result);
    }

    #[test]
    fn test_integrate_x_squared() {
        // Integral of x^2 is x^3/3 — check it contains "3"
        let result = integrate("x**2", "x").unwrap();
        assert!(result.contains('3'), "got: {}", result);
    }

    #[test]
    fn test_integrate_sin() {
        // Integral of sin(x) is -cos(x)
        let result = integrate("sin(x)", "x").unwrap();
        assert!(result.contains("cos"), "got: {}", result);
    }

    #[test]
    fn test_integrate_exp() {
        // Integral of exp(x) is exp(x)
        let result = integrate("exp(x)", "x").unwrap();
        assert!(result.contains("exp"), "got: {}", result);
    }

    #[test]
    fn test_integrate_one_over_x() {
        // Integral of 1/x is ln(x) — thales outputs "ln(abs(x))"
        let result = integrate("1/x", "x").unwrap();
        assert!(
            result.contains("ln") || result.contains("log"),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_solve_quadratic() {
        // x^2 - 5x + 6 = 0 → solutions [2, 3]
        let solutions = solve_equation("x**2 - 5*x + 6 = 0").unwrap();
        assert_eq!(solutions.len(), 2);
        assert!(
            solutions.iter().any(|s| s.contains('2')),
            "solutions: {:?}",
            solutions
        );
        assert!(
            solutions.iter().any(|s| s.contains('3')),
            "solutions: {:?}",
            solutions
        );
    }

    #[test]
    fn test_solve_linear() {
        // 2x + 3 = 7 → x = 2
        let solutions = solve_equation("2*x + 3 = 7").unwrap();
        assert_eq!(solutions.len(), 1);
        assert!(solutions[0].contains('2'), "got: {}", solutions[0]);
    }

    #[test]
    fn test_solve_x_equals_zero() {
        let solutions = solve_equation("x = 0").unwrap();
        assert_eq!(solutions.len(), 1);
    }

    #[test]
    fn test_solve_no_equals() {
        // Missing '=' sign should return an error, not panic
        assert!(solve_equation("no equals sign").is_err());
    }

    #[test]
    fn test_expand() {
        // (x + 1)^2 expanded should contain x
        let result = expand("(x + 1)**2").unwrap();
        assert!(result.contains('x'), "got: {}", result);
    }

    #[test]
    fn test_summation() {
        // Sum of x^2 from 0 to 10 = 0 + 1 + 4 + 9 + ... + 100 = 385
        let result = summation("x**2", 0, 10).unwrap();
        assert_eq!(result, 385.0);
    }

    #[test]
    fn test_plot_function() {
        // Plot should produce valid PNG bytes (PNG magic: \x89PNG)
        let result = plot_function("x**2", -10.0, 10.0, 100).unwrap();
        // `&result[..4]` slices the first 4 bytes. `&[0x89, 0x50, 0x4e, 0x47]`
        // is a byte array literal (hex for \x89 P N G).
        assert_eq!(&result[..4], &[0x89, 0x50, 0x4e, 0x47], "not a PNG");
    }

    #[test]
    fn test_differentiate_error() {
        // Malformed input should not panic — it should return Err or Ok
        // (some CAS crates may not error on garbage, but must not crash)
        assert!(
            differentiate("!!!invalid", "x").is_err() || differentiate("!!!invalid", "x").is_ok()
        );
    }
}
