// Percentage and financial calculation tools — percentage operations,
// compound interest, loan payments, NPV, IRR, and ROI.
//
// All formulas are hand-rolled (no external crate needed). IRR uses
// Newton-Raphson with a bisection fallback. All rates are in percent
// (e.g. 5.0 means 5%).

use rmcp::model::CallToolResult;
use serde_json::json;

use crate::tools::{err_json, ok_json};

// ===== Request structs =====

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PercentageRequest {
    /// Mode: "percent_of" (b% of a), "what_percent" (a is what % of b),
    /// "percent_change" (from a to b), "increase" (a increased by b%),
    /// "decrease" (a decreased by b%).
    pub mode: String,
    /// Primary value (interpretation depends on mode).
    pub a: f64,
    /// Secondary value (interpretation depends on mode).
    pub b: f64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CompoundInterestRequest {
    /// Principal amount (initial investment).
    pub principal: f64,
    /// Annual interest rate in percent (e.g. 5.0 = 5%).
    pub annual_rate_pct: f64,
    /// Number of years.
    pub years: f64,
    /// Compounding frequency per year (default 12). Use 0 for continuous
    /// compounding.
    #[serde(default = "default_compounds_per_year")]
    pub compounds_per_year: f64,
}

fn default_compounds_per_year() -> f64 {
    12.0
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct LoanPaymentRequest {
    /// Principal amount (loan amount).
    pub principal: f64,
    /// Annual interest rate (APR) in percent.
    pub annual_rate_pct: f64,
    /// Loan term in years.
    pub years: f64,
    /// Payments per year (default 12 = monthly).
    #[serde(default = "default_payments_per_year")]
    pub payments_per_year: f64,
}

fn default_payments_per_year() -> f64 {
    12.0
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NetPresentValueRequest {
    /// Discount rate in percent (e.g. 10.0 = 10%).
    pub rate_pct: f64,
    /// Cash flows. cashflows[0] is at t=0 (not discounted), cashflows[1]
    /// at t=1, etc. NOTE: this differs from Excel's NPV which discounts
    /// the first cash flow by one period.
    pub cashflows: Vec<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct IrrRequest {
    /// Cash flows. Must have at least one sign change (positive and
    /// negative values) for IRR to be defined.
    pub cashflows: Vec<f64>,
    /// Initial guess for the rate in percent (default 10).
    #[serde(default = "default_guess_pct")]
    pub guess_pct: f64,
}

fn default_guess_pct() -> f64 {
    10.0
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RoiRequest {
    /// Gain (final value).
    pub gain: f64,
    /// Cost (initial investment).
    pub cost: f64,
}

// ===== Pure functions =====

/// Computes a percentage-related calculation based on the mode.
pub fn percentage(mode: &str, a: f64, b: f64) -> Result<f64, String> {
    let result = match mode {
        "percent_of" => a * b / 100.0,
        "what_percent" => {
            if b == 0.0 {
                return Err("Cannot divide by zero (b=0)".to_string());
            }
            a / b * 100.0
        }
        "percent_change" => {
            if a == 0.0 {
                return Err("Cannot compute percent change from zero (a=0)".to_string());
            }
            (b - a) / a * 100.0
        }
        "increase" => a * (1.0 + b / 100.0),
        "decrease" => a * (1.0 - b / 100.0),
        _ => {
            return Err(format!(
                "Unknown mode: {} (expected percent_of, what_percent, percent_change, increase, or decrease)",
                mode
            ))
        }
    };
    if !result.is_finite() {
        return Err("Result is too large (overflow)".to_string());
    }
    Ok(result)
}

/// Computes compound interest: future value, total interest, and effective
/// annual rate. compounds_per_year = 0 means continuous compounding.
pub fn compound_interest(
    principal: f64,
    annual_rate_pct: f64,
    years: f64,
    compounds_per_year: f64,
) -> (f64, f64, f64) {
    let r = annual_rate_pct / 100.0;
    let (fv, ear) = if compounds_per_year == 0.0 {
        // Continuous compounding: FV = P * e^(rt), EAR = e^r - 1
        (principal * (r * years).exp(), r.exp() - 1.0)
    } else {
        let n = compounds_per_year;
        let nt = n * years;
        (
            principal * (1.0 + r / n).powf(nt),
            (1.0 + r / n).powf(n) - 1.0,
        )
    };
    (fv, fv - principal, ear * 100.0)
}

/// Computes loan payment (amortization). Returns (payment, num_payments,
/// total_paid, total_interest).
pub fn loan_payment(
    principal: f64,
    annual_rate_pct: f64,
    years: f64,
    payments_per_year: f64,
) -> Result<(f64, f64, f64, f64), String> {
    if payments_per_year == 0.0 {
        return Err("payments_per_year cannot be zero".to_string());
    }
    let n = years * payments_per_year;
    let r = annual_rate_pct / 100.0 / payments_per_year;
    let payment = if r == 0.0 {
        principal / n
    } else {
        principal * r / (1.0 - (1.0 + r).powf(-n))
    };
    let total_paid = payment * n;
    Ok((payment, n, total_paid, total_paid - principal))
}

/// Computes net present value. cashflows[0] is at t=0 (not discounted).
pub fn net_present_value(rate_pct: f64, cashflows: &[f64]) -> f64 {
    let r = rate_pct / 100.0;
    cashflows
        .iter()
        .enumerate()
        .map(|(t, cf)| cf / (1.0 + r).powi(t as i32))
        .sum()
}

/// Computes the derivative of NPV with respect to the rate.
fn npv_derivative(rate: f64, cashflows: &[f64]) -> f64 {
    cashflows
        .iter()
        .enumerate()
        .skip(1)
        .map(|(t, cf)| -cf * (t as f64) / (1.0 + rate).powi(t as i32 + 1))
        .sum()
}

/// Checks if cashflows have at least one sign change.
fn has_sign_change(cashflows: &[f64]) -> bool {
    let positive = cashflows.iter().any(|&c| c > 0.0);
    let negative = cashflows.iter().any(|&c| c < 0.0);
    positive && negative
}

/// Computes the internal rate of return using Newton-Raphson with a
/// bisection fallback. Returns the rate as a percentage.
pub fn internal_rate_of_return(cashflows: &[f64], guess_pct: f64) -> Result<f64, String> {
    if cashflows.is_empty() {
        return Err("Cashflows cannot be empty".to_string());
    }
    if !has_sign_change(cashflows) {
        return Err(
            "IRR is undefined: cashflows must have at least one sign change (positive and negative values)"
                .to_string(),
        );
    }

    let mut rate = guess_pct / 100.0;

    // Newton-Raphson iteration
    for _ in 0..100 {
        let f = net_present_value(rate * 100.0, cashflows);
        if f.abs() < 1e-10 {
            return Ok(rate * 100.0);
        }
        let df = npv_derivative(rate, cashflows);
        if df.abs() < 1e-15 {
            break; // derivative too small, fall back to bisection
        }
        let new_rate = rate - f / df;
        if new_rate <= -0.9999 || new_rate > 1e6 {
            break; // diverged, fall back to bisection
        }
        if (new_rate - rate).abs() < 1e-12 {
            return Ok(new_rate * 100.0); // converged
        }
        rate = new_rate;
    }

    // Bisection fallback on [-0.9999, 100]
    let mut lo = -0.9999;
    let mut hi = 100.0;
    let mut npv_lo = net_present_value(lo * 100.0, cashflows);
    let npv_hi = net_present_value(hi * 100.0, cashflows);

    if npv_lo * npv_hi > 0.0 {
        return Err(
            "IRR could not be found: NPV does not change sign in the search interval".to_string(),
        );
    }

    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let npv_mid = net_present_value(mid * 100.0, cashflows);
        if npv_mid.abs() < 1e-10 {
            return Ok(mid * 100.0);
        }
        if npv_lo * npv_mid < 0.0 {
            hi = mid;
        } else {
            lo = mid;
            npv_lo = npv_mid;
        }
    }

    Ok(((lo + hi) / 2.0) * 100.0)
}

/// Computes return on investment. Returns (roi_pct, net_gain).
pub fn return_on_investment(gain: f64, cost: f64) -> Result<(f64, f64), String> {
    if cost == 0.0 {
        return Err("Cost cannot be zero".to_string());
    }
    Ok(((gain - cost) / cost * 100.0, gain - cost))
}

// ===== Tool wrappers =====

pub fn percentage_tool(req: PercentageRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match percentage(&req.mode, req.a, req.b) {
        Ok(result) => Ok(ok_json(json!({"result": result}))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn compound_interest_tool(
    req: CompoundInterestRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let (fv, total_interest, ear) = compound_interest(
        req.principal,
        req.annual_rate_pct,
        req.years,
        req.compounds_per_year,
    );
    Ok(ok_json(json!({
        "future_value": fv,
        "total_interest": total_interest,
        "effective_annual_rate_pct": ear
    })))
}

pub fn loan_payment_tool(req: LoanPaymentRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match loan_payment(
        req.principal,
        req.annual_rate_pct,
        req.years,
        req.payments_per_year,
    ) {
        Ok((payment, num_payments, total_paid, total_interest)) => Ok(ok_json(json!({
            "payment": payment,
            "num_payments": num_payments,
            "total_paid": total_paid,
            "total_interest": total_interest
        }))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn net_present_value_tool(
    req: NetPresentValueRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let result = net_present_value(req.rate_pct, &req.cashflows);
    Ok(ok_json(json!({"npv": result})))
}

pub fn internal_rate_of_return_tool(req: IrrRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match internal_rate_of_return(&req.cashflows, req.guess_pct) {
        Ok(result) => Ok(ok_json(json!({"irr_pct": result}))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn return_on_investment_tool(req: RoiRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match return_on_investment(req.gain, req.cost) {
        Ok((roi_pct, net_gain)) => Ok(ok_json(json!({"roi_pct": roi_pct, "net_gain": net_gain}))),
        Err(e) => Ok(err_json(&e)),
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    // --- percentage ---

    #[test]
    fn test_percentage_of() {
        assert!((percentage("percent_of", 200.0, 15.0).unwrap() - 30.0).abs() < 1e-10);
    }

    #[test]
    fn test_percentage_what_percent() {
        assert!((percentage("what_percent", 30.0, 200.0).unwrap() - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_percentage_change() {
        assert!((percentage("percent_change", 100.0, 150.0).unwrap() - 50.0).abs() < 1e-10);
    }

    #[test]
    fn test_percentage_increase() {
        assert!((percentage("increase", 100.0, 20.0).unwrap() - 120.0).abs() < 1e-10);
    }

    #[test]
    fn test_percentage_decrease() {
        assert!((percentage("decrease", 100.0, 20.0).unwrap() - 80.0).abs() < 1e-10);
    }

    #[test]
    fn test_percentage_errors() {
        assert!(percentage("what_percent", 1.0, 0.0).is_err());
        assert!(percentage("percent_change", 0.0, 100.0).is_err());
        assert!(percentage("invalid", 1.0, 1.0).is_err());
    }

    // --- compound_interest ---

    #[test]
    fn test_compound_interest_monthly() {
        let (fv, interest, ear) = compound_interest(1000.0, 5.0, 10.0, 12.0);
        assert!((fv - 1647.01).abs() < 0.1);
        assert!((interest - 647.01).abs() < 0.1);
        assert!((ear - 5.116).abs() < 0.01);
    }

    #[test]
    fn test_compound_interest_continuous() {
        let (fv, _interest, ear) = compound_interest(1000.0, 5.0, 10.0, 0.0);
        assert!((fv - 1648.72).abs() < 0.1);
        assert!((ear - 5.127).abs() < 0.01);
    }

    // --- loan_payment ---

    #[test]
    fn test_loan_payment() {
        let (payment, n, total_paid, total_interest) =
            loan_payment(10000.0, 6.0, 30.0, 12.0).unwrap();
        assert!((payment - 59.96).abs() < 0.1);
        assert_eq!(n, 360.0);
        assert!((total_paid - 21583.0).abs() < 1.0);
        assert!((total_interest - 11583.0).abs() < 1.0);
    }

    #[test]
    fn test_loan_payment_zero_interest() {
        let (payment, n, total_paid, total_interest) =
            loan_payment(12000.0, 0.0, 1.0, 12.0).unwrap();
        assert!((payment - 1000.0).abs() < 1e-10);
        assert_eq!(n, 12.0);
        assert!((total_paid - 12000.0).abs() < 1e-10);
        assert!((total_interest - 0.0).abs() < 1e-10);
    }

    // --- net_present_value ---

    #[test]
    fn test_npv() {
        let result = net_present_value(10.0, &[-100.0, 50.0, 60.0]);
        assert!((result - (-4.96)).abs() < 0.1);
    }

    #[test]
    fn test_npv_first_cashflow_not_discounted() {
        // cashflows[0] at t=0: NPV = 100 + 110/1.1 = 100 + 100 = 200
        let result = net_present_value(10.0, &[100.0, 110.0]);
        assert!((result - 200.0).abs() < 0.1);
    }

    // --- internal_rate_of_return ---

    #[test]
    fn test_irr_simple() {
        // [-100, 110] → IRR = 10%
        let irr = internal_rate_of_return(&[-100.0, 110.0], 10.0).unwrap();
        assert!((irr - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_irr_multi_period() {
        // [-100, 50, 50, 50] → IRR ≈ 23.4%
        let irr = internal_rate_of_return(&[-100.0, 50.0, 50.0, 50.0], 10.0).unwrap();
        assert!((irr - 23.4).abs() < 0.5);
    }

    #[test]
    fn test_irr_no_sign_change() {
        assert!(internal_rate_of_return(&[100.0, 50.0, 50.0], 10.0).is_err());
    }

    #[test]
    fn test_irr_empty() {
        assert!(internal_rate_of_return(&[], 10.0).is_err());
    }

    // --- return_on_investment ---

    #[test]
    fn test_roi() {
        let (roi_pct, net_gain) = return_on_investment(1500.0, 1000.0).unwrap();
        assert!((roi_pct - 50.0).abs() < 1e-10);
        assert!((net_gain - 500.0).abs() < 1e-10);
    }

    #[test]
    fn test_roi_zero_cost() {
        assert!(return_on_investment(100.0, 0.0).is_err());
    }

    // --- overflow / edge case tests ---

    #[test]
    fn test_percentage_overflow() {
        // 1e308 * 100 = infinity → should error, not return null
        assert!(percentage("percent_of", 1e308, 100.0).is_err());
    }
}
