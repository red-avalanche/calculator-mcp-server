// Programmer / integer calculation tools — base conversion and bitwise
// operations. All operations use i64 internally; values exceeding 2^53
// should be passed as JSON strings to avoid floating-point precision loss.

use rmcp::model::CallToolResult;

use crate::tools::{err_json, ok_json};

// ===== Request structs =====

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BaseConvertRequest {
    /// The value to convert, as a string. Supports 0x/0o/0b prefixes for
    /// hex/octal/binary; bare values are decimal. Negative values are
    /// supported (e.g. "-0xFF"). Must fit in i64 range.
    pub value: String,
    /// Target base: 2, 8, 10, or 16.
    pub to_base: i64,
    /// Source base: 2, 8, 10, or 16. If omitted, auto-detected from prefix
    /// (0x -> 16, 0o -> 8, 0b -> 2, else 10).
    pub from_base: Option<i64>,
}

/// Accepts an integer as either a JSON string or a JSON number.
/// Strings are required for values exceeding 2^53 to avoid precision loss.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum IntInput {
    /// Integer as a string (required for values exceeding 2^53).
    S(String),
    /// Integer as a JSON number.
    I(i64),
}

impl IntInput {
    fn parse(&self) -> Result<i64, String> {
        match self {
            IntInput::S(s) => s
                .trim()
                .parse::<i64>()
                .map_err(|_| format!("Cannot parse '{}' as integer", s)),
            IntInput::I(n) => Ok(*n),
        }
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BitwiseRequest {
    /// Bitwise operation: "and", "or", "xor", "not", "shl", or "shr".
    pub operation: String,
    /// First operand (string or integer). For "not", this is the value to
    /// invert. Interpreted as two's complement when signed=true.
    pub a: IntInput,
    /// Second operand (string or integer). Required for "and", "or", "xor",
    /// "shl", "shr". Must NOT be provided for "not".
    #[serde(default)]
    pub b: Option<IntInput>,
    /// Bit width: 8, 16, 32, or 64 (default 64). Values are masked to this
    /// width. Example: 0xFF as signed 8-bit = -1.
    #[serde(default = "default_bit_width")]
    pub bit_width: i64,
    /// If true, interpret values as signed two's complement (default false).
    #[serde(default)]
    pub signed: bool,
}

fn default_bit_width() -> i64 {
    64
}

// ===== Response structs =====

#[derive(Debug, serde::Serialize)]
pub struct BaseConvertResult {
    pub decimal: String,
    pub hex: String,
    pub octal: String,
    pub binary: String,
    pub result: String,
}

#[derive(Debug, serde::Serialize)]
pub struct BitwiseResult {
    pub value: String,
    pub unsigned_value: String,
    pub hex: String,
    pub octal: String,
    pub binary: String,
}

// ===== Helper functions =====

fn mask_for_width(bit_width: i64) -> u64 {
    match bit_width {
        64 => u64::MAX,
        _ => (1u64 << bit_width) - 1,
    }
}

/// Reinterprets the low `bit_width` bits of a u64 as a signed integer
/// (two's complement).
fn to_signed(val: u64, bit_width: i64) -> i64 {
    let mask = mask_for_width(bit_width);
    let sign_bit = 1u64 << (bit_width - 1);
    if val & sign_bit != 0 {
        // Sign-extend: set all bits above bit_width
        (val | !mask) as i64
    } else {
        val as i64
    }
}

/// Validates that `value` fits in the given bit_width and signedness.
/// For signed, the value is masked to bit_width first (two's complement
/// interpretation), so any i64 is valid. For unsigned, negative values
/// and values exceeding the bit_width are rejected.
fn validate_range(value: i64, bit_width: i64, signed: bool) -> Result<(), String> {
    if signed {
        // For signed, any i64 is valid — masking + sign-extension handles
        // the interpretation. The range check is implicitly satisfied.
        return Ok(());
    }
    if value < 0 {
        return Err(format!(
            "Value {} is negative but signed=false (unsigned {}-bit range is [0, {}])",
            value,
            bit_width,
            mask_for_width(bit_width)
        ));
    }
    if bit_width < 64 {
        let max = mask_for_width(bit_width) as i64;
        if value > max {
            return Err(format!(
                "Value {} is outside unsigned {}-bit range [0, {}]",
                value, bit_width, max
            ));
        }
    }
    Ok(())
}

/// Extracts a leading sign (+ or -) from a string, returning (sign, rest).
fn extract_sign(s: &str) -> (i64, &str) {
    if let Some(r) = s.strip_prefix('-') {
        (-1, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (1, r)
    } else {
        (1, s)
    }
}

/// Strips a base prefix (0x, 0o, 0b) from a string if it matches the base.
fn strip_base_prefix(s: &str, base: i64) -> &str {
    match base {
        16 => s
            .strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(s),
        8 => s
            .strip_prefix("0o")
            .or_else(|| s.strip_prefix("0O"))
            .unwrap_or(s),
        2 => s
            .strip_prefix("0b")
            .or_else(|| s.strip_prefix("0B"))
            .unwrap_or(s),
        _ => s,
    }
}

// ===== Pure functions =====

pub fn base_convert(
    value: &str,
    to_base: i64,
    from_base: Option<i64>,
) -> Result<BaseConvertResult, String> {
    if !matches!(to_base, 2 | 8 | 10 | 16) {
        return Err(format!("to_base must be 2, 8, 10, or 16, got {}", to_base));
    }

    let trimmed = value.trim();

    // Determine source base, sign, and digits.
    let (base, sign, digits) = match from_base {
        Some(b) => {
            if !matches!(b, 2 | 8 | 10 | 16) {
                return Err(format!("from_base must be 2, 8, 10, or 16, got {}", b));
            }
            let (sign, rest) = extract_sign(trimmed);
            let digits = strip_base_prefix(rest, b);
            (b, sign, digits)
        }
        None => {
            let (sign, rest) = extract_sign(trimmed);
            if let Some(d) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
                (16, sign, d)
            } else if let Some(d) = rest.strip_prefix("0o").or_else(|| rest.strip_prefix("0O")) {
                (8, sign, d)
            } else if let Some(d) = rest.strip_prefix("0b").or_else(|| rest.strip_prefix("0B")) {
                (2, sign, d)
            } else {
                (10, sign, rest)
            }
        }
    };

    let n = i64::from_str_radix(digits, base as u32)
        .map(|v| sign * v)
        .map_err(|e| format!("Failed to parse '{}' as base {}: {}", digits, base, e))?;

    // Format all representations (sign-magnitude form, NOT two's complement).
    let decimal = n.to_string();
    let hex = if n < 0 {
        format!("-0x{:x}", n.abs())
    } else {
        format!("0x{:x}", n)
    };
    let octal = if n < 0 {
        format!("-0o{:o}", n.abs())
    } else {
        format!("0o{:o}", n)
    };
    let binary = if n < 0 {
        format!("-0b{:b}", n.abs())
    } else {
        format!("0b{:b}", n)
    };

    let result = match to_base {
        10 => decimal.clone(),
        16 => hex.clone(),
        8 => octal.clone(),
        2 => binary.clone(),
        _ => unreachable!(),
    };

    Ok(BaseConvertResult {
        decimal,
        hex,
        octal,
        binary,
        result,
    })
}

pub fn bitwise(
    operation: &str,
    a: IntInput,
    b: Option<IntInput>,
    bit_width: i64,
    signed: bool,
) -> Result<BitwiseResult, String> {
    if !matches!(bit_width, 8 | 16 | 32 | 64) {
        return Err(format!(
            "bit_width must be 8, 16, 32, or 64, got {}",
            bit_width
        ));
    }

    if !matches!(operation, "and" | "or" | "xor" | "not" | "shl" | "shr") {
        return Err(format!(
            "Unknown operation: {} (expected and, or, xor, not, shl, or shr)",
            operation
        ));
    }

    let needs_b = matches!(operation, "and" | "or" | "xor" | "shl" | "shr");
    let forbids_b = operation == "not";

    if needs_b && b.is_none() {
        return Err(format!(
            "Operation '{}' requires a second operand 'b'",
            operation
        ));
    }
    if forbids_b && b.is_some() {
        return Err("Operation 'not' does not accept a second operand 'b'".to_string());
    }

    // Parse and validate a
    let a_val = a.parse()?;
    validate_range(a_val, bit_width, signed)?;
    let mask = mask_for_width(bit_width);
    let a_u64 = (a_val as u64) & mask;

    // Parse and validate b if needed
    let b_val = if needs_b {
        let bv = b.unwrap().parse()?;
        if operation == "shl" || operation == "shr" {
            if bv < 0 || bv >= bit_width {
                return Err(format!(
                    "Shift count {} is out of range [0, {})",
                    bv, bit_width
                ));
            }
        } else {
            validate_range(bv, bit_width, signed)?;
        }
        bv
    } else {
        0
    };

    let b_u64 = if matches!(operation, "and" | "or" | "xor") {
        (b_val as u64) & mask
    } else {
        b_val as u64
    };

    // Perform the operation
    let result_u64: u64 = match operation {
        "and" => (a_u64 & b_u64) & mask,
        "or" => (a_u64 | b_u64) & mask,
        "xor" => (a_u64 ^ b_u64) & mask,
        "not" => (!a_u64) & mask,
        "shl" => (a_u64 << (b_val as u32)) & mask,
        "shr" => {
            if signed {
                let signed_a = to_signed(a_u64, bit_width);
                ((signed_a >> (b_val as u32)) as u64) & mask
            } else {
                a_u64 >> (b_val as u32)
            }
        }
        _ => unreachable!(),
    };

    let value = if signed {
        to_signed(result_u64, bit_width).to_string()
    } else {
        result_u64.to_string()
    };

    let hex_width = (bit_width / 4) as usize;
    let oct_width = ((bit_width + 2) / 3) as usize;
    let bin_width = bit_width as usize;

    Ok(BitwiseResult {
        value,
        unsigned_value: result_u64.to_string(),
        hex: format!("0x{:0width$x}", result_u64, width = hex_width),
        octal: format!("0o{:0width$o}", result_u64, width = oct_width),
        binary: format!("0b{:0width$b}", result_u64, width = bin_width),
    })
}

// ===== Tool wrappers =====

pub fn base_convert_tool(req: BaseConvertRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match base_convert(&req.value, req.to_base, req.from_base) {
        Ok(result) => Ok(ok_json(result)),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn bitwise_tool(req: BitwiseRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match bitwise(&req.operation, req.a, req.b, req.bit_width, req.signed) {
        Ok(result) => Ok(ok_json(result)),
        Err(e) => Ok(err_json(&e)),
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    // --- base_convert ---

    #[test]
    fn test_base_convert_prefix_detection() {
        let r = base_convert("0xFF", 10, None).unwrap();
        assert_eq!(r.decimal, "255");
        assert_eq!(r.hex, "0xff");
        assert_eq!(r.octal, "0o377");
        assert_eq!(r.binary, "0b11111111");
        assert_eq!(r.result, "255");
    }

    #[test]
    fn test_base_convert_explicit_from_base() {
        let r = base_convert("FF", 10, Some(16)).unwrap();
        assert_eq!(r.decimal, "255");
    }

    #[test]
    fn test_base_convert_negative() {
        let r = base_convert("-0xFF", 10, None).unwrap();
        assert_eq!(r.decimal, "-255");
        assert_eq!(r.hex, "-0xff");
    }

    #[test]
    fn test_base_convert_to_hex() {
        let r = base_convert("255", 16, None).unwrap();
        assert_eq!(r.result, "0xff");
    }

    #[test]
    fn test_base_convert_invalid_digit() {
        assert!(base_convert("0xGG", 10, None).is_err());
    }

    #[test]
    fn test_base_convert_invalid_base() {
        assert!(base_convert("10", 3, None).is_err());
    }

    // --- bitwise ---

    #[test]
    fn test_bitwise_and() {
        let r = bitwise("and", IntInput::I(0xFF), Some(IntInput::I(0x0F)), 8, false).unwrap();
        assert_eq!(r.unsigned_value, "15");
        assert_eq!(r.hex, "0x0f");
    }

    #[test]
    fn test_bitwise_or() {
        let r = bitwise("or", IntInput::I(0xF0), Some(IntInput::I(0x0F)), 8, false).unwrap();
        assert_eq!(r.unsigned_value, "255");
        assert_eq!(r.hex, "0xff");
    }

    #[test]
    fn test_bitwise_xor() {
        let r = bitwise("xor", IntInput::I(0xFF), Some(IntInput::I(0x0F)), 8, false).unwrap();
        assert_eq!(r.unsigned_value, "240");
        assert_eq!(r.hex, "0xf0");
    }

    #[test]
    fn test_bitwise_not() {
        let r = bitwise("not", IntInput::I(0x0F), None, 8, false).unwrap();
        assert_eq!(r.unsigned_value, "240");
        assert_eq!(r.hex, "0xf0");
        assert_eq!(r.binary, "0b11110000");
    }

    #[test]
    fn test_bitwise_signed_twos_complement() {
        // 0xFF as signed 8-bit = -1
        let r = bitwise("and", IntInput::I(0xFF), Some(IntInput::I(0xFF)), 8, true).unwrap();
        assert_eq!(r.value, "-1");
        assert_eq!(r.unsigned_value, "255");
    }

    #[test]
    fn test_bitwise_shl() {
        let r = bitwise("shl", IntInput::I(0x0F), Some(IntInput::I(4)), 8, false).unwrap();
        assert_eq!(r.unsigned_value, "240");
        assert_eq!(r.hex, "0xf0");
    }

    #[test]
    fn test_bitwise_shl_wrap() {
        // 0xFF << 4 in 8-bit = 0xF0 (upper bits dropped)
        let r = bitwise("shl", IntInput::I(0xFF), Some(IntInput::I(4)), 8, false).unwrap();
        assert_eq!(r.hex, "0xf0");
    }

    #[test]
    fn test_bitwise_shr_unsigned() {
        let r = bitwise("shr", IntInput::I(0x80), Some(IntInput::I(4)), 8, false).unwrap();
        assert_eq!(r.unsigned_value, "8");
        assert_eq!(r.hex, "0x08");
    }

    #[test]
    fn test_bitwise_shr_signed_arithmetic() {
        // 0x80 as signed 8-bit = -128; -128 >> 4 = -8 (arithmetic shift)
        let r = bitwise("shr", IntInput::I(0x80), Some(IntInput::I(4)), 8, true).unwrap();
        assert_eq!(r.value, "-8");
        assert_eq!(r.hex, "0xf8");
    }

    #[test]
    fn test_bitwise_shift_out_of_range() {
        assert!(bitwise("shl", IntInput::I(1), Some(IntInput::I(8)), 8, false).is_err());
        assert!(bitwise("shr", IntInput::I(1), Some(IntInput::I(-1)), 8, false).is_err());
    }

    #[test]
    fn test_bitwise_missing_b() {
        assert!(bitwise("and", IntInput::I(1), None, 8, false).is_err());
    }

    #[test]
    fn test_bitwise_not_forbids_b() {
        assert!(bitwise("not", IntInput::I(1), Some(IntInput::I(2)), 8, false).is_err());
    }

    #[test]
    fn test_bitwise_out_of_range() {
        // 256 is outside unsigned 8-bit range [0, 255]
        assert!(bitwise("and", IntInput::I(256), Some(IntInput::I(1)), 8, false).is_err());
        // -1 is negative but signed=false
        assert!(bitwise("and", IntInput::I(-1), Some(IntInput::I(1)), 8, false).is_err());
    }

    #[test]
    fn test_bitwise_string_input() {
        let r = bitwise(
            "and",
            IntInput::S("255".to_string()),
            Some(IntInput::S("15".to_string())),
            8,
            false,
        )
        .unwrap();
        assert_eq!(r.unsigned_value, "15");
    }

    #[test]
    fn test_bitwise_64bit() {
        let r = bitwise("not", IntInput::I(0), None, 64, false).unwrap();
        assert_eq!(r.value, "18446744073709551615");
        assert_eq!(r.hex, "0xffffffffffffffff");
    }
}
