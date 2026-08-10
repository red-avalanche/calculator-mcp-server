# UPSTREAM.md — Unported tools

## factorize
No tested Rust CAS crate correctly factorizes expressions.
- mathcore 0.3.1: returns expanded form, not factored
- mathhook-core 0.2.0: returns expanded form, not factored
- thales 0.4.3: no factor API
- symb_anafis 0.8.1: differentiation only

### Failing inputs
- `x^2 + 2*x + 1` → expected `(x + 1)^2`, got `x^2 + 2*x + 1` (expanded)
- `x^2 - 1` → expected `(x - 1)*(x + 1)`, got `x^2 - 1` (expanded)

### Suggested fix
Implement polynomial factorization in mathcore or mathhook-core upstream.