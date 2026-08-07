//! The guarded contracts. Everything else in the pipeline may churn; these
//! are stable and golden-seed tested. C1/C2/C3 guard internal seams; C0
//! guards the external one — the bundle delivered to the frontend team.

pub mod corridor_graph;
pub mod delivery;
pub mod primitive_field;
pub mod routing_substrate;
