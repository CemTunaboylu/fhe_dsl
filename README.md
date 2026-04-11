# Tiny FHE DSL — A Compiler Pipeline for Encrypted Arithmetic Circuits

![Rust Version](https://img.shields.io/badge/Rust-1.92.0-orange) ![MSRV](https://img.shields.io/badge/MSRV-1.92-orange)

A Rust DSL that compiles normal-looking arithmetic into optimized circuits suitable for Fully Homomorphic Encryption evaluation. Write arithmetic expressions using standard Rust operators; the DSL builds, optimizes, and lowers them into an IR that any backend can evaluate.

```rust
let ctx = new_folding_strict_context(11u64);

let a = ctx.input(0);
let b = ctx.input(1);

let output = &a * &b + &a;

let circuit = ctx.compile(output).expect("should compile");
```

At this point, `circuit` is an optimized DAG — not a computed value. A backend evaluates it against concrete inputs:

```rust
let mut backend = default_mock_fhe_backend::<u64>(noise_budget);

let result = backend
    .eval_outputs(&circuit, &[3, 7])
    .expect("should evaluate");
```

## Motivation

In FHE, every arithmetic operation is expensive and accumulates noise. Multiplicative depth is the primary bottleneck — once the noise budget is exhausted, decryption fails. This makes circuit optimization critical: fewer gates and shallower depth directly translate to cheaper and more feasible encrypted computation.

This project builds a compiler pipeline that performs those optimizations automatically before any ciphertext touches the circuit.

## Architecture

```
DSL expressions (operator overloading)
      ↓
Arithmetic DAG (hash-consed, SSA)
      ↓
Optimization passes (constant folding, CSE, reassociation)
      ↓
IR circuit (Gate-based DAG)
      ↓
Optimization passes (rebalancing, dead code elimination)
      ↓
Backend evaluation (pluggable)
```

The pipeline is split into independent crates:

| Crate | Role |
|-------|------|
| `dsl` | Expression building with operator overloading, constant folding, and CSE at construction time |
| `ir` | Gate-based circuit IR in SSA form |
| `passes` | Reassociation, tree rebalancing, liveness analysis, depth analysis |
| `backend` | Pluggable evaluation — plaintext mod-q and a mock FHE backend with noise tracking |
| `common` | The `Supported` trait and shared utilities |
| `op` | Binary operation definitions |

## Optimizations

**Constant folding** — Expressions with known values are evaluated at compile time. `ctx.constant(2) + ctx.constant(3)` produces a single `Const(5)` node, not an `Add` gate.

**Common subexpression elimination** — Via hash-consing. If the same expression is constructed twice, both references point to the same node. Commutative operands are canonicalized so `a + b` and `b + a` are recognized as identical.

**Live code inclusion (tree shaking)** — Compilation starts from output nodes and walks backward. Any node unreachable from an output is never lowered to IR.

**Reuse-driven reassociation** — A simplified variant of LLVM/GCC's reassociation pass adapted for arithmetic circuits. Given `x = a+b` already in the graph, the expression `a+c+b` is reassociated to `x+c`, eliminating a redundant gate. When reassociation brings two constants together, they are folded in the same pass. Details in the [wiki](../../wiki/Reuse_Driven_Reassociation).

**Tree-balancing reassociation** — Left-degenerate chains like `a+b+c+d+e+f+g+h` (depth 7) are rebalanced into a balanced binary tree (depth 3), directly reducing multiplicative depth. Details in the [wiki](../../wiki/Tree_Balancing_Reassociation).

## Multiple Outputs

```rust
let o1 = &a * &b;
let o2 = &a + &b;
let o3 = &a - &b;

let circuit = ctx.compile_many(&[o1, o2, o3]).unwrap();
```

Shared subexpressions across outputs are computed once.

## Compilation Modes

The DSL supports strictness modes that control whether unused inputs, constants, or operations are allowed:

```rust
// Strict: compilation fails if any declared input/constant/op is unused
let ctx = new_folding_strict_context(11u64);

// Loose: unused nodes are silently optimized away
let ctx = new_folding_loose_context(11u64);

// Fine-grained: strict on inputs and ops, loose on constants
let strictness: Strictness = (StrictnessOn::Input + StrictnessOn::Op).into();
```

## Backend Trait

Any type implementing `Backend<T>` can evaluate circuits:

```rust
pub trait Backend<T: Supported> {
    type Elem;

    fn add(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem;
    fn sub(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem;
    fn mul(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem;
    fn input(&mut self, c: T) -> Self::Elem;
    fn constant(&mut self, c: T) -> Self::Elem;
    // ...
}
```

Two backends are included:

- **`PlainModQBackend`** — Evaluates circuits in plaintext modular arithmetic. Useful for correctness testing.
- **`MockFHEBackend`** — Tracks multiplicative depth and noise accumulation per element. Noise grows multiplicatively on `mul` (approximating real lattice-based noise behavior) and additively on `add`/`sub`. Exceeding the noise budget returns an error, allowing circuit feasibility analysis without a real FHE library.

The `Noise` trait is pluggable — `MockNoise` provides budget tracking, `Noiseless` is a no-op for backends that don't need it.

## Design Decisions

- **`Supported` trait over a type alias.** Circuit elements are generic over `T: Supported`, bounded by `num-traits` (`PrimInt`, `WrappingAdd`, `WrappingMul`, etc.). This provides a clean migration path toward polynomial ring elements without coupling the IR to a specific integer width.

- **Plaintext modulus, not ciphertext modulus.** The `plain_text_modulus` field in circuits and backends corresponds to the plaintext space. In a full FHE stack, the ciphertext modulus `q` is a separate (much larger) parameter managed by the encryption layer. This DSL operates at the circuit/plaintext level by design.

- **Subtraction is not canonicalized.** `Sub` is kept as a first-class gate rather than lowering to `Add(lhs, Mul(rhs, -1))`. In FHE, negation is essentially free (negate polynomial coefficients), so canonicalizing would unnecessarily inflate multiplicative depth.

- **Noise model is approximate.** The mock backend uses `lhs.noise * rhs.noise + MUL_COST` for multiplication — directionally correct (noise compounds multiplicatively) but not a precise model of any specific FHE scheme. It is designed for relative comparison of circuit variants, not absolute noise prediction.

## Status

The compiler pipeline, all optimization passes, and both backends are functional and tested. Current work includes extending type support and exploring a real FHE backend integration.
