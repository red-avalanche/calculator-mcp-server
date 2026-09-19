// Precision and representation utilities — rounding, significant figures,
// scientific/engineering notation, and decimal-to-fraction conversion.
//
// All operations are f64-based. The binary representation of f64 means some
// decimal values cannot be represented exactly (e.g. 2.675 is stored as
// 2.67499999...), so rounding to 2 decimal places yields 2.67, not 2.68.
// This caveat is documented in the round_number tool description.

use rmcp::model::CallToolResult;
use serde_json::json;

use crate::tools::{err_json, ok_json};

// ===== Request structs =====

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RoundNumberRequest {
    /// The value to round.
    pub value: f64,
    /// Number of decimal places. May be negative (e.g. -2 rounds to hundreds).
    pub decimals: i32,
    /// Rounding mode: "half_up" (half away from zero, default), "half_even"
    /// (banker's rounding — round half to even), "floor", "ceil", or "trunc".
    ///
    /// NOTE: f64 binary representation means some values can't be exact.
    /// For example, 2.675 is stored as 2.67499999... so rounding to 2 decimal
    /// places gives 2.67, not 2.68. This is an inherent limitation of
    /// floating-point arithmetic, not a bug.
    #[serde(default = "default_round_mode")]
    pub mode: String,
}

fn default_round_mode() -> String {
    "half_up".to_string()
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SignificantFiguresRequest {
    /// The value to format.
    pub value: f64,
    /// Number of significant figures (1–17).
    pub sig_figs: i64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FormatNotationRequest {
    /// The value to format.
    pub value: f64,
    /// Notation style: "scientific" (mantissa in [1, 10), e.g. 1.23e4) or
    /// "engineering" (exponent forced to a multiple of 3, mantissa in
    /// [1, 1000), e.g. 12.3e3).
    pub style: String,
    /// Number of significant figures (optional, default 6).
    pub sig_figs: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ToFractionRequest {
    /// The value to convert to a fraction.
    pub value: f64,
    /// Optional maximum denominator for approximation. If omitted, the exact
    /// continued-fraction expansion is used. Example: 0.333 with
    /// max_denominator >= 3 yields 1/3 (approximate, not exact).
    pub max_denominator: Option<i64>,
}

// ===== Pure functions =====

/// Rounds a value to `decimals` decimal places using the specified mode.
///
/// - half_up: half away from zero (Rust's f64::round)
/// - half_even: banker's rounding (round half to even)
/// - floor: toward negative infinity
/// - ceil: toward positive infinity
/// - trunc: toward zero
pub fn round_number(value: f64, decimals: i32, mode: &str) -> Result<f64, String> {
    if !value.is_finite() {
        return Err("Cannot round non-finite value".to_string());
    }
    let factor = 10f64.powi(decimals);
    let scaled = value * factor;
    if !scaled.is_finite() {
        return Err(format!(
            "Value {} is too large to round to {} decimal places (intermediate overflow)",
            value, decimals
        ));
    }
    let rounded = match mode {
        "half_up" => scaled.round(),
        "half_even" => {
            let abs_scaled = scaled.abs();
            let floor = abs_scaled.floor();
            let frac = abs_scaled - floor;
            let rounded_abs = if frac > 0.5 {
                floor + 1.0
            } else if frac < 0.5 {
                floor
            } else {
                // Exactly 0.5: round to even
                if floor as i64 % 2 == 0 {
                    floor
                } else {
                    floor + 1.0
                }
            };
            if scaled < 0.0 {
                -rounded_abs
            } else {
                rounded_abs
            }
        }
        "floor" => scaled.floor(),
        "ceil" => scaled.ceil(),
        "trunc" => scaled.trunc(),
        _ => {
            return Err(format!(
                "Unknown rounding mode: {} (expected half_up, half_even, floor, ceil, or trunc)",
                mode
            ))
        }
    };
    Ok(rounded / factor)
}

/// Formats a value to a specified number of significant figures, returning
/// a string to preserve trailing zeros (e.g. 2.500, not 2.5).
pub fn significant_figures(value: f64, sig_figs: i64) -> Result<String, String> {
    if !value.is_finite() {
        return Err("Cannot format non-finite value".to_string());
    }
    if !(1..=17).contains(&sig_figs) {
        return Err(format!("sig_figs must be 1..=17, got {}", sig_figs));
    }
    if value == 0.0 {
        return Ok("0".to_string());
    }

    let exponent = value.abs().log10().floor() as i32;
    let decimals = sig_figs as i32 - 1 - exponent;
    let factor = 10f64.powi(decimals);
    let scaled = value * factor;
    if !scaled.is_finite() {
        return Err(format!(
            "Value {} is too large for {} significant figures (overflow)",
            value, sig_figs
        ));
    }
    let rounded = scaled.round() / factor;

    if decimals > 0 {
        Ok(format!("{:.*}", decimals as usize, rounded))
    } else if rounded.abs() <= i64::MAX as f64 {
        // Value fits in i64 — use plain integer string
        Ok(format!("{}", rounded as i64))
    } else {
        // Value too large for i64 — use scientific notation
        Ok(format!("{:.*e}", (sig_figs - 1) as usize, rounded))
    }
}

/// Formats a value in scientific or engineering notation.
pub fn format_notation(value: f64, style: &str, sig_figs: Option<i64>) -> Result<String, String> {
    if !value.is_finite() {
        return Err("Cannot format non-finite value".to_string());
    }

    let prec = match sig_figs {
        Some(n) if (1..=17).contains(&n) => n as usize,
        Some(n) => return Err(format!("sig_figs must be 1..=17, got {}", n)),
        None => 6,
    };

    if value == 0.0 {
        let zeros = "0".repeat(prec.saturating_sub(1));
        return Ok(format!("0.{}e0", zeros));
    }

    match style {
        "scientific" => {
            // Rust's {:e} format gives mantissa in [1, 10) with e±X.
            // Precision = decimal places = sig_figs - 1.
            Ok(format!("{:.*e}", prec - 1, value))
        }
        "engineering" => {
            let exponent = value.abs().log10().floor() as i32;
            // Floor division to get the largest multiple of 3 <= exponent.
            let eng_exp = (exponent as f64 / 3.0).floor() as i32 * 3;
            let mantissa = value / 10f64.powi(eng_exp);
            let abs_m = mantissa.abs();
            let digits_before = if abs_m < 10.0 {
                1
            } else if abs_m < 100.0 {
                2
            } else {
                3
            };
            let decimals = prec.saturating_sub(digits_before);
            Ok(format!("{:.*}e{}", decimals, mantissa, eng_exp))
        }
        _ => Err(format!(
            "Unknown style: {} (expected 'scientific' or 'engineering')",
            style
        )),
    }
}

/// Converts a value to a fraction using continued fractions.
/// Returns (numerator, denominator, mixed_string, exact).
///
/// Without max_denominator: runs the CF expansion until it terminates exactly
/// (within f64 precision) or overflows i64.
/// With max_denominator: returns the best approximation with denominator <= max.
pub fn to_fraction(
    value: f64,
    max_denominator: Option<i64>,
) -> Result<(i64, i64, String, bool), String> {
    if !value.is_finite() {
        return Err("Cannot convert non-finite value to fraction".to_string());
    }

    let sign: i64 = if value < 0.0 { -1 } else { 1 };
    let mut x = value.abs();

    // Continued-fraction convergents: h_n/k_n
    // h_{-2}=0, h_{-1}=1, k_{-2}=1, k_{-1}=0
    let mut h_prev2: i64 = 0;
    let mut h_prev1: i64 = 1;
    let mut k_prev2: i64 = 1;
    let mut k_prev1: i64 = 0;
    let mut exact = true;

    loop {
        // Guard: if x is too large to fit in i64, we can't continue the CF
        let floored = x.floor();
        if floored > i64::MAX as f64 || floored < i64::MIN as f64 {
            exact = false;
            break;
        }
        let a = floored as i64;

        // Overflow guard for a * h_prev1
        if a > 0 && (h_prev1 > i64::MAX / a || k_prev1 > i64::MAX / a) {
            exact = false;
            break;
        }
        let h_new = a * h_prev1 + h_prev2;
        let k_new = a * k_prev1 + k_prev2;

        // Check max_denominator
        if let Some(max) = max_denominator {
            if k_new > max {
                // Use intermediate convergent: a_max = (max - k_prev2) / k_prev1
                if k_prev1 == 0 {
                    break;
                }
                let a_max = (max - k_prev2) / k_prev1;
                if a_max >= 1 {
                    let h_inter = a_max * h_prev1 + h_prev2;
                    let k_inter = a_max * k_prev1 + k_prev2;
                    let num = sign * h_inter;
                    let den = k_inter;
                    return Ok((num, den, format_mixed(num, den), false));
                } else {
                    // Previous convergent is the best approximation
                    let num = sign * h_prev1;
                    let den = k_prev1;
                    return Ok((num, den, format_mixed(num, den), false));
                }
            }
        }

        // Overflow / sanity checks
        if h_new < 0 || k_new <= 0 {
            exact = false;
            break;
        }

        h_prev2 = h_prev1;
        h_prev1 = h_new;
        k_prev2 = k_prev1;
        k_prev1 = k_new;

        let frac = x - a as f64;
        if frac.abs() < 1e-15 {
            break; // exact termination
        }
        x = 1.0 / frac;
    }

    let num = sign * h_prev1;
    let den = k_prev1;
    if den == 0 {
        return Err("Failed to compute fraction (denominator is zero)".to_string());
    }
    Ok((num, den, format_mixed(num, den), exact))
}

/// Formats a fraction as a mixed number string.
/// For |v| >= 1: "whole rem/den" (e.g. "-1 1/3").
/// For |v| < 1: "num/den" (e.g. "3/4" or "-1/3").
fn format_mixed(num: i64, den: i64) -> String {
    let abs_num = num.abs();
    let whole = abs_num / den;
    let rem = abs_num % den;
    let sign_str = if num < 0 { "-" } else { "" };
    if rem == 0 {
        format!("{}{}", sign_str, whole)
    } else if whole > 0 {
        format!("{}{} {}/{}", sign_str, whole, rem, den)
    } else {
        format!("{}{}/{}", sign_str, abs_num, den)
    }
}

// ===== Tool wrappers =====

pub fn round_number_tool(req: RoundNumberRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match round_number(req.value, req.decimals, &req.mode) {
        Ok(result) => Ok(ok_json(json!({"result": result}))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn significant_figures_tool(
    req: SignificantFiguresRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match significant_figures(req.value, req.sig_figs) {
        Ok(result) => Ok(ok_json(json!({"result": result}))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn format_notation_tool(req: FormatNotationRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match format_notation(req.value, &req.style, req.sig_figs) {
        Ok(result) => Ok(ok_json(json!({"result": result}))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn to_fraction_tool(req: ToFractionRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match to_fraction(req.value, req.max_denominator) {
        Ok((numerator, denominator, mixed, exact)) => Ok(ok_json(json!({
            "numerator": numerator,
            "denominator": denominator,
            "mixed": mixed,
            "exact": exact
        }))),
        Err(e) => Ok(err_json(&e)),
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    // --- round_number ---

    #[test]
    fn test_round_half_up() {
        assert_eq!(round_number(2.5, 0, "half_up").unwrap(), 3.0);
        assert_eq!(round_number(-2.5, 0, "half_up").unwrap(), -3.0);
        assert_eq!(round_number(2.4, 0, "half_up").unwrap(), 2.0);
        assert_eq!(round_number(2.6, 0, "half_up").unwrap(), 3.0);
    }

    #[test]
    fn test_round_half_even() {
        assert_eq!(round_number(2.5, 0, "half_even").unwrap(), 2.0);
        assert_eq!(round_number(3.5, 0, "half_even").unwrap(), 4.0);
        assert_eq!(round_number(-2.5, 0, "half_even").unwrap(), -2.0);
        assert_eq!(round_number(-3.5, 0, "half_even").unwrap(), -4.0);
        assert_eq!(round_number(2.4, 0, "half_even").unwrap(), 2.0);
        assert_eq!(round_number(2.6, 0, "half_even").unwrap(), 3.0);
    }

    #[test]
    fn test_round_floor_ceil_trunc() {
        assert_eq!(round_number(2.5, 0, "floor").unwrap(), 2.0);
        assert_eq!(round_number(-2.5, 0, "floor").unwrap(), -3.0);
        assert_eq!(round_number(2.5, 0, "ceil").unwrap(), 3.0);
        assert_eq!(round_number(-2.5, 0, "ceil").unwrap(), -2.0);
        assert_eq!(round_number(2.5, 0, "trunc").unwrap(), 2.0);
        assert_eq!(round_number(-2.5, 0, "trunc").unwrap(), -2.0);
    }

    #[test]
    fn test_round_decimals() {
        assert_eq!(round_number(3.14159, 2, "half_up").unwrap(), 3.14);
        assert_eq!(round_number(3.14159, 4, "half_up").unwrap(), 3.1416);
    }

    #[test]
    fn test_round_negative_decimals() {
        assert_eq!(round_number(1234.0, -2, "half_up").unwrap(), 1200.0);
        assert_eq!(round_number(1250.0, -2, "half_up").unwrap(), 1300.0);
    }

    #[test]
    fn test_round_invalid_mode() {
        assert!(round_number(1.0, 0, "invalid").is_err());
    }

    // --- significant_figures ---

    #[test]
    fn test_significant_figures() {
        assert_eq!(significant_figures(1234.0, 3).unwrap(), "1230");
        assert_eq!(significant_figures(0.01234, 3).unwrap(), "0.0123");
        assert_eq!(significant_figures(2.5, 3).unwrap(), "2.50");
        assert_eq!(significant_figures(0.0, 3).unwrap(), "0");
    }

    #[test]
    fn test_significant_figures_trailing_zeros() {
        assert_eq!(significant_figures(2.5, 4).unwrap(), "2.500");
        assert_eq!(significant_figures(123.0, 5).unwrap(), "123.00");
    }

    #[test]
    fn test_significant_figures_invalid() {
        assert!(significant_figures(1.0, 0).is_err());
        assert!(significant_figures(1.0, 18).is_err());
    }

    // --- format_notation ---

    #[test]
    fn test_format_scientific() {
        assert_eq!(
            format_notation(12000.0, "scientific", Some(3)).unwrap(),
            "1.20e4"
        );
        assert_eq!(
            format_notation(0.0012, "scientific", Some(2)).unwrap(),
            "1.2e-3"
        );
    }

    #[test]
    fn test_format_engineering() {
        assert_eq!(
            format_notation(12000.0, "engineering", Some(2)).unwrap(),
            "12e3"
        );
        assert_eq!(
            format_notation(0.0012, "engineering", Some(2)).unwrap(),
            "1.2e-3"
        );
    }

    #[test]
    fn test_format_notation_zero() {
        assert_eq!(
            format_notation(0.0, "scientific", Some(3)).unwrap(),
            "0.00e0"
        );
        assert_eq!(
            format_notation(0.0, "engineering", None).unwrap(),
            "0.00000e0"
        );
    }

    #[test]
    fn test_format_notation_invalid_style() {
        assert!(format_notation(1.0, "invalid", None).is_err());
    }

    // --- to_fraction ---

    #[test]
    fn test_to_fraction_exact() {
        let (num, den, mixed, exact) = to_fraction(0.75, None).unwrap();
        assert_eq!(num, 3);
        assert_eq!(den, 4);
        assert_eq!(mixed, "3/4");
        assert!(exact);
    }

    #[test]
    fn test_to_fraction_integer() {
        let (num, den, mixed, exact) = to_fraction(5.0, None).unwrap();
        assert_eq!(num, 5);
        assert_eq!(den, 1);
        assert_eq!(mixed, "5");
        assert!(exact);
    }

    #[test]
    fn test_to_fraction_negative() {
        let (num, den, mixed, exact) = to_fraction(-1.3333333333333333, None).unwrap();
        assert_eq!(num, -4);
        assert_eq!(den, 3);
        assert_eq!(mixed, "-1 1/3");
        assert!(exact);
    }

    #[test]
    fn test_to_fraction_approximate() {
        let (num, den, _mixed, exact) = to_fraction(0.333, Some(3)).unwrap();
        assert_eq!(num, 1);
        assert_eq!(den, 3);
        assert!(!exact);
    }

    #[test]
    fn test_to_fraction_zero() {
        let (num, den, mixed, exact) = to_fraction(0.0, None).unwrap();
        assert_eq!(num, 0);
        assert_eq!(den, 1);
        assert_eq!(mixed, "0");
        assert!(exact);
    }

    // --- overflow / edge case tests ---

    #[test]
    fn test_round_number_overflow() {
        // 1e308 * 100 = infinity → should error, not return null
        assert!(round_number(1e308, 2, "half_up").is_err());
    }

    #[test]
    fn test_significant_figures_large() {
        // 1e20 > i64::MAX → should use scientific notation, not overflow
        let result = significant_figures(1e20, 3).unwrap();
        assert!(
            result.contains("e"),
            "Expected scientific notation for 1e20, got: {}",
            result
        );
    }

    #[test]
    fn test_significant_figures_normal_large() {
        // 1234 with 3 sig figs → "1230" (fits in i64, use plain string)
        assert_eq!(significant_figures(1234.0, 3).unwrap(), "1230");
    }

    #[test]
    fn test_to_fraction_huge_value() {
        // 1e308 → can't represent as i64 fraction, should error
        assert!(to_fraction(1e308, None).is_err());
    }
}
