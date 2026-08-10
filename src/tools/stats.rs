// Statistical analysis tools — mean, variance, standard deviation, median,
// mode, correlation coefficient, linear regression, confidence interval.
//
// Uses `statrs` 0.19 for mean, variance, std dev, median, and the t-distribution.
// Mode, Pearson correlation, and linear regression are hand-rolled (statrs
// doesn't provide them for sample data).
//
// All variance/stddev use POPULATION statistics (ddof=0) to match numpy's
// default behavior (np.var, np.std). The Python server used numpy, so we
// match its semantics.

use rmcp::model::CallToolResult;
use serde_json::json;
// `statrs::statistics` provides the `Statistics` trait (mean, variance, etc.)
// and the `Data` + `Median` types for median computation.
use statrs::statistics::{Data, Median, Statistics};

use crate::tools::{err_json, ok_json};

// ===== Request structs =====
// `Vec<f64>` is a growable array of f64 values (like Python's `list[float]`).
// `#[serde(default)]` on `Option<f64>` makes the field optional in JSON.

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MeanRequest {
    /// A list of numerical values.
    pub data: Vec<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct VarianceRequest {
    /// A list of numerical values.
    pub data: Vec<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct StandardDeviationRequest {
    /// A list of numerical values.
    pub data: Vec<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MedianRequest {
    /// A list of numerical values.
    pub data: Vec<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ModeRequest {
    /// A list of numerical values.
    pub data: Vec<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CorrelationRequest {
    /// The first list of numerical values.
    pub data_x: Vec<f64>,
    /// The second list of numerical values.
    pub data_y: Vec<f64>,
}

// `Vec<(f64, f64)>` is a Vec of tuples — each element is a pair of f64.
// In JSON, this serializes as an array of 2-element arrays: [[1,2], [3,4]].
// schemars handles tuples as fixed-length arrays.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct LinearRegressionRequest {
    /// A list of (x, y) coordinate pairs.
    pub data: Vec<(f64, f64)>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ConfidenceIntervalRequest {
    /// A list of numerical values.
    pub data: Vec<f64>,
    /// The confidence level (0-1). Defaults to 0.95 (95% confidence).
    #[serde(default)]
    pub confidence: Option<f64>,
}

// ===== Pure functions =====
// Each takes `&[f64]` (a borrowed slice — a reference to a contiguous array
// of f64, like Python's `list[float]` but without ownership). Returns
// `Result<f64, String>` — Ok(value) or Err(error_message).

/// Computes the arithmetic mean. Uses statrs' `Statistics` trait, which is
/// blanket-implemented for any iterator of `&f64`. Empty input → error.
pub fn mean(data: &[f64]) -> Result<f64, String> {
    if data.is_empty() {
        return Err("Cannot compute mean of empty array".to_string());
    }
    // `.mean()` is provided by the `Statistics` trait. It's called directly
    // on `&[f64]` because the trait is blanket-implemented for `IntoIterator`.
    Ok(data.mean())
}

/// Computes the population variance (ddof=0, matching numpy's np.var).
pub fn variance(data: &[f64]) -> Result<f64, String> {
    if data.is_empty() {
        return Err("Cannot compute variance of empty array".to_string());
    }
    // `population_variance()` uses ddof=0 (divide by N). This matches numpy's
    // default. The sample variance (ddof=1, divide by N-1) would use
    // `.variance()` instead.
    Ok(data.population_variance())
}

/// Computes the population standard deviation (ddof=0, matching numpy's np.std).
pub fn standard_deviation(data: &[f64]) -> Result<f64, String> {
    if data.is_empty() {
        return Err("Cannot compute standard deviation of empty array".to_string());
    }
    Ok(data.population_std_dev())
}

/// Computes the median (middle value when sorted). Uses statrs' `Data` type
/// which sorts in place, so we clone the data first.
pub fn median(data: &[f64]) -> Result<f64, String> {
    if data.is_empty() {
        return Err("Cannot compute median of empty array".to_string());
    }
    // `data.to_vec()` clones the slice into an owned Vec. We need an owned
    // Vec because `Data::new` takes ownership and sorts in place.
    let sorted_data = data.to_vec();
    let d = Data::new(sorted_data);
    // `.median()` is from the `Median` trait (imported above). For even-
    // length arrays, it returns the average of the two middle elements.
    Ok(d.median())
}

/// Computes the mode (most frequent value). Ties are broken by returning
/// the smallest value (matching scipy.stats.mode with keepdims=False).
/// Hand-rolled because statrs doesn't provide a mode function for sample data.
pub fn mode(data: &[f64]) -> Result<f64, String> {
    if data.is_empty() {
        return Err("Cannot compute mode of empty array".to_string());
    }

    // Sort a copy so we can count runs of equal values.
    // `partial_cmp` returns `Option<Ordering>` because NaN can't be ordered.
    // `.unwrap()` is safe here because we assume no NaN in the input.
    let mut sorted_data = data.to_vec();
    sorted_data.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut max_count = 0;
    let mut best_value = None; // `Option<f64>` — None until we find the first mode
    let mut i = 0;

    // Walk through the sorted array, counting runs of equal values.
    while i < sorted_data.len() {
        let current_value = sorted_data[i];
        let mut count = 0;

        // Count how many times `current_value` appears consecutively.
        while i < sorted_data.len() && sorted_data[i] == current_value {
            count += 1;
            i += 1;
        }

        // Update the best if this count is strictly greater than the max.
        // Using `>` (not `>=`) means the FIRST (smallest) value wins ties,
        // because we iterate in sorted order.
        if count > max_count {
            max_count = count;
            best_value = Some(current_value);
        }
    }

    // `match` on Option: `Some(val)` extracts the value, `None` is an error
    // (shouldn't happen since we checked for empty input above).
    match best_value {
        Some(val) => Ok(val),
        None => Err("Cannot compute mode of empty array".to_string()),
    }
}

/// Computes the Pearson correlation coefficient between two arrays.
/// Returns a value in [-1, 1]. Hand-rolled because statrs doesn't provide
/// this for raw sample data.
pub fn correlation_coefficient(x: &[f64], y: &[f64]) -> Result<f64, String> {
    if x.len() != y.len() {
        return Err("Arrays must have the same length".to_string());
    }
    if x.is_empty() {
        return Err("Cannot compute correlation of empty arrays".to_string());
    }

    let x_mean = x.mean();
    let y_mean = y.mean();

    let mut numerator = 0.0;
    let mut sum_x_sq = 0.0;
    let mut sum_y_sq = 0.0;

    // `.zip()` pairs up elements from x and y. `(&x_val, &y_val)` destructures
    // the tuple of references. The `&` pattern dereferences automatically.
    for (&x_val, &y_val) in x.iter().zip(y.iter()) {
        let x_diff = x_val - x_mean;
        let y_diff = y_val - y_mean;
        numerator += x_diff * y_diff;
        sum_x_sq += x_diff * x_diff;
        sum_y_sq += y_diff * y_diff;
    }

    let denominator = (sum_x_sq * sum_y_sq).sqrt();

    // If either array has zero variance, the correlation is undefined.
    // We return 0.0 (matching numpy's behavior for this edge case).
    if denominator == 0.0 {
        Ok(0.0)
    } else {
        Ok(numerator / denominator)
    }
}

/// Performs linear regression (least squares). Returns (slope, intercept).
/// Requires at least 2 points and non-zero variance in x.
pub fn linear_regression(data: &[(f64, f64)]) -> Result<(f64, f64), String> {
    if data.len() < 2 {
        return Err("Need at least 2 points for linear regression".to_string());
    }

    // Extract x and y values into separate Vecs.
    // `.map(|(x, _)| *x)` destructures each tuple, takes the first element.
    // `.collect::<Vec<f64>>()` gathers the iterator into a Vec.
    let x_values: Vec<f64> = data.iter().map(|(x, _)| *x).collect();
    let y_values: Vec<f64> = data.iter().map(|(_, y)| *y).collect();

    let x_mean = x_values.mean();
    let y_mean = y_values.mean();

    // Least squares: slope = Σ(xᵢ-x̄)(yᵢ-ȳ) / Σ(xᵢ-x̄)²
    let mut numerator = 0.0;
    let mut denominator = 0.0;

    // `&(x, y)` is a reference to a tuple. Destructuring in the pattern
    // extracts the elements by copying (f64 is Copy, so no clone needed).
    for &(x, y) in data {
        let x_diff = x - x_mean;
        numerator += x_diff * (y - y_mean);
        denominator += x_diff * x_diff;
    }

    if denominator == 0.0 {
        return Err("Zero variance in x values - cannot perform linear regression".to_string());
    }

    let slope = numerator / denominator;
    let intercept = y_mean - slope * x_mean;

    // Returns a tuple — Rust supports multiple return values via tuples.
    Ok((slope, intercept))
}

/// Computes the confidence interval for the mean using the t-distribution.
/// Returns (lower_bound, upper_bound). Requires at least 2 data points.
pub fn confidence_interval(data: &[f64], confidence: f64) -> Result<(f64, f64), String> {
    if data.len() < 2 {
        return Err("Need at least 2 data points for confidence interval".to_string());
    }
    if confidence <= 0.0 || confidence >= 1.0 {
        return Err("Confidence must be between 0 and 1 (exclusive)".to_string());
    }

    // `use` inside a function body — imports are scoped to this function.
    // `ContinuousCDF` provides `.inverse_cdf()` (the quantile function).
    // `StudentsT` is the t-distribution.
    use statrs::distribution::{ContinuousCDF, StudentsT};

    let n = data.len() as f64;
    let mean_val = data.mean();
    // `.std_dev()` is the SAMPLE standard deviation (ddof=1). We use this
    // for the standard error of the mean (SEM), matching scipy.stats.sem.
    let std_dev = data.std_dev();
    // SEM = std_dev / sqrt(n)
    let sem = std_dev / (n).sqrt();

    // Degrees of freedom = n - 1
    let df = n - 1.0;

    // Get the critical t-value for a two-tailed test.
    // `StudentsT::new(location, scale, freedom)` returns a Result (can fail
    // if freedom <= 0). `?` propagates the error.
    // `.inverse_cdf((1 + confidence) / 2)` gives the t-value such that
    // P(T <= t) = (1 + confidence) / 2. For 95% confidence, this is t_{0.975}.
    let t_critical = StudentsT::new(0.0, 1.0, df)
        .map_err(|_| "Failed to create t-distribution".to_string())?
        .inverse_cdf((1.0 + confidence) / 2.0);

    let margin_of_error = t_critical * sem;
    let lower = mean_val - margin_of_error;
    let upper = mean_val + margin_of_error;

    Ok((lower, upper))
}

// ===== Tool wrapper functions =====
// Each follows the same pattern: call the pure function, serialize the
// result as int or float, wrap in ok_json/err_json.
// The int/float check: if `result.fract() == 0.0`, serialize as `i64`
// (e.g. 20 instead of 20.0) to match the Python server's output.

pub fn mean_tool(req: MeanRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match mean(&req.data) {
        Ok(result) => {
            let json_result = if result.fract() == 0.0 {
                json!({"result": result as i64})
            } else {
                json!({"result": result})
            };
            Ok(ok_json(json_result))
        }
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn variance_tool(req: VarianceRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match variance(&req.data) {
        Ok(result) => {
            let json_result = if result.fract() == 0.0 {
                json!({"result": result as i64})
            } else {
                json!({"result": result})
            };
            Ok(ok_json(json_result))
        }
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn standard_deviation_tool(
    req: StandardDeviationRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match standard_deviation(&req.data) {
        Ok(result) => {
            let json_result = if result.fract() == 0.0 {
                json!({"result": result as i64})
            } else {
                json!({"result": result})
            };
            Ok(ok_json(json_result))
        }
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn median_tool(req: MedianRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match median(&req.data) {
        Ok(result) => {
            let json_result = if result.fract() == 0.0 {
                json!({"result": result as i64})
            } else {
                json!({"result": result})
            };
            Ok(ok_json(json_result))
        }
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn mode_tool(req: ModeRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match mode(&req.data) {
        Ok(result) => {
            let json_result = if result.fract() == 0.0 {
                json!({"result": result as i64})
            } else {
                json!({"result": result})
            };
            Ok(ok_json(json_result))
        }
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn correlation_tool(req: CorrelationRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match correlation_coefficient(&req.data_x, &req.data_y) {
        Ok(result) => {
            let json_result = if result.fract() == 0.0 {
                json!({"result": result as i64})
            } else {
                json!({"result": result})
            };
            Ok(ok_json(json_result))
        }
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn linear_regression_tool(
    req: LinearRegressionRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match linear_regression(&req.data) {
        // Destructure the (slope, intercept) tuple returned by linear_regression
        Ok((slope, intercept)) => {
            let json_result = json!({
                "slope": slope,
                "intercept": intercept
            });
            Ok(ok_json(json_result))
        }
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn confidence_interval_tool(
    req: ConfidenceIntervalRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    // Default confidence level is 0.95 if not specified
    let c = req.confidence.unwrap_or(0.95);
    match confidence_interval(&req.data, c) {
        Ok((lower, upper)) => {
            // Serialize as a JSON array [lower, upper] (matching Python's tuple output)
            let json_result = json!({
                "confidence_interval": [lower, upper]
            });
            Ok(ok_json(json_result))
        }
        Err(e) => Ok(err_json(&e)),
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mean() {
        assert_eq!(mean(&[1.0, 2.0, 3.0, 4.0]).unwrap(), 2.5);
        assert_eq!(mean(&[10.0, 20.0, 30.0]).unwrap(), 20.0);
        assert!(mean(&[]).is_err());
    }

    #[test]
    fn test_variance() {
        // Population variance (ddof=0): 1.25
        assert_eq!(variance(&[1.0, 2.0, 3.0, 4.0]).unwrap(), 1.25);
        assert!(variance(&[]).is_err());
    }

    #[test]
    fn test_standard_deviation() {
        // Population std dev: sqrt(1.25) ≈ 1.118...
        assert!(
            (standard_deviation(&[1.0, 2.0, 3.0, 4.0]).unwrap() - 1.118033988749895).abs() < 1e-10
        );
        assert!(standard_deviation(&[]).is_err());
    }

    #[test]
    fn test_median() {
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]).unwrap(), 2.5);
        assert!(median(&[]).is_err());
    }

    #[test]
    fn test_mode() {
        assert_eq!(mode(&[1.0, 2.0, 2.0, 3.0]).unwrap(), 2.0);
        assert_eq!(mode(&[1.0, 1.0, 2.0, 2.0]).unwrap(), 1.0); // ties → smallest
        assert_eq!(mode(&[]).unwrap_err(), "Cannot compute mode of empty array");
    }

    #[test]
    fn test_correlation() {
        assert!(
            (correlation_coefficient(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]).unwrap() - 1.0).abs()
                < 1e-10
        );
        assert!(correlation_coefficient(&[1.0], &[1.0, 2.0]).is_err()); // length mismatch
    }

    #[test]
    fn test_linear_regression() {
        let (slope, intercept) = linear_regression(&[(1.0, 2.0), (2.0, 3.0), (3.0, 5.0)]).unwrap();
        assert!((slope - 1.5).abs() < 1e-10);
        assert!((intercept - 0.3333333333333335).abs() < 1e-10);
        assert!(linear_regression(&[(1.0, 2.0)]).is_err()); // n < 2
    }

    #[test]
    fn test_confidence_interval() {
        let (lower, upper) = confidence_interval(&[1.0, 2.0, 3.0, 4.0], 0.95).unwrap();
        assert!((lower - 0.445739743239121).abs() < 1e-8);
        assert!((upper - 4.5542602567608785).abs() < 1e-8);
        assert!(confidence_interval(&[1.0], 0.95).is_err()); // n < 2
    }
}
