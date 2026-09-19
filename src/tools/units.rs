// Unit conversion and quantity arithmetic tools.
//
// Uses a factor/offset conversion table for all quantities. The `measurements`
// crate is included as a dependency (per plan) but the conversion logic is
// implemented directly for simpler, more flexible unit handling.
//
// Temperature uses affine offsets (°C/°F/K). Data sizes distinguish SI (KB=1000)
// from IEC (KiB=1024). Duration is hand-rolled (ms, s, min, h, d, wk) since
// `measurements` lacks a time-duration quantity. Months/years are intentionally
// excluded (not fixed durations — use date_add/date_diff instead).

use rmcp::model::CallToolResult;
use serde_json::json;

use crate::tools::{err_json, ok_json};

// ===== Request structs =====

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ConvertUnitRequest {
    /// The value to convert.
    pub value: f64,
    /// Source unit (e.g. "km", "lb", "C", "KB", "deg"). See tool description
    /// for the full list of supported units.
    pub from_unit: String,
    /// Target unit (e.g. "mi", "kg", "F", "MiB", "rad"). Must be the same
    /// quantity as from_unit.
    pub to_unit: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QuantityArithmeticRequest {
    /// Left operand: value and unit.
    pub left: Operand,
    /// Right operand: value and unit. Must be the same quantity as left.
    pub right: Operand,
    /// Operation: "add" or "subtract".
    pub operation: String,
    /// Unit for the result (optional, defaults to left's unit).
    pub result_unit: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Operand {
    /// The numerical value.
    pub value: f64,
    /// The unit (e.g. "km", "m", "lb").
    pub unit: String,
}

// ===== Unit conversion table =====

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuantityKind {
    Length,
    Mass,
    Temperature,
    Volume,
    Speed,
    Area,
    Duration,
    Data,
    Pressure,
    Energy,
    Power,
    Force,
    Angle,
}

impl QuantityKind {
    fn name(&self) -> &'static str {
        match self {
            QuantityKind::Length => "length",
            QuantityKind::Mass => "mass",
            QuantityKind::Temperature => "temperature",
            QuantityKind::Volume => "volume",
            QuantityKind::Speed => "speed",
            QuantityKind::Area => "area",
            QuantityKind::Duration => "duration",
            QuantityKind::Data => "data",
            QuantityKind::Pressure => "pressure",
            QuantityKind::Energy => "energy",
            QuantityKind::Power => "power",
            QuantityKind::Force => "force",
            QuantityKind::Angle => "angle",
        }
    }
}

struct UnitDef {
    quantity: QuantityKind,
    factor: f64, // multiply by this to convert to base unit
    offset: f64, // add this before multiplying (for temperature)
}

impl UnitDef {
    fn convert_to_base(&self, v: f64) -> f64 {
        (v + self.offset) * self.factor
    }
    fn convert_from_base(&self, v: f64) -> f64 {
        v / self.factor - self.offset
    }
}

/// Looks up a unit string, returning its definition.
/// Data units (B, KB, KiB, etc.) are case-sensitive; all others are
/// case-insensitive.
fn lookup_unit(unit: &str) -> Result<UnitDef, String> {
    let u = unit.trim();

    // --- Data units: case-sensitive (B=byte, b=bit, KB vs KiB) ---
    match u {
        "B" | "byte" | "bytes" => return Ok(u_data(1.0)),
        "b" | "bit" | "bits" => return Ok(u_data(0.125)),
        "KB" => return Ok(u_data(1000.0)),
        "MB" => return Ok(u_data(1e6)),
        "GB" => return Ok(u_data(1e9)),
        "TB" => return Ok(u_data(1e12)),
        "KiB" => return Ok(u_data(1024.0)),
        "MiB" => return Ok(u_data(1048576.0)),
        "GiB" => return Ok(u_data(1073741824.0)),
        "TiB" => return Ok(u_data(1099511627776.0)),
        // kN = kilonewton (case-sensitive, to avoid collision with kn = knots)
        "kN" => return Ok(u_force(1000.0)),
        _ => {}
    }

    // --- Months/years: not supported, point to date tools ---
    match u.to_lowercase().as_str() {
        "month" | "months" | "mo" | "year" | "years" | "yr" => {
            return Err(
                "Months and years are not fixed durations. Use date_add or date_diff instead."
                    .to_string(),
            );
        }
        _ => {}
    }

    // --- All other units: case-insensitive ---
    match u.to_lowercase().as_str() {
        // length (base: meters)
        "m" | "meter" | "meters" | "metre" | "metres" => Ok(u_len(1.0)),
        "km" | "kilometer" | "kilometers" | "kilometre" | "kilometres" => Ok(u_len(1000.0)),
        "cm" | "centimeter" | "centimeters" => Ok(u_len(0.01)),
        "mm" | "millimeter" | "millimeters" => Ok(u_len(0.001)),
        "mi" | "mile" | "miles" => Ok(u_len(1609.344)),
        "yd" | "yard" | "yards" => Ok(u_len(0.9144)),
        "ft" | "foot" | "feet" => Ok(u_len(0.3048)),
        "in" | "inch" | "inches" => Ok(u_len(0.0254)),
        "nmi" | "nautical_mile" | "nauticalmile" => Ok(u_len(1852.0)),

        // mass (base: kilograms)
        "kg" | "kilogram" | "kilograms" => Ok(u_mass(1.0)),
        "g" | "gram" | "grams" => Ok(u_mass(0.001)),
        "mg" | "milligram" | "milligrams" => Ok(u_mass(1e-6)),
        "t" | "tonne" | "tonnes" => Ok(u_mass(1000.0)),
        "lb" | "lbs" | "pound" | "pounds" => Ok(u_mass(0.45359237)),
        "oz" | "ounce" | "ounces" => Ok(u_mass(0.028349523125)),
        "st" | "stone" => Ok(u_mass(6.35029318)),

        // temperature (base: Celsius, affine)
        "c" | "celsius" => Ok(UnitDef {
            quantity: QuantityKind::Temperature,
            factor: 1.0,
            offset: 0.0,
        }),
        "f" | "fahrenheit" => Ok(UnitDef {
            quantity: QuantityKind::Temperature,
            factor: 5.0 / 9.0,
            offset: -32.0,
        }),
        "k" | "kelvin" => Ok(UnitDef {
            quantity: QuantityKind::Temperature,
            factor: 1.0,
            offset: -273.15,
        }),

        // volume (base: liters)
        "l" | "liter" | "liters" | "litre" | "litres" => Ok(u_vol(1.0)),
        "ml" | "milliliter" | "milliliters" => Ok(u_vol(0.001)),
        "m3" | "m^3" | "m³" | "cubic_meter" | "cubic_meters" => Ok(u_vol(1000.0)),
        "gal" | "gallon" | "gallons" => Ok(u_vol(3.785411784)),
        "qt" | "quart" | "quarts" => Ok(u_vol(0.946352946)),
        "pt" | "pint" | "pints" => Ok(u_vol(0.473176473)),
        "cup" | "cups" => Ok(u_vol(0.2365882365)),
        "floz" | "fl_oz" | "fluid_ounce" | "fluid_ounces" => Ok(u_vol(0.0295735295625)),

        // speed (base: m/s)
        "m/s" | "mps" => Ok(u_speed(1.0)),
        "km/h" | "kph" => Ok(u_speed(1000.0 / 3600.0)),
        "mph" => Ok(u_speed(0.44704)),
        "kn" | "knot" | "knots" => Ok(u_speed(1852.0 / 3600.0)),

        // area (base: m²)
        "m2" | "m^2" | "m²" | "sq_m" | "square_meter" => Ok(u_area(1.0)),
        "km2" | "km^2" | "km²" | "sq_km" => Ok(u_area(1e6)),
        "ha" | "hectare" | "hectares" => Ok(u_area(10000.0)),
        "acre" | "acres" => Ok(u_area(4046.8564224)),
        "ft2" | "ft^2" | "ft²" | "sq_ft" => Ok(u_area(0.09290304)),

        // duration (base: seconds, hand-rolled)
        "ms" | "millisecond" | "milliseconds" => Ok(u_dur(0.001)),
        "s" | "sec" | "second" | "seconds" => Ok(u_dur(1.0)),
        "min" | "minute" | "minutes" => Ok(u_dur(60.0)),
        "h" | "hr" | "hour" | "hours" => Ok(u_dur(3600.0)),
        "d" | "day" | "days" => Ok(u_dur(86400.0)),
        "wk" | "week" | "weeks" => Ok(u_dur(604800.0)),

        // pressure (base: pascals)
        "pa" | "pascal" | "pascals" => Ok(u_pres(1.0)),
        "kpa" => Ok(u_pres(1000.0)),
        "bar" | "bars" => Ok(u_pres(100000.0)),
        "atm" | "atmosphere" | "atmospheres" => Ok(u_pres(101325.0)),
        "psi" => Ok(u_pres(6894.757)),
        "mmhg" | "mm_hg" => Ok(u_pres(133.322)),

        // energy (base: joules)
        "j" | "joule" | "joules" => Ok(u_energy(1.0)),
        "kj" => Ok(u_energy(1000.0)),
        "cal" | "calorie" | "calories" => Ok(u_energy(4.184)),
        "kcal" | "kilocalorie" | "kilocalories" => Ok(u_energy(4184.0)),
        "wh" | "watt_hour" | "watt_hours" => Ok(u_energy(3600.0)),
        "kwh" | "kilowatt_hour" | "kilowatt_hours" => Ok(u_energy(3600000.0)),
        "btu" | "btus" => Ok(u_energy(1055.05585262)),

        // power (base: watts)
        "w" | "watt" | "watts" => Ok(u_power(1.0)),
        "kw" | "kilowatt" | "kilowatts" => Ok(u_power(1000.0)),
        "mw" | "megawatt" | "megawatts" => Ok(u_power(1000000.0)),
        "hp" | "horsepower" => Ok(u_power(745.699872)),

        // force (base: newtons)
        "n" | "newton" | "newtons" => Ok(u_force(1.0)),
        "kilonewton" | "kilonewtons" => Ok(u_force(1000.0)),
        "lbf" | "pound_force" => Ok(u_force(4.4482216152605)),

        // angle (base: radians)
        "rad" | "radian" | "radians" => Ok(u_angle(1.0)),
        "deg" | "degree" | "degrees" => Ok(u_angle(std::f64::consts::PI / 180.0)),
        "grad" | "gradian" | "gradians" | "gon" | "gons" => {
            Ok(u_angle(std::f64::consts::PI / 200.0))
        }

        _ => Err(format!(
            "Unknown unit: '{}'. Supported units: m, km, cm, mm, mi, yd, ft, in, nmi, \
             kg, g, mg, t, lb, oz, st, C, F, K, l, ml, m3, gal, qt, pt, cup, floz, \
             m/s, km/h, mph, kn, m2, km2, ha, acre, ft2, ms, s, min, h, d, wk, \
             B, KB, MB, GB, TB, KiB, MiB, GiB, TiB, bit, \
             Pa, kPa, bar, atm, psi, mmHg, J, kJ, cal, kcal, Wh, kWh, BTU, \
             W, kW, MW, hp, N, kN, lbf, deg, rad, grad",
            unit
        )),
    }
}

// Constructor helpers for each quantity
fn u_len(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Length,
        factor: f,
        offset: 0.0,
    }
}
fn u_mass(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Mass,
        factor: f,
        offset: 0.0,
    }
}
fn u_vol(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Volume,
        factor: f,
        offset: 0.0,
    }
}
fn u_speed(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Speed,
        factor: f,
        offset: 0.0,
    }
}
fn u_area(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Area,
        factor: f,
        offset: 0.0,
    }
}
fn u_dur(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Duration,
        factor: f,
        offset: 0.0,
    }
}
fn u_data(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Data,
        factor: f,
        offset: 0.0,
    }
}
fn u_pres(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Pressure,
        factor: f,
        offset: 0.0,
    }
}
fn u_energy(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Energy,
        factor: f,
        offset: 0.0,
    }
}
fn u_power(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Power,
        factor: f,
        offset: 0.0,
    }
}
fn u_force(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Force,
        factor: f,
        offset: 0.0,
    }
}
fn u_angle(f: f64) -> UnitDef {
    UnitDef {
        quantity: QuantityKind::Angle,
        factor: f,
        offset: 0.0,
    }
}

// ===== Pure functions =====

/// Converts a value from one unit to another.
pub fn convert_unit(value: f64, from_unit: &str, to_unit: &str) -> Result<f64, String> {
    let from = lookup_unit(from_unit)?;
    let to = lookup_unit(to_unit)?;

    if from.quantity != to.quantity {
        return Err(format!(
            "Cannot convert from {} ({}) to {} ({}) — different quantities",
            from_unit,
            from.quantity.name(),
            to_unit,
            to.quantity.name()
        ));
    }

    let base = from.convert_to_base(value);

    // Absolute zero check for temperature (base is Celsius)
    if from.quantity == QuantityKind::Temperature && base < -273.15 {
        return Err(format!(
            "Temperature {} {} is below absolute zero (-273.15 C / 0 K)",
            value, from_unit
        ));
    }

    Ok(to.convert_from_base(base))
}

/// Performs arithmetic on two quantities (add or subtract).
pub fn quantity_arithmetic(
    left: &Operand,
    right: &Operand,
    operation: &str,
    result_unit: Option<&str>,
) -> Result<(f64, String), String> {
    let left_def = lookup_unit(&left.unit)?;
    let right_def = lookup_unit(&right.unit)?;

    if left_def.quantity != right_def.quantity {
        return Err(format!(
            "Cannot {} {} and {} — different quantities ({})",
            operation,
            left.unit,
            right.unit,
            left_def.quantity.name()
        ));
    }

    // Temperature is rejected (ambiguous delta-vs-absolute semantics)
    if left_def.quantity == QuantityKind::Temperature {
        return Err(
            "Quantity arithmetic is not supported for temperature (ambiguous delta-vs-absolute \
             semantics). Use convert_unit for temperature conversion instead."
                .to_string(),
        );
    }

    let left_base = left_def.convert_to_base(left.value);
    let right_base = right_def.convert_to_base(right.value);

    let result_base = match operation {
        "add" => left_base + right_base,
        "subtract" => left_base - right_base,
        _ => {
            return Err(format!(
                "Unknown operation: {} (expected 'add' or 'subtract')",
                operation
            ))
        }
    };

    // Determine result unit
    let (result_def, result_unit_name) = match result_unit {
        Some(u) => {
            let def = lookup_unit(u)?;
            if def.quantity != left_def.quantity {
                return Err(format!(
                    "Result unit {} ({}) does not match operand quantity ({})",
                    u,
                    def.quantity.name(),
                    left_def.quantity.name()
                ));
            }
            (def, u.to_string())
        }
        None => (left_def, left.unit.clone()),
    };

    Ok((result_def.convert_from_base(result_base), result_unit_name))
}

// ===== Tool wrappers =====

pub fn convert_unit_tool(req: ConvertUnitRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match convert_unit(req.value, &req.from_unit, &req.to_unit) {
        Ok(result) => Ok(ok_json(json!({"result": result}))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn quantity_arithmetic_tool(
    req: QuantityArithmeticRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match quantity_arithmetic(
        &req.left,
        &req.right,
        &req.operation,
        req.result_unit.as_deref(),
    ) {
        Ok((value, unit)) => Ok(ok_json(json!({"value": value, "unit": unit}))),
        Err(e) => Ok(err_json(&e)),
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    // --- convert_unit ---

    #[test]
    fn test_convert_length() {
        let r = convert_unit(1.0, "km", "m").unwrap();
        assert!((r - 1000.0).abs() < 1e-10);
    }

    #[test]
    fn test_convert_mass() {
        let r = convert_unit(1.0, "kg", "lb").unwrap();
        assert!((r - 2.2046226218).abs() < 1e-6);
    }

    #[test]
    fn test_convert_temperature_c_to_f() {
        let r = convert_unit(0.0, "C", "F").unwrap();
        assert!((r - 32.0).abs() < 1e-10);
    }

    #[test]
    fn test_convert_temperature_f_to_c() {
        let r = convert_unit(212.0, "F", "C").unwrap();
        assert!((r - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_convert_temperature_k_to_c() {
        let r = convert_unit(273.15, "K", "C").unwrap();
        assert!((r - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_convert_temperature_absolute_zero() {
        assert!(convert_unit(-500.0, "C", "F").is_err());
    }

    #[test]
    fn test_convert_data_si_vs_iec() {
        let r = convert_unit(1.0, "KB", "B").unwrap();
        assert!((r - 1000.0).abs() < 1e-6);

        let r = convert_unit(1.0, "KiB", "B").unwrap();
        assert!((r - 1024.0).abs() < 1e-6);
    }

    #[test]
    fn test_convert_data_bit_to_byte() {
        let r = convert_unit(8.0, "bit", "B").unwrap();
        assert!((r - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_convert_duration() {
        let r = convert_unit(1.0, "h", "s").unwrap();
        assert!((r - 3600.0).abs() < 1e-10);

        let r = convert_unit(1.0, "d", "h").unwrap();
        assert!((r - 24.0).abs() < 1e-10);
    }

    #[test]
    fn test_convert_angle() {
        let r = convert_unit(180.0, "deg", "rad").unwrap();
        assert!((r - std::f64::consts::PI).abs() < 1e-10);
    }

    #[test]
    fn test_convert_cross_quantity_error() {
        assert!(convert_unit(1.0, "km", "kg").is_err());
    }

    #[test]
    fn test_convert_unknown_unit() {
        assert!(convert_unit(1.0, "foo", "m").is_err());
    }

    #[test]
    fn test_convert_month_points_to_date_tools() {
        let e = convert_unit(1.0, "month", "s").unwrap_err();
        assert!(e.contains("date_add") || e.contains("date_diff"));
    }

    // --- quantity_arithmetic ---

    #[test]
    fn test_quantity_arithmetic_add() {
        let left = Operand {
            value: 2.0,
            unit: "km".to_string(),
        };
        let right = Operand {
            value: 500.0,
            unit: "m".to_string(),
        };
        let (val, unit) = quantity_arithmetic(&left, &right, "add", Some("m")).unwrap();
        assert!((val - 2500.0).abs() < 1e-6);
        assert_eq!(unit, "m");
    }

    #[test]
    fn test_quantity_arithmetic_subtract() {
        let left = Operand {
            value: 1.0,
            unit: "kg".to_string(),
        };
        let right = Operand {
            value: 500.0,
            unit: "g".to_string(),
        };
        let (val, _unit) = quantity_arithmetic(&left, &right, "subtract", None).unwrap();
        assert!((val - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_quantity_arithmetic_default_unit() {
        let left = Operand {
            value: 2.0,
            unit: "km".to_string(),
        };
        let right = Operand {
            value: 500.0,
            unit: "m".to_string(),
        };
        let (val, unit) = quantity_arithmetic(&left, &right, "add", None).unwrap();
        assert!((val - 2.5).abs() < 1e-6);
        assert_eq!(unit, "km");
    }

    #[test]
    fn test_quantity_arithmetic_cross_quantity_error() {
        let left = Operand {
            value: 1.0,
            unit: "km".to_string(),
        };
        let right = Operand {
            value: 1.0,
            unit: "kg".to_string(),
        };
        assert!(quantity_arithmetic(&left, &right, "add", None).is_err());
    }

    #[test]
    fn test_quantity_arithmetic_temperature_rejected() {
        let left = Operand {
            value: 20.0,
            unit: "C".to_string(),
        };
        let right = Operand {
            value: 5.0,
            unit: "C".to_string(),
        };
        assert!(quantity_arithmetic(&left, &right, "add", None).is_err());
    }

    #[test]
    fn test_quantity_arithmetic_invalid_op() {
        let left = Operand {
            value: 1.0,
            unit: "m".to_string(),
        };
        let right = Operand {
            value: 1.0,
            unit: "m".to_string(),
        };
        assert!(quantity_arithmetic(&left, &right, "multiply", None).is_err());
    }
}
