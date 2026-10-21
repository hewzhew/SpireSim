pub struct BustedCrown;

impl BustedCrown {
    // Busted Crown grants +1 Energy (typically managed by the overarching run state / Max Energy initialization),
    // and reduces card reward choices by 2 (out of scope for pure combat engine).
    // The combat properties for +1 energy are handled when assembling the `PlayerEntity` properties
    // at the start of combat, therefore no dynamic execution hooks are required.
}
