// Matrix and vector operations — addition, multiplication, transpose,
// determinant, dot product, cross product, magnitude.
//
// Uses `nalgebra` 0.35, a pure-Rust linear algebra library (no BLAS/LAPACK
// dependencies, which makes it suitable for musl static linking).
//
// All operations validate dimensions and return error JSON instead of
// panicking. Integral results are serialized as integers (e.g. [[6,8]]
// not [[6.0,8.0]]) to match the Python server's output format.

// `DMatrix<f64>` is a dynamic-size matrix (rows and cols determined at
// runtime, not compile time). `DVector<f64>` is a dynamic-size column
// vector. `Vector3<f64>` is a fixed 3D vector (compile-time dimension).
use nalgebra::{DMatrix, DVector, Vector3};
use rmcp::model::CallToolResult;

use crate::tools::{err_json, ok_json};

// ===== Serialization helpers =====
// These convert Vec<Vec<f64>> / Vec<f64> into serde_json::Value, applying
// the int/float rule: if a value's fractional part is 0 and it fits in i64,
// serialize as integer (not float). This matches the Python server's
// behavior where numpy returns ints for integer-valued results.

/// Serializes a matrix (Vec<Vec<f64>>) as JSON, converting integral values
/// to integers. Uses nested `.map()` and `.collect()` to transform the
/// 2D structure element-by-element.
fn serialize_matrix(m: Vec<Vec<f64>>) -> serde_json::Value {
    // `serde_json::json!(...)` wraps the inner expression in a Value.
    // The inner expression builds a Vec<Vec<Value>> via two layers of
    // `.map().collect()`. Each element is either `json!(v as i64)` or
    // `json!(v)` depending on whether it's a whole number.
    serde_json::json!(m
        .iter()
        .map(|row| {
            row.iter()
                .map(|&v| {
                    if v.fract() == 0.0 && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                        serde_json::json!(v as i64)
                    } else {
                        serde_json::json!(v)
                    }
                })
                // `.collect::<Vec<_>>()` gathers the iterator into a Vec.
                // The `_` lets the compiler infer the element type.
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>())
}

/// Serializes a vector (Vec<f64>) as JSON with the same int/float rule.
fn serialize_vector(v: Vec<f64>) -> serde_json::Value {
    serde_json::json!(v
        .iter()
        .map(|&v| {
            if v.fract() == 0.0 && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                serde_json::json!(v as i64)
            } else {
                serde_json::json!(v)
            }
        })
        .collect::<Vec<_>>())
}

// ===== Request structs =====

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MatrixAdditionRequest {
    /// The first matrix as a list of rows (list of lists of floats).
    pub matrix_a: Vec<Vec<f64>>,
    /// The second matrix.
    pub matrix_b: Vec<Vec<f64>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MatrixMultiplicationRequest {
    /// The first matrix.
    pub matrix_a: Vec<Vec<f64>>,
    /// The second matrix.
    pub matrix_b: Vec<Vec<f64>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MatrixTransposeRequest {
    /// The matrix to transpose.
    pub matrix: Vec<Vec<f64>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MatrixDeterminantRequest {
    /// The matrix (must be square).
    pub matrix: Vec<Vec<f64>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct VectorDotProductRequest {
    /// The first vector.
    pub vector_a: Vec<f64>,
    /// The second vector.
    pub vector_b: Vec<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct VectorCrossProductRequest {
    /// The first vector (must be 3D).
    pub vector_a: Vec<f64>,
    /// The second vector (must be 3D).
    pub vector_b: Vec<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct VectorMagnitudeRequest {
    /// The vector.
    pub vector: Vec<f64>,
}

// ===== Conversion helpers =====
// These convert between the JSON wire format (Vec<Vec<f64>>) and nalgebra's
// internal matrix type (DMatrix<f64>).

/// Converts Vec<Vec<f64>> → DMatrix<f64>. Validates that the matrix is
/// non-empty and rectangular (all rows have the same length).
///
/// `&[Vec<f64>]` is a borrowed slice of owned Vecs — we don't take ownership.
fn to_dmatrix(m: &[Vec<f64>]) -> Result<DMatrix<f64>, String> {
    if m.is_empty() || m[0].is_empty() {
        return Err("Matrix must be non-empty".to_string());
    }
    let cols = m[0].len();
    // `.all()` returns true if the closure returns true for all elements.
    // `|row| row.len() == cols` checks that every row has the same length.
    if !m.iter().all(|row| row.len() == cols) {
        return Err("Matrix must be rectangular".to_string());
    }
    // Flatten the 2D Vec into a 1D Vec for nalgebra.
    // `.flatten()` turns [[1,2],[3,4]] into [1,2,3,4].
    // `.copied()` copies the f64 values out of the references.
    let flat: Vec<f64> = m.iter().flatten().copied().collect();
    // `from_row_slice(rows, cols, &flat)` creates a DMatrix from a flat
    // slice in ROW-MAJOR order (row by row). This matches our Vec<Vec<f64>>
    // wire format. NOTE: `DMatrix::from_vec` uses COLUMN-major order —
    // do NOT use it, it would produce wrong results.
    Ok(DMatrix::from_row_slice(m.len(), cols, &flat))
}

/// Converts DMatrix<f64> → Vec<Vec<f64>> for JSON serialization.
///
/// `(0..dm.nrows())` is a range iterator over row indices.
/// `.map(|i| ...)` transforms each row index into a Vec<f64>.
/// `dm.row(i)` returns a reference to row i. `.iter()` iterates over
/// elements. `.copied()` copies f64 values. `.collect()` gathers into Vec.
fn from_dmatrix(dm: &DMatrix<f64>) -> Vec<Vec<f64>> {
    (0..dm.nrows())
        .map(|i| dm.row(i).iter().copied().collect())
        .collect()
}

// ===== Pure functions =====

/// Adds two matrices element-wise. Both must have the same dimensions.
pub fn matrix_addition(a: &[Vec<f64>], b: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, String> {
    let dm_a = to_dmatrix(a)?;
    let dm_b = to_dmatrix(b)?;

    if dm_a.nrows() != dm_b.nrows() || dm_a.ncols() != dm_b.ncols() {
        return Err("Matrix dimensions must match for addition".to_string());
    }

    // nalgebra supports operator overloading: `dm_a + dm_b` performs
    // matrix addition via the `Add` trait. This is Rust's equivalent of
    // Python's `__add__`.
    let result = dm_a + dm_b;
    Ok(from_dmatrix(&result))
}

/// Multiplies two matrices (matrix product, not element-wise).
/// The number of columns in `a` must equal the number of rows in `b`.
pub fn matrix_multiplication(a: &[Vec<f64>], b: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, String> {
    let dm_a = to_dmatrix(a)?;
    let dm_b = to_dmatrix(b)?;

    if dm_a.ncols() != dm_b.nrows() {
        return Err("Matrix dimensions must be compatible for multiplication".to_string());
    }

    // `dm_a * dm_b` performs matrix multiplication via the `Mul` trait.
    let result = dm_a * dm_b;
    Ok(from_dmatrix(&result))
}

/// Transposes a matrix (swaps rows and columns).
pub fn matrix_transpose(m: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, String> {
    let dm = to_dmatrix(m)?;
    // `.transpose()` returns a new transposed matrix (doesn't mutate original).
    let result = dm.transpose();
    Ok(from_dmatrix(&result))
}

/// Computes the determinant of a square matrix. Result is rounded to 10
/// decimal places (matching the Python server's `round(float(result), 10)`).
pub fn matrix_determinant(m: &[Vec<f64>]) -> Result<f64, String> {
    let dm = to_dmatrix(m)?;

    // nalgebra's `.determinant()` PANICS if the matrix is not square.
    // We validate first to return an error instead of panicking.
    if dm.nrows() != dm.ncols() {
        return Err("Matrix must be square to compute determinant".to_string());
    }

    let det = dm.determinant();
    // Round to 10 decimal places: multiply by 1e10, round to nearest integer,
    // divide by 1e10. This matches Python's `round(float(result), 10)`.
    let det10 = (det * 1e10).round() / 1e10;
    Ok(det10)
}

/// Computes the dot product of two vectors. Both must have the same length.
pub fn vector_dot_product(a: &[f64], b: &[f64]) -> Result<f64, String> {
    if a.len() != b.len() {
        return Err("Vector dimensions must match for dot product".to_string());
    }

    // `DVector::from_vec` creates a dynamic-size column vector from a Vec.
    // `.to_vec()` clones the slice into an owned Vec (from_vec takes ownership).
    let v_a = DVector::from_vec(a.to_vec());
    let v_b = DVector::from_vec(b.to_vec());

    // `.dot(&v_b)` computes the dot product. The `&` borrows v_b.
    let result = v_a.dot(&v_b);
    Ok(result)
}

/// Computes the cross product of two 3D vectors. Returns an error if
/// either vector is not exactly 3-dimensional.
pub fn vector_cross_product(a: &[f64], b: &[f64]) -> Result<Vec<f64>, String> {
    if a.len() != 3 || b.len() != 3 {
        return Err("Cross product is only defined for 3-dimensional vectors".to_string());
    }

    // `Vector3::from_column_slice(a)` creates a fixed-size 3D vector from
    // a slice. The dimension is enforced at COMPILE TIME — if we tried to
    // create a Vector3 from a 4-element slice, it wouldn't compile.
    // We validate at runtime above because the input is dynamic-length.
    let va = Vector3::from_column_slice(a);
    let vb = Vector3::from_column_slice(b);

    // `.cross(&vb)` computes the cross product. Returns a Vector3.
    let result = va.cross(&vb);

    // Convert back to Vec<f64> by indexing. `result[0]`, `result[1]`,
    // `result[2]` are safe because Vector3 always has exactly 3 elements.
    Ok(vec![result[0], result[1], result[2]])
}

/// Computes the magnitude (Euclidean norm) of a vector.
/// For a vector [x₁, x₂, ..., xₙ], returns √(x₁² + x₂² + ... + xₙ²).
pub fn vector_magnitude(v: &[f64]) -> Result<f64, String> {
    let dv = DVector::from_vec(v.to_vec());
    // `.norm()` computes the L2 norm (Euclidean magnitude).
    // This is equivalent to numpy's np.linalg.norm for vectors.
    let result = dv.norm();
    Ok(result)
}

// ===== Tool wrapper functions =====

pub fn matrix_addition_tool(req: MatrixAdditionRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match matrix_addition(&req.matrix_a, &req.matrix_b) {
        Ok(result) => Ok(ok_json(serialize_matrix(result))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn matrix_multiplication_tool(
    req: MatrixMultiplicationRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match matrix_multiplication(&req.matrix_a, &req.matrix_b) {
        Ok(result) => Ok(ok_json(serialize_matrix(result))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn matrix_transpose_tool(
    req: MatrixTransposeRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match matrix_transpose(&req.matrix) {
        Ok(result) => Ok(ok_json(serialize_matrix(result))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn matrix_determinant_tool(
    req: MatrixDeterminantRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match matrix_determinant(&req.matrix) {
        Ok(result) => Ok(ok_json(serde_json::json!({ "result": result }))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn vector_dot_product_tool(
    req: VectorDotProductRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match vector_dot_product(&req.vector_a, &req.vector_b) {
        Ok(result) => {
            // Serialize as integer if the result is a whole number (e.g.
            // dot([1,2],[7,8]) = 23, not 23.0). This matches the Python server.
            if result.fract() == 0.0 && result >= i64::MIN as f64 && result <= i64::MAX as f64 {
                Ok(ok_json(serde_json::json!({ "result": result as i64 })))
            } else {
                Ok(ok_json(serde_json::json!({ "result": result })))
            }
        }
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn vector_cross_product_tool(
    req: VectorCrossProductRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match vector_cross_product(&req.vector_a, &req.vector_b) {
        Ok(result) => Ok(ok_json(serialize_vector(result))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn vector_magnitude_tool(
    req: VectorMagnitudeRequest,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match vector_magnitude(&req.vector) {
        Ok(result) => Ok(ok_json(serde_json::json!({ "result": result }))),
        Err(e) => Ok(err_json(&e)),
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix_addition() {
        // `vec![...]` is a macro that creates a Vec from a list of values.
        // `vec![vec![1.0, 2.0], vec![3.0, 4.0]]` creates a 2x2 matrix.
        let a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];
        let result = matrix_addition(&a, &b).unwrap();
        assert_eq!(result, vec![vec![6.0, 8.0], vec![10.0, 12.0]]);
    }

    #[test]
    fn test_matrix_multiplication() {
        let a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];
        let result = matrix_multiplication(&a, &b).unwrap();
        assert_eq!(result, vec![vec![19.0, 22.0], vec![43.0, 50.0]]);
    }

    #[test]
    fn test_matrix_transpose() {
        let m = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let result = matrix_transpose(&m).unwrap();
        assert_eq!(result, vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
    }

    #[test]
    fn test_matrix_determinant() {
        let m = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        // det([[1,2],[3,4]]) = 1*4 - 2*3 = -2
        assert_eq!(matrix_determinant(&m).unwrap(), -2.0);
    }

    #[test]
    fn test_vector_dot_product() {
        // dot([1,2],[7,8]) = 1*7 + 2*8 = 23
        assert_eq!(vector_dot_product(&[1.0, 2.0], &[7.0, 8.0]).unwrap(), 23.0);
    }

    #[test]
    fn test_vector_cross_product() {
        // cross([1,2,3],[4,5,6]) = [-3, 6, -3]
        assert_eq!(
            vector_cross_product(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]).unwrap(),
            vec![-3.0, 6.0, -3.0]
        );
        // Non-3D vectors should return an error, not panic
        assert!(vector_cross_product(&[1.0, 2.0], &[3.0, 4.0]).is_err());
    }

    #[test]
    fn test_vector_magnitude() {
        // ||[1,2,3]|| = sqrt(1+4+9) = sqrt(14) ≈ 3.7416...
        assert!((vector_magnitude(&[1.0, 2.0, 3.0]).unwrap() - 3.7416573867739413).abs() < 1e-10);
    }

    #[test]
    fn test_matrix_errors() {
        // Dimension mismatch for addition
        assert!(matrix_addition(&[vec![1.0]], &[vec![1.0, 2.0]]).is_err());
        // Non-square matrix for determinant
        assert!(matrix_determinant(&[vec![1.0, 2.0]]).is_err());
        // Length mismatch for dot product
        assert!(vector_dot_product(&[1.0], &[1.0, 2.0]).is_err());
    }
}
