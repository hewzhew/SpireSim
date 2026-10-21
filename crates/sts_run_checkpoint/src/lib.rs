//! Durable exact-run checkpoint encoding, validation, and archive joins.

extern crate self as sts_simulator;

pub use sts_oracle_learning_env::{agent, ai};

pub mod eval {
    pub mod run_control {
        pub use sts_oracle_learning_env::eval::run_control::*;
    }
}

#[path = "../../../src/eval/run_control/run_branch_checkpoint_artifact.rs"]
mod run_branch_checkpoint_artifact;

pub use run_branch_checkpoint_artifact::{
    TaskRunBranchCheckpointArtifactV1, TaskRunBranchCheckpointDecodeProfileV1,
    TaskRunBranchCheckpointIdentityV1, TaskRunBranchCheckpointPublicRootV1,
    TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_FORMAT_VERSION, TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC,
};
