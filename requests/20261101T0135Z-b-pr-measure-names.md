## Change

A caller that only needs the bodies' names had to measure every body or read `document.inspect`. `document.inspect` lists definition bodies only, not the occurrences an assembly places
(`Body2 (Part:2)`).

`inspect.measure {names_only: true}` returns `{body_count, bodies: [{name, definition}]}`: each world body's name and
its definition's name, with nothing measured. The `bodies` filter applies. An unknown name is now refused before
anything is measured, with the same error as before.

The MCP `measure` tool takes `names_only`, directly and as a batch step. `summary` leaves a names list unchanged,
because there is nothing in it to narrow.

## Tests

- `large_assembly_tests::names_only_lists_every_world_body_without_measuring` uses a generated assembly. It checks:
  - the same count and names as a full measure, in the same order;
  - each `definition` matches the body's `definition_name()`;
  - no volumes and no totals;
  - the `bodies` filter;
  - an unknown name is refused.
- The MCP `batch_stops_at_first_error_and_rolls_back` test now also checks `names_only` passed directly, combined
  with `summary`, and as a batch step.

## Checks

- `cargo fmt --all --check`
- `cargo clippy -p solidcraft-engine -p solidcraft-mcp --all-targets -- -D warnings`
- `cargo test -p solidcraft-engine --lib -- large_assembly_tests cmd::inspect`
- `cargo test -p solidcraft-mcp`

The new tests fail without the change.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
