# Permanent compiler phase graph

`validate_dependency_graph` checks registered/duplicate edges, applies the permanent
`allowed_layer_edge` matrix to the actual graph (or the existing fallback when Cargo
inspection fails), and rejects cycles using `has_graph_cycle`.

It depends only on ordered collections, contract models and shared diagnostics.
The foundation/frontend/compiler/backend/orchestrator/application directions retain
the existing authority boundaries; moving this code adds no dependency permission.

Run `cargo test --locked -p zryna-architecture`. Phase-direction, actual-graph and cycle
cases are in `src/tests/cargo_graph.rs`, including
`permanent_phase_graph_forbids_compiler_and_backend_provider_edges`.
