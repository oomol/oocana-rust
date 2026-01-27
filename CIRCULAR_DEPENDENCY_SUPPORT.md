# Circular Dependency Support (Lazy Loading)

## Overview

The manifest parsing system now supports **lazy loading** of subflows to handle circular dependencies at parse time while deferring infinite loop detection to runtime.

## Problem Solved

Previously, if a flow referenced itself (e.g., `A.flow` contains a subflow node pointing to `A.flow`), the parser would fail with stack overflow during the parse phase:

```
A.flow → resolve subflow A → parse A.flow → resolve subflow A → ∞
```

However, in many cases, circular references are **conditionally executed** at runtime and may not actually cause infinite loops:

```yaml
# A.flow - self-reference with conditional control
nodes:
  - id: task1
    type: task
    task: ./do_something
  - id: check_condition
    type: condition
    # Decides whether to continue looping
  - id: maybe_loop
    type: subflow
    subflow: ./A.flow  # Self-reference!
    inputs_from:
      - from_node: [check_condition, should_continue]
```

If `check_condition` returns `false`, the loop never executes.

## Solution: Lazy Loading

### How It Works

1. **Parse-time detection**: When parsing a flow, if a circular dependency is detected (same flow being resolved in the call stack), the parser returns a `FlowReference::Lazy` instead of failing.

2. **Minimal node creation**: A simplified `SubflowNode` is created with:
   - The lazy flow reference
   - Empty inputs definition (runtime will handle)
   - No slots processing (skipped to avoid complexity)
   - Default scope

3. **Runtime resolution**: The execution engine must:
   - Check if a flow reference is lazy before execution
   - Resolve the actual flow at runtime
   - Track execution depth to detect **actual** infinite loops

### API Usage

#### Enabling Lazy Loading

Use `resolve_flow_block_lazy()` instead of `resolve_flow_block()`:

```rust
// Old: Fails on circular dependency
let flow = block_resolver.resolve_flow_block("A.flow", &mut path_finder)?;

// New: Returns FlowReference (Resolved or Lazy)
let flow_ref = block_resolver.resolve_flow_block_lazy("A.flow", &mut path_finder)?;

// Check if lazy
if flow_ref.is_lazy() {
    warn!("Flow has circular dependency, must be resolved at runtime");
}

// Access resolved flow (panics if lazy)
let flow_arc = flow_ref.expect_resolved("Flow must be resolved before use");
```

#### FlowReference API

```rust
pub enum FlowReference {
    /// Fully resolved subflow
    Resolved(Arc<SubflowBlock>),

    /// Lazy reference due to circular dependency
    Lazy {
        flow_name: String,
        flow_path: PathBuf,
    },
}

impl FlowReference {
    pub fn is_lazy(&self) -> bool;
    pub fn resolved(&self) -> Option<&Arc<SubflowBlock>>;
    pub fn expect_resolved(&self, msg: &str) -> &Arc<SubflowBlock>;
    pub fn into_resolved(self) -> Arc<SubflowBlock>;
}
```

### Internal Implementation

The `SubflowBlock::from_manifest` method accepts a `use_lazy_loading` parameter:

```rust
SubflowBlock::from_manifest(
    manifest,
    flow_path,
    block_resolver,
    path_finder,
    true,  // Enable lazy loading
)
```

When `use_lazy_loading = true`:
- Calls `resolve_flow_block_lazy()` for subflow nodes
- Detects circular dependencies and returns `FlowReference::Lazy`
- Creates minimal nodes for lazy references, skipping:
  - Slots processing
  - Complex input/output resolution
  - Injection handling

### Runtime Requirements

⚠️ **Important**: The execution engine MUST:

1. **Check for lazy references**:
   ```rust
   if subflow_node.flow.is_lazy() {
       // Resolve the flow at runtime
       let resolved_flow = runtime_resolve_flow(&flow_ref)?;
   }
   ```

2. **Track execution depth**:
   ```rust
   const MAX_RUNTIME_DEPTH: usize = 100;

   fn execute_subflow(flow: &SubflowBlock, depth: usize) -> Result<()> {
       if depth > MAX_RUNTIME_DEPTH {
           return Err("Runtime recursion limit exceeded");
       }
       // Execute...
   }
   ```

3. **Provide clear error messages** when actual infinite loops occur

## Examples

### Self-Referencing Flow (Valid)

```yaml
# recursive_processor.flow
inputs:
  - handle: items
    type: array
  - handle: index
    type: number

nodes:
  - id: process_current
    type: task
    task: ./process_item
    inputs_from:
      - handle: item
        from_flow: [items, $index]

  - id: check_more
    type: condition
    expression: "$index + 1 < len($items)"

  - id: process_next
    type: subflow
    subflow: ./recursive_processor.flow  # Self-reference
    inputs_from:
      - handle: items
        from_flow: [items]
      - handle: index
        value: "$index + 1"
      - from_node: [check_more, result]  # Only executes if true
```

### Mutually Recursive Flows (Valid)

```yaml
# flow_a.flow
nodes:
  - id: task_a
    type: task
  - id: maybe_call_b
    type: subflow
    subflow: ./flow_b.flow
    # Conditional based on task_a output
```

```yaml
# flow_b.flow
nodes:
  - id: task_b
    type: task
  - id: maybe_call_a
    type: subflow
    subflow: ./flow_a.flow
    # Conditional based on task_b output
```

## Migration Guide

### For Library Users

No changes required for existing code. Lazy loading is opt-in via `resolve_flow_block_lazy()`.

### For Execution Engine Developers

1. Check for lazy references before execution
2. Implement runtime flow resolution
3. Add execution depth tracking
4. Update error messages to distinguish parse-time vs runtime recursion

## Performance Notes

- **Parse time**: Minimal overhead, only when circular dependencies exist
- **Memory**: Lazy nodes use less memory (no slots, simplified structure)
- **Runtime**: First execution of a lazy node incurs resolution cost

## Limitations

- **Slots not supported** for circular references (skipped during parse)
- **Inputs definition** may be incomplete for lazy nodes
- **Injection** not processed for circular references
- Runtime **must** implement depth checking to prevent infinite loops

## See Also

- `manifest_meta/src/node/subflow.rs` - FlowReference definition
- `manifest_meta/src/block_resolver.rs` - Lazy resolution logic
- `manifest_meta/src/flow.rs` - Minimal node creation
