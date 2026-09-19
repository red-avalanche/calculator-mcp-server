# PLAN.md — New feature areas for the calculator MCP server

**Status:** FINALIZED (confirmed by user, 2026-08-13)
**Scope:** Add five new feature areas to the existing 23-tool server: unit conversion & quantity
arithmetic, date/time calculations (NO timezones), programmer/integer calculations, percentage &
financial calculations, and precision/rounding utilities.

**Explicitly out of scope:**
- Statistics — already implemented (`mean`, `variance`, `standard_deviation`, `median`, `mode`,
  `correlation_coefficient`, `linear_regression`, `confidence_interval`).
- Timezone conversion of any kind (no IANA tzdb, no fixed offsets). Rationale (user): timezone
  rules change too often to maintain. All dates are naive/civil.
- `factorize` — remains unported, see UPSTREAM.md.
- Version bump / release tagging — handled by the existing tag-driven release workflow.

---

## 1. Goal

Extend the calculator MCP server from 23 to 40 tools. Each new area is one new module under
`src/tools/`, following the established pattern: request structs deriving `serde::Deserialize` +
`schemars::JsonSchema`, pure functions returning `Result<T, String>`, thin `#[tool(name=…)]`
handlers in `main.rs`, in-module `#[cfg(test)]` tests, `ok_json`/`err_json` for responses.

## 2. Confirmed decisions (from elicitation)

| # | Decision |
|---|---|
| D1 | New well-established crates allowed; must be pure-Rust (musl static release builds). Add exactly two: `measurements = "0.11"` and `chrono = { version = "0.4", default-features = false }`. Everything else is std-only/hand-rolled. |
| D2 | Statistics untouched. |
| D3 | No timezone support anywhere. |
| D4 | Rounding is f64-based; the binary-float caveat (2.675 → 2.67 at 2dp) is documented in the tool description. No decimal crate. |
| D5 | `quantity_arithmetic` REJECTS temperature operands with a clear error (ambiguous delta-vs-absolute semantics). |
| D6 | Date month/year semantics: only WHOLE calendar months/years count, using clamped anniversaries. Details and examples in §4.2. |
| D7 | IRR returns the root nearest the caller's guess; multiple-root caveat documented in the tool description. |
| D8 | Tool descriptions must be LLM-self-sufficient: each tool's doc comment lists its supported values (modes, unit categories/symbols, bases, bit widths). Unknown-unit errors return the valid candidates so the LLM can recover. |
| D9 | One tool per operation (existing codebase style); 17 new tools total. |

---

## 3. New dependencies (Stage 1)

```toml
measurements = "0.11"                                   # units; pure-Rust (dep: libm only)
chrono = { version = "0.4", default-features = false }  # NaiveDate/NaiveDateTime; no clock, no tz
```

Verified: `measurements` 0.11.1 covers length, mass, temperature (+`TemperatureDelta`), volume,
speed, area, pressure, energy, power, force, angle, and `Data` with BOTH SI (kilooctets = 1000 B)
and IEC (kibioctets = 1024 B) units; it implements `Add`/`Sub`/`Mul<f64>` (quantity arithmetic);
its only dependency is pure-Rust `libm`. It has NO time-duration quantity — durations are
hand-rolled (§4.1). `chrono` ≥ 0.4.20 is pure-Rust; with `default-features = false` it provides
`NaiveDate`/`NaiveDateTime` parsing, `checked_add_months`/`checked_add_years` (clamped),
`signed_duration_since`, `weekday`, `iso_week`, `ordinal`, `leap_year` — everything needed.

---

## 4. Functional requirements by module

### 4.1 `src/tools/units.rs` — 2 tools

**`convert_unit`** — `{value: f64, from_unit: String, to_unit: String}` → `{"result": f64}`

MUST support these quantities and units (aliases resolve case-insensitively where sensible;
one internal alias table maps `unit string → (quantity, constructor)`):

- **length:** m, km, cm, mm, mi, yd, ft, in, nmi
- **mass:** kg, g, mg, t, lb, oz, st
- **temperature:** C, F, K (aliases: celsius, fahrenheit, kelvin). Affine offsets via
  `measurements::Temperature`. Result below absolute zero (0 K) → error.
- **volume:** l, ml, m3, gal, qt, pt, cup, floz
- **speed:** m/s, km/h, mph, kn
- **area:** m2, km2, ha, acre, ft2
- **duration (hand-rolled, fixed ratios; `measurements` lacks this):** ms, s, min, h, d, wk.
  Months/years intentionally EXCLUDED (not fixed durations — use the date tools); attempting
  them errors with a message pointing at `date_add`/`date_diff`.
- **data size:** bit, b/B; SI: KB, MB, GB, TB (powers of 1000); IEC: KiB, MiB, GiB, TiB
  (powers of 1024). The KB-vs-KiB distinction MUST be exact; never collapse them.
  (`measurements` calls bytes "octets": `from_kilooctets`/`from_kibioctets` etc. — the alias
  table hides this.)
- **pressure:** pa, kpa, bar, atm, psi, mmhg
- **energy:** j, kj, cal, kcal, wh, kwh, btu
- **power:** w, kw, mw, hp
- **force:** n, kn, lbf
- **angle:** deg, rad, grad

Rules:
- `from_unit` and `to_unit` must resolve to the SAME quantity, else error naming the two
  different quantities detected.
- Unknown unit string → error listing the valid unit symbols (full list, or the inferred
  quantity's list when unambiguous).
- Tool description lists the supported categories so the LLM can clarify with the user.

**`quantity_arithmetic`** — `{left: {value: f64, unit: String}, right: {value: f64, unit: String},
operation: "add"|"subtract", result_unit?: String}` → `{"value": f64, "unit": String}`

- Both operands must resolve to the same quantity → else error (e.g. "cannot add kilometers and
  kilograms").
- Temperature operands → REJECTED with a clear error (D5).
- Compute via conversion to base units; return in `result_unit` (default: left's unit).
- Example: `{2, "km"} + {500, "m"}` → `{"value": 2500, "unit": "m"}` when result_unit = "m".

### 4.2 `src/tools/datetime.rs` — 3 tools (chrono, naive dates only, NO timezones)

**Date/month/year anniversary convention (D6) — applies everywhere below:**
- Months and years count as WHOLE calendar months/years only.
- `add_months_clamped(date, n)` = chrono `checked_add_months` semantics: the day clamps to the
  last day of the target month (Jan 31 + 1 month → Feb 28/29).
- `add_years_clamped(date, n)`: Feb 29 + 1 year → Feb 28 of the target (non-leap) year.
- Complete months elapsed from S to E = the largest n with `add_months_clamped(S, n) ≤ E`;
  complete years likewise. Remainder is expressed in days.
- Worked examples (MUST be unit tests):
  - Jan 13 → Feb 28: 1 complete month (anniversary Feb 13 passed), remainder 15 days.
  - Jan 31 → Feb 27: 0 complete months (anniversary Feb 28 not yet reached).
  - Jan 31 → Feb 28: 1 complete month.
  - 2024-02-29 + 1 year → 2025-02-28.
- If the user wants finer granularity, they use days/hours units — document this in the
  `date_diff` tool description.

Input parsing: accept `YYYY-MM-DD`, `YYYY-MM-DD HH:MM:SS`, and `YYYY-MM-DDTHH:MM:SS`
(naive; any timezone suffix → error). Date-only inputs are treated as midnight. Unparseable
input → error listing the accepted formats. Date-only input produces date-only output
unless time units were involved.

**`date_add`** — `{date: String, years?: i64, months?: i64, weeks?: i64, days?: i64,
hours?: i64, minutes?: i64, seconds?: i64}` → `{"result": String}`
- Negative values subtract. Apply years, then months (both clamped), then the rest as exact time.
- Overflow / out-of-range date → tool error, never panic (use `checked_*`).

**`date_diff`** — `{start: String, end: String,
unit?: "auto"|"years"|"months"|"weeks"|"days"|"hours"|"minutes"|"seconds"}` (default auto)
- `years`/`months`: complete clamped anniversaries only (see convention above); signed.
- `weeks`/`days`/`hours`/`minutes`/`seconds`: signed exact duration division (weeks truncate
  toward zero).
- `auto` → `{"years": y, "months": m, "days": d, "total_days": t, "sign": 1|-1}` where the
  breakdown is the magnitude decomposition (years → months → days, greedily, clamped) and
  `sign` is −1 when end < start; datetimes also include `total_seconds`.

**`date_info`** — `{date: String}` →
`{"weekday": "Monday"…, "iso_week": u32, "day_of_year": u32, "days_in_month": u32,
"is_leap_year": bool, "days_remaining_in_year": u32}`

### 4.3 `src/tools/programmer.rs` — 2 tools (std-only; i64/u64 domain)

All integer inputs/outputs that can exceed 2^53 MUST be passed/returned as JSON strings (plain
JSON integers ≤ 2^53 also accepted via an untagged enum). Document the i64 value range.

**`base_convert`** — `{value: String, to_base: 2|8|10|16, from_base?: 2|8|10|16}` →
`{"decimal": "…", "hex": "0x…", "octal": "0o…", "binary": "0b…", "result": "…"}`
- `from_base` optional: prefixes `0x`/`0o`/`0b` are auto-detected; bare input defaults to 10.
- Negative input allowed; output uses sign-magnitude form (`-ff`-style, e.g. `-0xFF`), NOT
  two's complement — two's complement lives in `bitwise`.
- Always return all four representations plus `result` (value in `to_base`, prefixed
  `0x`/`0o`/`0b` for non-10 bases); invalid digit for base → error.

**`bitwise`** — `{operation: "and"|"or"|"xor"|"not"|"shl"|"shr", a, b?,
bit_width?: 8|16|32|64, signed?: bool}` (defaults: width 64, unsigned) →
`{"value": "…", "unsigned_value": "…", "hex": "…", "octal": "…", "binary": "…"}`

- Values are masked to `bit_width`. `value` respects `signed` (two's complement: 0xFF @ i8 → −1);
  `unsigned_value` is the raw masked integer; hex/octal/binary show the full padded bit pattern
  (`0xFF`, `0o377`, `0b11111111` for width 8).
- `and`/`or`/`xor` require `b`. `not` forbids `b`.
- `shl`/`shr`: `b` is the shift count, must satisfy `0 ≤ b < bit_width` else error.
  `shr` is arithmetic when `signed`, logical when unsigned. `shl` drops shifted-out bits
  (wrapping at width) — documented.
- Operands outside the width range → error; inside-range is checked AFTER two's-complement
  interpretation when `signed`.
- Tool description includes one worked example (0xFF as signed 8-bit = −1).

### 4.4 `src/tools/finance.rs` — 6 tools (std-only, f64)

**`percentage`** — `{mode, a: f64, b: f64}` → `{"result": f64}`
- `percent_of`: b% of a → `a * b/100`
- `what_percent`: a is what % of b → `a/b * 100` (b ≠ 0)
- `percent_change`: from a to b → `(b−a)/a * 100` (a ≠ 0)
- `increase`: a increased by b% → `a * (1 + b/100)`
- `decrease`: a decreased by b% → `a * (1 − b/100)`
- Tool description lists all five modes with one-line formulas.

**`compound_interest`** — `{principal: f64, annual_rate_pct: f64, years: f64,
compounds_per_year?: f64}` (default 12; **0 = continuous**) →
`{"future_value", "total_interest", "effective_annual_rate_pct"}`
- Discrete: `FV = P(1 + r/n)^(nt)`; continuous (n=0): `FV = P·e^(rt)`.
- EAR: `(1 + r/n)^n − 1`, or `e^r − 1` when continuous.

**`loan_payment`** — `{principal: f64, annual_rate_pct: f64, years: f64,
payments_per_year?: f64}` (default 12) →
`{"payment", "num_payments", "total_paid", "total_interest"}`
- `r = APR/(100·ppy)`, `n = years·ppy`; `payment = P·r/(1 − (1+r)^(−n))`; 0% rate → `P/n`.

**`net_present_value`** — `{rate_pct: f64, cashflows: Vec<f64>}` → `{"npv": f64}`
- `NPV = Σ cashflows[t] / (1+r)^t` with t = array index (first cash flow at t=0).
  Description MUST call out the difference from Excel's NPV (which discounts the first cash
  flow one period).

**`internal_rate_of_return`** — `{cashflows: Vec<f64>, guess_pct?: f64}` (default 10) →
`{"irr_pct": f64}`
- Newton–Raphson on f(r) with analytic derivative, from the guess; fall back to bisection on
  `(−1, 10)` when Newton diverges.
- Cashflows with no sign change → error ("IRR undefined"). Non-convergence → tool error.
- Multiple-root caveat documented in the description (D7).

**`return_on_investment`** — `{gain: f64, cost: f64}` →
`{"roi_pct": (gain−cost)/cost*100, "net_gain": gain−cost}`; cost = 0 → error.

### 4.5 `src/tools/precision.rs` — 4 tools (std-only, f64; D4)

**`round_number`** — `{value: f64, decimals: i32, mode?: "half_up"|"half_even"|"floor"|"ceil"|
"trunc"}` (default half_up) → `{"result": f64}`
- `decimals` may be negative (round to tens/hundreds). `half_up` = half away from zero (Rust's
  `f64::round` semantics); `half_even` = banker's rounding (hand-rolled);
  `floor`/`ceil`/`trunc` via the eponymous f64 methods after scaling by `10^decimals`.
- The f64 binary-representation caveat (2.675 → 2.67 at 2 dp) MUST appear in the tool description.

**`significant_figures`** — `{value: f64, sig_figs: i64}` (1..=17) → `{"result": String}`
- String output to preserve trailing zeros (`2.500`, not `2.5`).
- `decimals = sig_figs − 1 − floor(log10(|v|))`, round half-up, format with fixed decimals.
  `0` → `"0"`. Negative values round by magnitude.

**`format_notation`** — `{value: f64, style: "scientific"|"engineering", sig_figs?: i64}` →
`{"result": String}`
- Scientific: `d.dddde±X` mantissa in [1, 10). Engineering: exponent forced to a multiple of 3,
  mantissa in [1, 1000). `sig_figs` (when given) controls mantissa digits; string output.

**`to_fraction`** — `{value: f64, max_denominator?: i64}` →
`{"numerator": i64, "denominator": i64, "mixed": String, "exact": bool}`
- Continued fractions. Without `max_denominator`: exact CF until f64 precision exhausts
  (`exact: true` when it terminates exactly). With `max_denominator`: best approximation with
  denominator ≤ max (e.g. 0.333 @ max ≥ 3 → 1/3, `exact: false`).
- Negative → negative numerator; `mixed` for |v| ≥ 1 formatted like `"-1 1/3"`.
- Non-finite or denominator-overflow → error.

### 4.6 Integration (all modules)

- Every tool registered in `main.rs` via `#[tool(name = "…")]` with a doc comment that becomes
  the LLM-facing description; doc comments must satisfy D8 (self-sufficient value lists).
- `src/tools/mod.rs` gains: `pub mod units; pub mod datetime; pub mod programmer;
  pub mod finance; pub mod precision;`
- Server `with_instructions` updated to e.g. "Mathematical calculator: symbolic math,
  statistics, matrices, units, dates, programmer/integer, finance, precision."
- Errors: computation errors → `err_json`; never write to stdout; no panics on user input
  (use `checked_*` / `Result` everywhere). Matches existing conventions.
- Each module gets `#[cfg(test)]` tests covering the happy path plus every edge case listed
  above (especially the §4.2 date-convention examples, boundary rounding, KB-vs-KiB, and
  two's-complement cases).
- `scripts/smoke_test.sh` gains ≥ 2 calls to new tools (suggested: `convert_unit` mi→km,
  `base_convert` 255→hex).
- README Features section updated with the five new areas.

---

## 5. Implementation stages

Stage order: 1 first; 2–6 mutually independent (parallelizable); 7 last.

**Stage 1 — Dependencies `[READY]`**
- Tasks: add the two deps from §3 to Cargo.toml; `cargo build` clean; confirm no transitive
  C dependencies (musl safety).
- Deliverables: buildable tree. Dependencies: none.

**Stage 2 — Precision `[READY]`** (4 tools: `round_number`, `significant_figures`,
`format_notation`, `to_fraction` per §4.5)
- Deliverables: `src/tools/precision.rs`, mod.rs + main.rs registration, module tests.
- Dependencies: none beyond Stage 1.

**Stage 3 — Programmer `[READY]`** (2 tools: `base_convert`, `bitwise` per §4.3)
- Deliverables: `src/tools/programmer.rs`, registration, tests.
- Dependencies: Stage 1.

**Stage 4 — Units `[READY]`** (2 tools: `convert_unit`, `quantity_arithmetic` per §4.1, incl.
the unit alias table and the hand-rolled duration quantity)
- Deliverables: `src/tools/units.rs`, registration, tests.
- Dependencies: Stage 1.

**Stage 5 — Datetime `[READY]`** (3 tools: `date_add`, `date_diff`, `date_info` per §4.2)
- Deliverables: `src/tools/datetime.rs`, registration, tests (must include all D6 examples).
- Dependencies: Stage 1.

**Stage 6 — Finance `[READY]`** (6 tools: `percentage`, `compound_interest`, `loan_payment`,
`net_present_value`, `internal_rate_of_return`, `return_on_investment` per §4.4)
- Deliverables: `src/tools/finance.rs`, registration, tests (incl. known PMT/NPV reference
  values and 0% loan case).
- Dependencies: Stage 1.

**Stage 7 — Polish & gates `[READY]`**
- Tasks: update `with_instructions`, smoke test, README; run the full gate and make it green:
  `cargo fmt --check && cargo clippy -- -D warnings && cargo test && bash scripts/smoke_test.sh`
  plus a musl release check: `cargo build --target x86_64-unknown-linux-musl --release`.
- Dependencies: Stages 2–6.

---

## 6. Tool inventory after completion (40)

| Module | Tools |
|---|---|
| (root) | version, calculate |
| symbolic (existing) | differentiate, integrate, solve_equation, expand, summation, plot_function |
| stats (existing) | mean, variance, standard_deviation, median, mode, correlation_coefficient, linear_regression, confidence_interval |
| linalg (existing) | matrix_addition, matrix_multiplication, matrix_transpose, matrix_determinant, vector_dot_product, vector_cross_product, vector_magnitude |
| **units (new)** | convert_unit, quantity_arithmetic |
| **datetime (new)** | date_add, date_diff, date_info |
| **programmer (new)** | base_convert, bitwise |
| **finance (new)** | percentage, compound_interest, loan_payment, net_present_value, internal_rate_of_return, return_on_investment |
| **precision (new)** | round_number, significant_figures, format_notation, to_fraction |

## 7. Known limitations / risks (accepted)

- Rounding operates on f64 (binary float artifacts for values like 2.675); documented in tool
  descriptions (D4).
- `quantity_arithmetic` has no temperature support (D5); delta-temperature arithmetic is a
  possible future extension via `TemperatureDelta`.
- `date_diff` month/year answers follow the whole-anniversary convention (D6) and will not
  match fractional-month expectations; documented.
- IRR returns one root (nearest the guess) even when several exist (D7); documented.
- Duration quantity excludes months/years by design (use date tools); conversion attempts
  error with a pointer to the right tool.

---
---

# Appendix: Historical — original Rust-rewrite plan (preserved verbatim)

> The section below is the pre-existing PLAN.md for the Python→Rust rewrite, kept for
> reference. The plan above supersedes it as the active document.

# PLAN.md — Rust rewrite of calculator-mcp-server

**Purpose:** Convert the Python FastMCP calculator server (`calculator_server.py`) into a single-binary
Rust MCP server. See the full plan in the original repository. This file was restored after a
destructive git operation wiped uncommitted work.

## Key decisions already made (from completed spike):

### CAS crate selection (spike completed):
- `differentiate` → `symb_anafis` 0.8.1 (string API, `^` for powers, clean output)
- `integrate` → `thales` 0.4.3 (parse_expression + integrate + simplify, `^` for powers)
- `solve_equation` → `mathcore` 0.3.1 (MathCore::solve, `^` for powers)
- `expand` → `mathhook-core` 0.2.0 (Parser + Expand trait, `^` for powers, ugly Display needs cleanup)
- `factorize` → SKIP (no crate passes; documented in UPSTREAM.md)
- `summation` → direct (fasteval eval_with_x at each integer)
- `plot_function` → direct (fasteval + plotters, NO fonts/text — causes panic)

### Cargo.toml deps:
rmcp 3.0, tokio 1, serde 1, serde_json 1, schemars 1, fasteval 0.2, statrs 0.19,
nalgebra 0.35, plotters 0.3 (bitmap_backend + line_series, NO default features),
anyhow 1, tracing 0.1, tracing-subscriber 0.3, symb_anafis 0.8, thales 0.4,
mathcore 0.3, mathhook-core 0.2, base64 0.22, image 0.25

### Key implementation notes:
- `log(` must be renamed to `_pylog(` in preprocessing (fasteval builtin shadows callback)
- `**` → `^` preprocessing for all CAS tools
- `eval_with_x` binds variable x in the fasteval callback for summation/plot
- plot_function must NOT use any text/caption/labels (plotters panics without fonts)
- plot_function must be wrapped in catch_unwind (panics kill tokio workers)
- All CAS calls wrapped in catch_unwind
- Error handling: Ok(err_json) not Err(ErrorData) for tool computation errors
