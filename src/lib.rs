extern crate self as sts_simulator;

pub mod content;
pub mod engine;
mod ids;
pub mod runtime;
pub mod sim;
pub mod state;

// Backward-compatible crate-private path for existing monster content. The
// actual move-plan types live with runtime combat state now.
mod semantics {
    pub mod combat {
        pub use crate::runtime::monster_move::*;
    }
}

// Stable policy layers that sit directly on the simulator domain.
pub mod ai;
mod testing;
pub use testing::fixtures;
pub use testing::support as test_support;

// Crate-private compatibility for Java-content modules that still use the old
// reward paths. The active ownership is `state::rewards` plus engine handlers;
// there is intentionally no `src/rewards` implementation tree.
mod rewards {
    pub mod generator {
        pub use crate::state::rewards::generator::*;
    }

    #[cfg(test)]
    pub mod handler {
        pub use crate::engine::reward_handler::*;
    }

    pub mod state {
        pub use crate::state::rewards::*;
    }
}

pub use ids::EntityId;
