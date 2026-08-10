// `fasteval` is a pure-Rust expression evaluator. `ez_eval` takes an
// expression string and a namespace callback, and returns `Result<f64,
// fasteval::Error>`. It's designed for untrusted input (has parse-time
// size/nesting limits).
use fasteval::ez_eval;
// `CallToolResult` is the MCP tool response type (see mod.rs for details).
use rmcp::model::CallToolResult;
// `json!` is a macro that builds `serde_json::Value` from a JSON-like
// Rust expression at compile time.
use serde_json::json;
// `statrs` provides statistical and special mathematical functions.
// These submodules contain the `gamma`, `ln_gamma`, `erf`, `erfc` functions.
use statrs::function::erf;
use statrs::function::gamma;

// The request struct for the `calculate` tool. `#[derive(...)]` auto-implements:
//   - `Debug`: for `{:?}` formatting (logging)
//   - `serde::Deserialize`: for parsing incoming JSON arguments
//   - `schemars::JsonSchema`: for generating the JSON Schema shown to MCP clients
// `pub` makes the struct visible outside this module (main.rs needs it for
// the `Parameters<CalculateRequest>` type).
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CalculateRequest {
    /// The mathematical expression to evaluate.
    /// Use ** for powers (e.g. x**2), explicit * for multiplication.
    /// Supports: sin, cos, tan, exp, log, log10, sqrt, pi, e, abs, factorial,
    /// gamma, erf, mean, median, std, var, min, max, sum, prod, and more.
    /// Note: isfinite/isinf/isnan return 1.0/0.0 instead of bools.
    /// Note: cumsum, cumprod, clip, unique, sort, argsort, argmax, np, math
    /// are not supported (they return arrays, but fasteval is scalar-only).
    pub expression: String,
}

/// The namespace callback — this is the Rust port of Python's `ALLOW_FUNCTION`
/// dict. fasteval calls this function for every name it doesn't recognize as
/// a builtin. If we return `Some(value)`, fasteval uses that value. If we
/// return `None`, fasteval reports "undefined name" (an error).
///
/// Parameters:
///   - `name`: the function/variable name (e.g. "pi", "sin", "factorial")
///   - `args`: the arguments passed (empty for 0-arg "variables" like pi/e)
///   - `x_val`: `None` for `eval_expression` (bare `x` is undefined), or
///     `Some(value)` for `eval_with_x` (binds `x` to a numeric value, used
///     by summation and plot_function).
///
/// Returns `Option<f64>` — `Some(result)` if we handle the name, `None` if
/// we don't (letting fasteval try its builtins or report an error).
fn namespace_lookup(name: &str, args: Vec<f64>, x_val: Option<f64>) -> Option<f64> {
    // If the caller bound `x` to a value, return it. This is how summation
    // and plot_function evaluate expressions with a variable.
    if name == "x" {
        return x_val;
    }
    // `match` is Rust's pattern matching construct — like a switch statement
    // but much more powerful. Each arm is `"pattern" => expression`.
    // The `|` operator combines multiple patterns into one arm.
    match name {
        // Constants (0-arg "functions" — fasteval calls with empty args)
        "pi" => Some(std::f64::consts::PI),
        "e" => Some(std::f64::consts::E),
        // `args.first()` returns `Option<&f64>` — `None` if empty, `Some(&val)`
        // if there's at least one element. This is Rust's safe alternative to
        // indexing (which would panic on out-of-bounds).
        // `.map(|&x| x.exp())` transforms `Option<&f64>` into `Option<f64>`:
        // if `Some(&x)`, it calls `x.exp()` and wraps in `Some`; if `None`,
        // it stays `None`. The `|&x|` pattern dereferences the `&f64`.
        "exp" => args.first().map(|&x| x.exp()),
        "sqrt" => args.first().map(|&x| x.sqrt()),
        // Python's `log` has two modes: 1-arg = natural log (ln), 2-arg =
        // log_base(value). fasteval has a builtin `log` with DIFFERENT arg
        // order, so we rename `log(` → `_pylog(` in preprocessing and handle
        // it here to avoid the builtin shadowing our callback.
        "_pylog" => match args.len() {
            1 => args.first().map(|&x| x.ln()),
            // Direct indexing `args[0]` is safe here because we checked `len()`.
            2 => Some(args[0].log(args[1])),
            _ => None,
        },
        "log10" => args.first().map(|&x| x.log10()),
        // Reciprocal trig functions
        "cot" => args.first().map(|&x| 1.0 / x.tan()),
        "csc" => args.first().map(|&x| 1.0 / x.sin()),
        "sec" => args.first().map(|&x| 1.0 / x.cos()),
        // Angle conversions
        "degrees" => args.first().map(|&x| x.to_degrees()),
        "radians" => args.first().map(|&x| x.to_radians()),
        // Factorial — only for non-negative integers, capped at 20! to
        // avoid overflow. `x.trunc()` checks if x is a whole number.
        // `.and_then()` chains `Option`s: if `first()` is `None`, returns
        // `None`; if `Some(&x)`, calls the closure which itself returns
        // an `Option`.
        "factorial" => args.first().and_then(|&x| {
            if x >= 0.0 && x == x.trunc() {
                let n = x as u64; // `as` is a type cast (f64 → u64)
                if n > 20 {
                    None
                } else {
                    let mut result = 1u64; // `1u64` = the literal 1 as u64
                    for i in 1..=n {
                        // `1..=n` is an inclusive range (1 to n inclusive)
                        result *= i;
                    }
                    Some(result as f64)
                }
            } else {
                None
            }
        }),
        // Special functions from statrs
        "gamma" => args.first().map(|&x| gamma::gamma(x)),
        "lgamma" => args.first().map(|&x| gamma::ln_gamma(x)),
        "erf" => args.first().map(|&x| erf::erf(x)),
        "erfc" => args.first().map(|&x| erf::erfc(x)),
        // Python returns bools; we return 1.0/0.0 (documented divergence)
        "isfinite" => args.first().map(|&x| if x.is_finite() { 1.0 } else { 0.0 }),
        "isinf" => args
            .first()
            .map(|&x| if x.is_infinite() { 1.0 } else { 0.0 }),
        "isnan" => args.first().map(|&x| if x.is_nan() { 1.0 } else { 0.0 }),
        // Integer square root via binary search
        "isqrt" => args.first().and_then(|&x| {
            if x >= 0.0 && x == x.trunc() {
                let val = x as u64;
                if val == 0 {
                    Some(0.0)
                } else {
                    let mut low = 1u64;
                    let mut high = val;
                    while low <= high {
                        let mid = low + (high - low) / 2;
                        let square = mid * mid;
                        if square == val {
                            return Some(mid as f64);
                        } else if square < val {
                            low = mid + 1;
                        } else {
                            high = mid - 1;
                        }
                    }
                    Some(high as f64)
                }
            } else {
                None
            }
        }),
        // Variadic reductions over `args` (the full argument list).
        // `args.iter()` creates an iterator over `&f64` references.
        // `.product()` consumes the iterator and multiplies all elements.
        "prod" => Some(args.iter().product()),
        // `.sum::<f64>()` sums all elements. The `::<f64>` is a "turbofish" —
        // it tells the compiler the output type is `f64`. Without it, the
        // compiler can't infer the type (sum is generic over numeric types).
        "mean" => {
            if args.is_empty() {
                None
            } else {
                Some(args.iter().sum::<f64>() / args.len() as f64)
            }
        }
        // Median: sort, then take the middle element (or average of two
        // middle elements for even-length arrays).
        // `partial_cmp` returns `Option<Ordering>` because NaN can't be
        // ordered. `.unwrap()` panics if `None` — safe here because we
        // already filtered NaN values upstream.
        "median" => {
            if args.is_empty() {
                None
            } else {
                let mut sorted = args.to_vec(); // `to_vec()` clones the Vec
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let len = sorted.len();
                if len % 2 == 1 {
                    Some(sorted[len / 2])
                } else {
                    let mid1 = sorted[len / 2 - 1];
                    let mid2 = sorted[len / 2];
                    Some((mid1 + mid2) / 2.0)
                }
            }
        }
        // Population standard deviation (ddof=0, numpy default).
        // `.powi(2)` is integer power (x²) — faster than `.powf(2.0)`.
        "std" => {
            if args.is_empty() {
                None
            } else {
                let mean_val = args.iter().sum::<f64>() / args.len() as f64;
                let variance =
                    args.iter().map(|&x| (x - mean_val).powi(2)).sum::<f64>() / args.len() as f64;
                Some(variance.sqrt())
            }
        }
        // Population variance (ddof=0)
        "var" => {
            if args.is_empty() {
                None
            } else {
                let mean_val = args.iter().sum::<f64>() / args.len() as f64;
                let variance =
                    args.iter().map(|&x| (x - mean_val).powi(2)).sum::<f64>() / args.len() as f64;
                Some(variance)
            }
        }
        // `.fold(initial, |accumulator, &item| ...)` is like Python's
        // `functools.reduce` — it applies the closure repeatedly, carrying
        // the accumulator forward.
        "min" => {
            if args.is_empty() {
                None
            } else {
                Some(args.iter().fold(f64::INFINITY, |acc, &x| acc.min(x)))
            }
        }
        "max" => {
            if args.is_empty() {
                None
            } else {
                Some(args.iter().fold(f64::NEG_INFINITY, |acc, &x| acc.max(x)))
            }
        }
        "sum" => Some(args.iter().sum()),
        // These are fasteval builtins — returning `None` tells fasteval to
        // use its own implementation. We list them explicitly for clarity.
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "ceil" | "floor" | "round" | "abs" => {
            None
        }
        // Dropped functions (array-output or module namespaces). fasteval
        // is scalar-only (returns f64), so these numpy functions that
        // return arrays can't be supported. Returning `None` means fasteval
        // will report "undefined name" — an error, which is the desired
        // behavior for unsupported functions.
        "cumsum" | "cumprod" | "clip" | "unique" | "sort" | "argsort" | "argmax" | "np"
        | "math" => None,
        // Catch-all: any name we don't recognize. fasteval will report
        // "undefined name" — an error.
        _ => None,
    }
}

/// Evaluates a mathematical expression string and returns the result as f64.
///
/// This is the pure computation function (no MCP wrapper). It does NOT bind
/// the variable `x` — a bare `x` in the expression will cause an error.
/// Used by the `calculate` tool.
///
/// `&str` is a borrowed string slice — it doesn't own the data, just
/// references it. This is the standard Rust string type for function
/// parameters (like `const char*` in C).
pub fn eval_expression(expr: &str) -> Result<f64, String> {
    // `let mut` declares a mutable binding. Rust bindings are immutable
    // by default; `mut` allows reassignment.
    // `.to_string()` allocates a new `String` (owned, heap-allocated) from
    // the borrowed `&str`. We need an owned `String` because `.replace()`
    // returns a new `String`.
    let mut processed = expr.to_string();
    // Preprocessing pipeline (order matters):
    // 1. `**` → `^`: fasteval uses `^` for powers (right-associative, like
    //    Python's `**`). Users write `**` (Python syntax); we convert.
    processed = processed.replace("**", "^");
    // 2. `log(` → `_pylog(`: fasteval has a builtin `log(base, val)` with
    //    different arg order than Python's `log(value, base)`. Renaming
    //    prevents the builtin from shadowing our callback.
    processed = processed.replace("log(", "_pylog(");
    // 3. Strip `[` and `]`: fasteval has no list literals. This turns
    //    `mean([1,2,3])` into `mean(1,2,3)` (a valid variadic call).
    //    `replace(['[', ']'], "")` is an array pattern — replaces both
    //    characters in one call (more efficient than chaining .replace()).
    processed = processed.replace(['[', ']'], "");

    // Create the namespace callback as a closure. Closures in Rust are
    // anonymous functions that can capture their environment. The syntax
    // `|params| -> ReturnType { body }` is like Python's lambdas but more
    // powerful. `mut` is needed because fasteval may call it multiple times
    // and the closure mutates its captured state.
    let mut namespace_callback =
        |name: &str, args: Vec<f64>| -> Option<f64> { namespace_lookup(name, args, None) };
    // `ez_eval` parses and evaluates the expression, calling our callback
    // for unknown names. `.map_err(|e| e.to_string())` converts the
    // fasteval error type into a `String` (our error type for this function).
    ez_eval(&processed, &mut namespace_callback).map_err(|e| e.to_string())
}

/// Like `eval_expression`, but binds the variable `x` to a numeric value.
/// Used by `summation` (to evaluate at each integer) and `plot_function`
/// (to evaluate at each sample point).
pub fn eval_with_x(expr: &str, x: f64) -> Result<f64, String> {
    let mut processed = expr.to_string();
    processed = processed.replace("**", "^");
    processed = processed.replace("log(", "_pylog(");
    processed = processed.replace(['[', ']'], "");

    // Same as eval_expression, but passes `Some(x)` so that `namespace_lookup`
    // returns the bound value when it sees the name "x".
    let mut namespace_callback =
        |name: &str, args: Vec<f64>| -> Option<f64> { namespace_lookup(name, args, Some(x)) };
    ez_eval(&processed, &mut namespace_callback).map_err(|e| e.to_string())
}

/// MCP tool wrapper for `calculate`. Takes the raw expression string,
/// evaluates it, and returns a `CallToolResult` (success or error).
///
/// Returns `Result<CallToolResult, ErrorData>` — `Ok` for a successful
/// tool execution (even if the computation itself failed — that's an
/// error *result*, not a protocol error). `Err(ErrorData)` would be for
/// malformed protocol requests, which we don't produce here.
pub fn calculate(expression: &str) -> Result<CallToolResult, rmcp::ErrorData> {
    match eval_expression(expression) {
        Ok(val) => {
            // Guard against NaN and infinity (e.g. from division by zero).
            // Python raises ZeroDivisionError; we return an error JSON.
            if val.is_nan() {
                return Ok(crate::tools::err_json("Result is NaN"));
            }
            if val.is_infinite() {
                return Ok(crate::tools::err_json(
                    "Result is infinite (possible division by zero)",
                ));
            }
            // Serialize as integer if the value is a whole number that fits
            // in i64. This matches Python's behavior: `10` not `10.0`.
            // `val.fract()` returns the fractional part (0.0 for integers).
            // `val as i64` is a type cast (f64 → i64), truncating if needed.
            if val.fract() == 0.0 && val >= i64::MIN as f64 && val <= i64::MAX as f64 {
                Ok(crate::tools::ok_json(json!({"result": val as i64})))
            } else {
                Ok(crate::tools::ok_json(json!({"result": val})))
            }
        }
        // Computation error (parse error, undefined name, etc.) — return
        // as an error *result* (Ok with error content), NOT as Err.
        // This matches the Python server's `{"error": str(e)}` behavior.
        Err(msg) => Ok(crate::tools::err_json(&msg)),
    }
}

// `#[cfg(test)]` means "compile this module only when running `cargo test`."
// This keeps test code out of the release binary. The `mod tests` block is
// a nested module — `use super::*` imports everything from the parent
// module (the `calculate` module) so tests can call `eval_expression`
// without a path prefix.
#[cfg(test)]
mod tests {
    use super::*;

    // `#[test]` marks a function as a test. `cargo test` discovers and runs
    // all `#[test]` functions. Tests pass if they return `()` (no panic);
    // they fail if they panic (e.g. `assert_eq!` fails, or `.unwrap()` on Err).

    #[test]
    fn test_basic_arithmetic() {
        // `.unwrap()` extracts the Ok value or panics if Err. In tests,
        // panicking is the failure mechanism (like `assert` in Python).
        assert_eq!(eval_expression("2 * 3 + 4").unwrap(), 10.0);
    }

    #[test]
    fn test_sin_pi_half() {
        // Floating-point comparison with tolerance (1e-10) because sin(pi/2)
        // may not be exactly 1.0 due to floating-point representation.
        assert!((eval_expression("sin(pi/2)").unwrap() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_sqrt() {
        assert_eq!(eval_expression("sqrt(16)").unwrap(), 4.0);
    }

    #[test]
    fn test_invalid_expression() {
        // `.is_err()` returns true if the Result is Err. We expect an error
        // for an undefined name.
        assert!(eval_expression("invalid * expression").is_err());
    }

    #[test]
    fn test_log_two_args() {
        // log(100, 10) = log_base_10(100) = 2.0 (Python semantics)
        assert!((eval_expression("log(100, 10)").unwrap() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_log_one_arg() {
        // log(100) = ln(100) (natural log, 1-arg Python semantics)
        assert!((eval_expression("log(100)").unwrap() - 100f64.ln()).abs() < 1e-10);
    }

    #[test]
    fn test_mean() {
        // mean([1,2,3,4]) — brackets are stripped in preprocessing, turning
        // this into mean(1,2,3,4) (a variadic call to our callback)
        assert_eq!(eval_expression("mean([1,2,3,4])").unwrap(), 2.5);
    }

    #[test]
    fn test_factorial() {
        assert_eq!(eval_expression("factorial(5)").unwrap(), 120.0);
    }

    #[test]
    fn test_gamma() {
        // gamma(5) = 4! = 24 (with slight float imprecision from statrs)
        assert!((eval_expression("gamma(5)").unwrap() - 24.0).abs() < 1e-10);
    }

    #[test]
    fn test_right_assoc_power() {
        // 2**3**2 = 2**(3**2) = 2**9 = 512 (right-associative, like Python)
        assert_eq!(eval_expression("2**3**2").unwrap(), 512.0);
    }

    #[test]
    fn test_empty_string() {
        assert!(eval_expression("").is_err());
    }
}
