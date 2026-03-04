![Rust Version](https://img.shields.io/badge/Rust-1.92.0-orange)

![MSRV](https://img.shields.io/badge/MSRV-1.92-orange)

# Fully Homomorphic Encryption Domain Specific Language

This project is a simple DSL for building FHE circuits from arbitrary code performing arithmetic operations. By overloading operations of full-homomorphism (addition, subtraction and multiplication), we build an arithmetic circuit of each. Currently only `u64` is supported, but `float` is on the way.

We currently perform 5 optimizations, 2 of which is algebraic simplifications:
- Constant folding
- Common Sub-Expression Elimination (via hash-consing)
- [Three-Shaking a.k.a Live Code Inclusion](https://medium.com/@Rich_Harris/tree-shaking-versus-dead-code-elimination-d3765df85c80)
- Reuse-driven reassociation 
- Tree balancing reassociation (WIP)

Constant folding and Common Sub-Expression Elimination is performed from the start in DSL. It is to keep the gate counts as minimal as possible from the start. We could keep it separate but I wanted an optimization to take place if we can identify that it can be done immediately.   

We eliminate dead code when we compile the `dsl::Context` into `ir::Circuit` by depth-first post-order traversing from root to leaf nodes. Any node that cannot be reached from output nodes are eliminated. 

```rust
let q = 11;
// ctx is a RefCell that is wrapped in RC so that handles can reach it to register nodes. 
let ctx : ContextHandle = new_folding_strict_context(q);

let i1 = ctx.input(0);
let i2 = ctx.input(1);

// more variables and operations 
...

// ContextHandle's expose 2 methods 

let output = i1*i2; // arbitrary arithmetic operations assigned to the final handle 
// Dead-code elimination takes place during compilation, DFS from output eliminates unreachable nodes
let circuit = ctx.compile(&self, output).expect("to compile"); 

// OR 

let output_1 = i1*i2; // arbitrary arithmetic operations assigned to the final handle 
let output_2 = i1+i2; // arbitrary arithmetic operations assigned to the final handle 
let output_3 = i1-i2// arbitrary arithmetic operations assigned to the final handle 

// Dead-code elimination takes place during compilation, DFS from each output eliminates unreachable nodes
let circuit = ctx.compile_many(&self, &[output_1, output_2, output_3]).expect("to compile"); 

// After compilation, the circuit can be used with an appropriate Backend as follows
let mut mock_fhe = MockFHEBackend::new(noise_budget);
let results = mock_fhe 
    // The inputs &[1,1] corresponding the above inputs each index (0,1) will take that element at that index as an input value.
    .eval_outputs(&circuit, &[1, 1])
    .expect("should have evaluated");
```

## What we are supporting 
- dynamic compilation modes
- multiple evaluation of the same circuit
- batched execution (wip)
- parallel execution (wip)

## General View 

The main library module to start arbitrary computations is `dsl`. It exposes helper functions to quickly create contexts with `Loose` and `Strict` compilation modes. For more dynamic and granular control over compilation modes, we expose 2 structures.

```rust
#[derive(Clone, Debug, Default, Sequence)]
pub enum StrictnessOn {
    #[default]
    Input,
    Const,
    Op,
}

#[derive(Clone, Debug, Default)]
pub struct Strictness(u8);
```

`Strictness` ensures that any unused provided target (`Input`, `Const` or `Op`) prevents the circuit to compile with appropriate errors indicating which ones are unused. In a `Loose`context, any unused target is optimized - no worries - but we may want to know them thus the structures. `Strictness` structures support `+` and `-` to easily combine them, adding means including the strictness, subtracting is excluding i.e. loosening the strictness. 

```rust
    let strictness: Strictness = (StrictnessOn::Input + StrictnessOn::Op).into();
```

The circuits are registers each value in a Static Single Assignment form. This means that each variable is only assigned once to enable aggressive optimizations and easier data flow graphs. 

## Constant folding 

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

## Common Sub-Expression Elimination

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

## Tree-shaking - Live Code Inclusion

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

## Algebraic simplifications

### Reuse-driven reassociation 

This is a simplified version of LLVM's or GCCs reassociation pass. Since our circuits currently are simpler (no control-flow trickery yet), ours is a lot simpler but has the same base. 

The idea is that if we have an operation instruction that has an already computed sub-expression (it must be an Op node that is of the same kind and the operation has to be associative and commutative), thus we reuse that to eliminate unnecessary operations. The algorithm has subtle points which are explained in detail below.

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
### Tree balancing reassociation (WIP)


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
## Reuse-driven reassociation algorithm
## Tree balancing reassociation algorithm


