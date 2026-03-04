# Tiny FHE DSL (Tiny Fully Homomorphic Encryption Circuit Builder)

![Rust Version](https://img.shields.io/badge/Rust-1.92.0-orange) ![MSRV](https://img.shields.io/badge/MSRV-1.92-orange)

This repository is an experiment in building Fully Homomorphic Encryption circuits from normal-looking Rust arithmetic.

The idea is simple, you write some arithmetic like:

```rust
let output = input_1 * input_2 + const_1 * const_2;
```

And instead of directly computing it, we build a circuit in the background representing the computation. Later, a backend can evaluate the circuit against arbitrary inputs. In simple terms it is like a tiny compiler for encrypted arithmetic. Your Rust arithmetic becomes a compiler pipeline for encrypted math.

## Why This Exists

Simply, Fully Homomorphic Encryption (FHE) allows computation on encrypted data. But there’s a catch: every arithmetic operation is very expensive.

So before executing anything, we want to:

- reduce the number of operations
- simplify expressions
- eliminate duplicates
- rebalance trees

This project experiments with building a tiny compiler pipeline that performs those optimizations automatically.

The goals:

- learning/exploring more compiler techniques
- experiment with FHE-friendly circuits
- keep the DSL ergonomic

And because writing compilers for encrypted math is fun.

It is an active experiment/learning project.

Things will break.
Probably often.

## What does it do?

The DSL overloads arithmetic operators (+ - *, fully homomorphic operations) and builds an arithmetic circuit. While doing that, it quietly performs some optimizations so the circuit doesn’t explode in size. It currently only supports `u64`, but `float` is on the way.

Current optimizations:

- **Constant Folding**: `2+3` becomes `5` via compile time evaluation
- **Common Subexpression Elimination**: (via hash-consing) If you compute the same thing twice, we reuse it.
- [**Tree Shaking** a.k.a Live Code Inclusion](https://medium.com/@Rich_Harris/tree-shaking-versus-dead-code-elimination-d3765df85c80) (dead code removal): If something doesn’t contribute to the outputs, it disappears.
- **Reuse-driven reassociation**: More complex subexpression elimination, operations are re-arranged i.e. reassociated to reuse already existing operations.
- **Tree balancing reassociation** (work in progress): The depth of the circuit as a tree is reduced.

The goal is simple:
fewer gates = cheaper encrypted computation. It is important that we do as less as we can, and do it with minimal depth for fully homomorphic encryption because each operation has a cost incurred in terms of noise.

🔑 The last 2 optimizations are called algebraic simplifications, and are explained in detail in the wiki.

### Quick Example

```rust
let q = 11;

let ctx = new_folding_strict_context(q);

let i1 = ctx.input(0);
let i2 = ctx.input(1);

let output = &i1 * &i2 + &i1;

let circuit = ctx.compile(output).expect("should compile");
```

At this point we now have a circuit instead of a number.

```rust
let mut mock_fhe = MockFHEBackend::new(noise_budget);

let result = mock_fhe
    .eval_outputs(&circuit, &[1, 1])
    .expect("should evaluate");
```

Inputs correspond to the indices used when defining inputs. You can also implement your own backend to evaluate circuits.

### Multiple Outputs

You can also compile multiple outputs at once:

```rust
let output1 = i1 * i2;
let output2 = i1 + i2;
let output3 = i1 - i2;

let circuit = ctx.compile_many(&[output1, output2, output3]).unwrap();
```

Only the nodes required to compute those outputs survive compilation. Everything else is removed.

## Architecture (bird’s eye view)

```
DSL expressions
      ↓
Arithmetic DAG
      ↓
Optimization passes
      ↓
IR circuit
      ↓
Optimization passes
      ↓
Backend execution
```

### What the DSL Actually Builds

```rust
let out = a * b + a;
```

The DSL constructs a graph like this:

```
    (+)
   /   \
 (*)    a
 / \
a   b
```

And the circuit is like this:

```
Input a ───┐
           │
           ▼
        Multiply ────┐
           ▲         │
           │         ▼
Input b ───┘       Add ──→ Output
                    ▲
                    │
                 Input a

```

## Compilation Modes

The DSL supports strictness modes controlling what is allowed during construction.

```rust
pub enum StrictnessOn {
    Input,
    Const,
    Op,
}

pub struct Strictness(u8);
```

This mainly exists so you can control how aggressively the DSL enforces correctness during graph construction. You can ignore it if you just want to build circuits quickly. `dsl` module exposes helper functions to quickly create contexts with `Loose` and `Strict` compilation modes.

`Strictness` ensures that any unused provided target (`Input`, `Const` or `Op`) prevents the circuit from compilation with an error indicating which ones are unused. In a `Loose`context, any unused target is optimized - no worries - but we may want to know them thus the structures. `Strictness` structures support `+` and `-` to easily combine them, adding means including the strictness, subtracting is excluding i.e. loosening the strictness.

```rust
    let strictness: Strictness = (StrictnessOn::Input + StrictnessOn::Op).into();
```

## Current Limitations

- Only u64 arithmetic
- No real FHE backend yet
- Tree balancing pass is still cooking
- Batched execution not finished
- Parallel evaluation not finished

They are all in the todo list...

## Details

### Optimizations

- Constant folding and Common Sub-Expression Elimination is performed from the start in DSL. It is to keep the gate counts as minimal as possible from the start. We could keep it separate but I wanted an optimization to take place if we can identify that it can be done immediately.

- We eliminate dead code when we compile the `dsl::Context` into `ir::Circuit` by depth-first post-order traversing from root to leaf nodes. Any node that cannot be reached from output nodes are eliminated.

- The circuits are registers where each value in a Static Single Assignment form. This means that each variable is only assigned once to enable aggressive optimizations and easier data flow graphs.

#### Constant folding

We evaluate expressions with known values at compile time.

```rust
let q = 11;
// enables folding and other optimizations without causing compile time errors. It does not compile if an input or an operation is not used. 
let ctx = new_folding_strict_context(q);

let index_of_the_input = 0;
// will register an input node
let a = ctx.input(index_of_the_input);

// will register a constant node with value 1
let c1 = ctx.constant(1);
// will register a constant node with value 9
let c2 = ctx.constant(9);

// Since the value of c1 and c2 is known at compile time, it will not register an Op but perform the operation directly and instead registers a constant node with value 10. 
// As if it is let add_c1_c2 = ctx.constant(c1+c2);
let add_c1_c2 = c1+c2;
```

#### Common Sub-Expression Elimination

If we identify an expression that is repeating, we don't register the same expression but rather point to the already formed expression.  

```rust
let q = 11;
// enables folding and other optimizations without causing compile time errors. It does not compile if an input or an operation is not used. 
let ctx = new_folding_strict_context(q);

let index_of_the_input = 0;
// will register an input node
let input = ctx.input(index_of_the_input);
// will register a constant node
let constant = ctx.constant(1);

let input_plus_constant = &input + &constant;

// won't register the rhs node, will instead reference input_plus_constant
let input_plus_input_plus_constant = &input + (&input + &constant);
// &input + (&input + &constant) -> &input + &input_plus_constant
```

#### Tree-shaking - Live Code Inclusion

I first wrote it as Dead-code Elimination on top of my head but I realise that what we are performing is Live Code Inclusion. Compilation step starts with outputs, we know which `ExprHandle`s to start from. Thus it is not trying to find dead code, it is finding live code by traversing the graph in a post-order fashion. We are picking live expressions and lowering them to IR gates.

```rust
let q = 11;
// enables folding and other optimizations without causing compile time errors. It does not compile if an input or an operation is not used. 
let ctx = new_folding_strict_context(q);

// will register a constant node with value 1
let c1 = ctx.constant(1);
// will register a constant node with value 9
let c2 = ctx.constant(9);

// Since the value of c1 and c2 is known at compile time, it will not register an Op but perform the operation directly and instead registers a constant node with value 10.
let output = c1+c2;

// Since above (c1+c2) is not an Op node anymore, c1 and c2 is not alive anymore, in other terms, if we start traversing the graph in reverse from the output, c1 and c2 are unreachable from output node. 
let circuit = ctx.compile(&self, &[output]).expect("to compile"); 

// At this point, c1 and c2 constant nodes are not in the circuit anymore and compilation is successful because context has a compilation mode that allows such optimizations.
```

### Algebraic simplifications

#### Reuse-driven reassociation

This is a simplified version of LLVM's or GCCs reassociation pass. Since our circuits currently are simpler (no control-flow trickery yet), ours is a lot simpler but has the same idea.

If we have an operation instruction that has an already computed sub-expression (it must be an Op node that is of the same kind and the operation has to be associative and commutative), we reuse that to eliminate unnecessary operations. The algorithm has subtle points which are explained in wiki.

```rust
let x = a+b;
// this will be parsed as (a+c)+b SSA form which misses already registered and computed (a+b).
// let a_c = a+c;
// let y = a_c + b;
let y = a+c+b;  

// after the reassociation pass, it becomes by reassociating the operation: a+c+b -> a+b+c -> (a+b)+c -> x+c
// the intermediate a_c node is eliminated.
 let y = x+c;
```

It allows for further optimizations when we combine everything until now:

For example:

```rust
let x = a+b;
// this will be parsed as if the below code is written in SSA form which misses already registered and computed (a+b) and c1+c2 fold.
// let a_c1 = a+c1;
// let a_c1_c2 = a_c1 + c2;
// let y = a_c1_c2 + b;
let y = ((a+c1)+c2)+b;  

// after the first reassociation pass, (a+c1)+c2 -> (c1+c2)+a which will allow folding c1 and c2
// the intermediate a_c1 node is eliminated.
let c_fold = ctx.constant(c1+c2); // not an Op but the value as 
// (c_fold+a) will be as if let c_fold_a = c_fold+a; is an intermediate Op
let y = (c_fold+a)+b;

// after the second reassociation pass, (c_fold+a)+b -> c_fold+(a+b) which will allow reusing x
// the intermediate c_fold_a node is eliminated.
let y = c_fold+x;
```

#### Tree balancing reassociation (WIP)

```rust
let x = a+b+c+d+e+f+g+h;
// will be parsed as ((((((a+b)+c)+d)+e)+f)+g)+h which is left heavy tree, a linked list
/*
                +
               / \
              +   h
             / \
            +   g
           / \
          +   f
         / \
        +   e
       / \
      +   d
     / \
    +   c
   / \
  a   b
*/

// after the balancing reassociation pass, it becomes 
// ((a+b)+(c+d))+((e+f)+(g+h)) which is left heavy tree, a linked list
/* 
              +
            /   \
           L     R

       L            R
     /   \        /  \ 
    +     +      +    +
   / \   / \    / \  / \
  a  b  c   d  e  f  g  h
*/
// which is the same tree as 
let x1 = a+b;
let x2 = c+d;

let y1 = e+f;
let y2 = g+h;

let x = x1+x2;
let y = y1+y2;

let output = x + y;
```
