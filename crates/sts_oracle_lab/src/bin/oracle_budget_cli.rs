use clap::Args;
use sts_oracle_runtime::runtime::branch::OracleRunBudget;

#[derive(Clone, Copy, Debug, Args)]
pub(super) struct BudgetArgs {
    #[arg(long, default_value_t = 250_000)]
    hallway_generation_work: usize,
    #[arg(long, default_value_t = 5_000)]
    hallway_ms: u64,
    #[arg(long, default_value_t = 750_000)]
    elite_generation_work: usize,
    #[arg(long, default_value_t = 15_000)]
    elite_ms: u64,
    #[arg(long, default_value_t = 2_000_000)]
    boss_generation_work: usize,
    #[arg(long, default_value_t = 30_000)]
    boss_ms: u64,
}

impl BudgetArgs {
    pub(super) fn into_budget(self) -> OracleRunBudget {
        OracleRunBudget {
            hallway_generation_work: self.hallway_generation_work,
            hallway_ms: self.hallway_ms,
            elite_generation_work: self.elite_generation_work,
            elite_ms: self.elite_ms,
            boss_generation_work: self.boss_generation_work,
            boss_ms: self.boss_ms,
            ..OracleRunBudget::default()
        }
    }
}
