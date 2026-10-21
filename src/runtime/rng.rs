//! Bit-perfect replicas of the Slay the Spire RNG subsystems.
//!
//! The game uses two distinct PRNG algorithms:
//! - **RandomXS128** (LibGDX): xorshift128+ for all gameplay randomness
//! - **java.util.Random** (LCG): only used by `Collections.shuffle()`
//!
//! Both are replicated at the bit level. Using any other RNG source
//! (e.g., `rand` crate, `thread_rng`) would break deterministic replay.

// ─── RandomXS128 (LibGDX) ───────────────────────────────────────────────────

/// Bit-perfect replica of LibGDX `RandomXS128`, the core PRNG for Slay the Spire.
///
/// Seeded via `murmurHash3(seed)` exactly as in Java. Every public method
/// increments `counter` to enable RNG consumption tracking.
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

thread_local! {
    static ACTIVE_STS_RNG_TRACE: RefCell<Option<Vec<StsRngTraceEvent>>> =
        const { RefCell::new(None) };
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StsRng {
    pub seed0: u64,
    pub seed1: u64,
    pub counter: u32,
    #[serde(skip)]
    trace_owner: Option<&'static str>,
}

impl PartialEq for StsRng {
    fn eq(&self, other: &Self) -> bool {
        self.seed0 == other.seed0 && self.seed1 == other.seed1 && self.counter == other.counter
    }
}

/// Runtime-local call shape recorded only while an explicit trace scope is
/// active. Stable owner metadata travels with an RNG across ordinary moves and
/// clones, but is deliberately omitted from checkpoints and semantic equality.
#[derive(Clone, Debug, PartialEq)]
pub enum StsRngCall {
    RandomInt {
        range: i32,
        result: i32,
    },
    RandomIntRange {
        start: i32,
        end: i32,
        result: i32,
    },
    RandomBoolean {
        result: bool,
    },
    RandomBooleanChance {
        chance: f32,
        result: bool,
    },
    RandomFloat {
        result: f32,
    },
    RandomFloatRange {
        range: f32,
        result: f32,
    },
    RandomFloatMinMax {
        minimum: f32,
        maximum: f32,
        result: f32,
    },
    RandomLongBits {
        result: u64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct StsRngTraceEvent {
    pub owner: Option<&'static str>,
    pub call: StsRngCall,
}

/// A non-nestable, thread-local trace scope. Dropping an unfinished scope
/// discards its calls so an error or unwind cannot pollute the next action.
pub struct StsRngTraceGuard {
    finished: bool,
    not_send: PhantomData<Rc<()>>,
}

impl StsRngTraceGuard {
    pub fn finish(mut self) -> Result<Vec<StsRngTraceEvent>, String> {
        let events = ACTIVE_STS_RNG_TRACE.with(|active| active.borrow_mut().take());
        self.finished = true;
        events.ok_or_else(|| "STS RNG trace scope is no longer active".to_owned())
    }
}

impl Drop for StsRngTraceGuard {
    fn drop(&mut self) {
        if !self.finished {
            ACTIVE_STS_RNG_TRACE.with(|active| {
                active.borrow_mut().take();
            });
        }
    }
}

pub fn begin_sts_rng_trace() -> Result<StsRngTraceGuard, String> {
    let started = ACTIVE_STS_RNG_TRACE.with(|active| {
        let mut active = active.borrow_mut();
        if active.is_some() {
            false
        } else {
            *active = Some(Vec::new());
            true
        }
    });
    started
        .then_some(StsRngTraceGuard {
            finished: false,
            not_send: PhantomData,
        })
        .ok_or_else(|| "STS RNG trace scopes cannot be nested".to_owned())
}

impl StsRng {
    /// Construct one RNG from explicit RandomXS128 state without attaching
    /// transition-trace metadata.
    pub fn from_state(seed0: u64, seed1: u64, counter: u32) -> Self {
        Self {
            seed0,
            seed1,
            counter,
            trace_owner: None,
        }
    }

    pub(crate) fn new_named(seed: u64, owner: &'static str) -> Self {
        let mut rng = Self::new(seed);
        rng.trace_owner = Some(owner);
        rng
    }

    /// Attach runtime-only owner metadata after checkpoint restoration.
    /// This does not affect serialization or semantic equality.
    #[doc(hidden)]
    pub fn bind_trace_owner(&mut self, owner: &'static str) {
        self.trace_owner = Some(owner);
    }
}

fn record_sts_rng_call(rng: &StsRng, call: StsRngCall) {
    ACTIVE_STS_RNG_TRACE.with(|active| {
        if let Some(events) = active.borrow_mut().as_mut() {
            events.push(StsRngTraceEvent {
                owner: rng.trace_owner,
                call,
            });
        }
    });
}

impl Default for StsRng {
    fn default() -> Self {
        Self::new(0)
    }
}

impl StsRng {
    /// Create a new RNG from a game seed, matching Java's `new Random(seed)`.
    pub fn new(seed: u64) -> Self {
        let seed = if seed == 0 { i64::MIN as u64 } else { seed };
        let seed0 = murmur_hash3(seed);
        let seed1 = murmur_hash3(seed0);
        Self {
            seed0,
            seed1,
            counter: 0,
            trace_owner: None,
        }
    }

    /// Advance the RNG by `count` calls, used for save-file restoration.
    pub fn new_with_counter(seed: u64, count: u32) -> Self {
        let mut rng = Self::new(seed);
        for _ in 0..count {
            rng.random(999);
        }
        rng
    }

    /// Recreate Java's `new Random(seed, source.counter)` while preserving the
    /// source stream's runtime-only trace owner.
    ///
    /// The original constructor advances through `random(999)` once per prior
    /// call. This is observably different from cloning the current XS128 state
    /// even though both normally land on the same bits.
    pub fn duplicate_from_seed_and_counter(&self, seed: u64) -> Self {
        let mut rng = Self::new(seed);
        rng.trace_owner = self.trace_owner;
        for _ in 0..self.counter {
            rng.random(999);
        }
        rng
    }

    /// Advance to a target counter using Java `Random.setCounter` semantics.
    ///
    /// Java does not assign the counter directly. It repeatedly calls
    /// `randomBoolean()` until the target counter is reached, which advances the
    /// underlying RNG state as well as the public counter.
    pub fn advance_counter_to(&mut self, target_counter: u32) {
        while self.counter < target_counter {
            let _ = self.random_boolean();
        }
    }

    // ── Core xorshift128+ ──────────────────────────────────────────────

    #[inline]
    fn next_long(&mut self) -> u64 {
        let mut s1 = self.seed0;
        let s0 = self.seed1;
        self.seed0 = s0;
        s1 ^= s1 << 23;
        self.seed1 = s1 ^ s0 ^ (s1 >> 17) ^ (s0 >> 26);
        self.seed1.wrapping_add(s0)
    }

    /// Bounded u64 with rejection sampling (mirrors `RandomXS128.nextLong(n)`).
    #[inline]
    fn next_long_bounded(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        loop {
            let bits = self.next_long() >> 1;
            let value = bits % n;
            if bits.wrapping_sub(value).wrapping_add(n - 1) <= u64::MAX / 2 {
                return value;
            }
        }
    }

    #[inline]
    /// Direct RandomXS128.nextInt used by Java Collections.shuffle. This
    /// bypasses the STS Random wrapper and its counter/trace, just as Java does.
    pub(crate) fn direct_xs128_next_int(&mut self, n: i32) -> i32 {
        self.next_int_bounded(n)
    }

    fn next_int_bounded(&mut self, n: i32) -> i32 {
        self.next_long_bounded(n as u64) as i32
    }

    #[inline]
    fn next_float(&mut self) -> f32 {
        (self.next_long() >> 40) as f32 * (1.0 / (1u64 << 24) as f32)
    }

    #[inline]
    fn next_boolean(&mut self) -> bool {
        (self.next_long() & 1) != 0
    }

    // ── STS wrapper layer (maps to com.megacrit.cardcrawl.random.Random) ─

    /// `random(range)` → uniform integer in `[0, range]` (inclusive).
    #[track_caller]
    pub fn random(&mut self, range: i32) -> i32 {
        self.counter += 1;
        if std::env::var("RNG_TRACE").is_ok() {
            let loc = std::panic::Location::caller();
            eprintln!(
                "  [RNG] counter={} random({}) at {}:{}",
                self.counter,
                range,
                loc.file(),
                loc.line()
            );
        }
        let result = self.next_int_bounded(range + 1);
        record_sts_rng_call(self, StsRngCall::RandomInt { range, result });
        result
    }

    /// `random(start, end)` → uniform integer in `[start, end]` (inclusive).
    #[track_caller]
    pub fn random_range(&mut self, start: i32, end: i32) -> i32 {
        self.counter += 1;
        if std::env::var("RNG_TRACE").is_ok() {
            let loc = std::panic::Location::caller();
            eprintln!(
                "  [RNG] counter={} random_range({},{}) at {}:{}",
                self.counter,
                start,
                end,
                loc.file(),
                loc.line()
            );
        }
        let result = start + self.next_int_bounded(end - start + 1);
        record_sts_rng_call(self, StsRngCall::RandomIntRange { start, end, result });
        result
    }

    /// `randomBoolean()` → 50/50.
    #[track_caller]
    pub fn random_boolean(&mut self) -> bool {
        self.counter += 1;
        if std::env::var("RNG_TRACE").is_ok() {
            let loc = std::panic::Location::caller();
            eprintln!(
                "  [RNG] counter={} random_boolean() at {}:{}",
                self.counter,
                loc.file(),
                loc.line()
            );
        }
        let result = self.next_boolean();
        record_sts_rng_call(self, StsRngCall::RandomBoolean { result });
        result
    }

    /// `randomBoolean(chance)` → true with probability `chance`.
    #[track_caller]
    pub fn random_boolean_chance(&mut self, chance: f32) -> bool {
        self.counter += 1;
        if std::env::var("RNG_TRACE").is_ok() {
            let loc = std::panic::Location::caller();
            eprintln!(
                "  [RNG] counter={} random_boolean_chance({}) at {}:{}",
                self.counter,
                chance,
                loc.file(),
                loc.line()
            );
        }
        let result = self.next_float() < chance;
        record_sts_rng_call(self, StsRngCall::RandomBooleanChance { chance, result });
        result
    }

    /// `random()` → float in `[0.0, 1.0)`.
    #[track_caller]
    pub fn random_f32(&mut self) -> f32 {
        self.counter += 1;
        if std::env::var("RNG_TRACE").is_ok() {
            let loc = std::panic::Location::caller();
            eprintln!(
                "  [RNG] counter={} random_f32() at {}:{}",
                self.counter,
                loc.file(),
                loc.line()
            );
        }
        let result = self.next_float();
        record_sts_rng_call(self, StsRngCall::RandomFloat { result });
        result
    }

    /// `random(range)` → float in `[0.0, range)`.
    #[track_caller]
    pub fn random_f32_range(&mut self, range: f32) -> f32 {
        self.counter += 1;
        if std::env::var("RNG_TRACE").is_ok() {
            let loc = std::panic::Location::caller();
            eprintln!(
                "  [RNG] counter={} random_f32_range({}) at {}:{}",
                self.counter,
                range,
                loc.file(),
                loc.line()
            );
        }
        let result = self.next_float() * range;
        record_sts_rng_call(self, StsRngCall::RandomFloatRange { range, result });
        result
    }

    /// `random(min, max)` → float in `[min, max)` (MathUtils.random matching).
    #[track_caller]
    pub fn random_f32_min_max(&mut self, min: f32, max: f32) -> f32 {
        self.counter += 1;
        if std::env::var("RNG_TRACE").is_ok() {
            let loc = std::panic::Location::caller();
            eprintln!(
                "  [RNG] counter={} random_f32_min_max({},{}) at {}:{}",
                self.counter,
                min,
                max,
                loc.file(),
                loc.line()
            );
        }
        let result = min + self.next_float() * (max - min);
        record_sts_rng_call(
            self,
            StsRngCall::RandomFloatMinMax {
                minimum: min,
                maximum: max,
                result,
            },
        );
        result
    }

    /// `randomLong()` → raw u64, used as seed for `java.util.Random` in shuffles.
    #[track_caller]
    pub fn random_long(&mut self) -> u64 {
        self.counter += 1;
        if std::env::var("RNG_TRACE").is_ok() {
            let loc = std::panic::Location::caller();
            eprintln!(
                "  [RNG] counter={} random_long() at {}:{}",
                self.counter,
                loc.file(),
                loc.line()
            );
        }
        let result = self.next_long();
        record_sts_rng_call(self, StsRngCall::RandomLongBits { result });
        result
    }
}

fn murmur_hash3(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    x
}

// ─── java.util.Random (LCG) ─────────────────────────────────────────────────

/// Bit-perfect replica of `java.util.Random` (48-bit LCG).
///
/// Used exclusively by `Collections.shuffle(list, new Random(seed))` in the
/// Java engine. This is NOT the game's primary PRNG.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JavaUtilRandom {
    seed: u64,
}

const LCG_MULTIPLIER: u64 = 0x5DEECE66D;
const LCG_ADDEND: u64 = 0xB;
const LCG_MASK: u64 = (1u64 << 48) - 1;

impl JavaUtilRandom {
    pub fn new(seed: u64) -> Self {
        Self {
            seed: (seed ^ LCG_MULTIPLIER) & LCG_MASK,
        }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = self
            .seed
            .wrapping_mul(LCG_MULTIPLIER)
            .wrapping_add(LCG_ADDEND)
            & LCG_MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// `nextInt(bound)` with rejection sampling, matching Java exactly.
    pub fn next_int(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            return 0;
        }
        if bound & (bound - 1) == 0 {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits - val + (bound - 1) >= 0 {
                return val;
            }
        }
    }
}

// ─── Shuffle ─────────────────────────────────────────────────────────────────

/// Mirrors `Collections.shuffle(list, new Random(rng.randomLong()))`.
///
/// Consumes exactly one `nextLong()` from the game's xorshift128+ RNG,
/// then uses `java.util.Random` (LCG) for the Fisher-Yates shuffle.
pub fn shuffle_with_random_long<T>(items: &mut [T], game_rng: &mut StsRng) {
    let seed = game_rng.random_long();
    let mut jur = JavaUtilRandom::new(seed);
    for i in (1..items.len()).rev() {
        let j = jur.next_int((i + 1) as i32) as usize;
        items.swap(i, j);
    }
}

// ─── RNG Pool ────────────────────────────────────────────────────────────────

/// All RNG streams used by the Java engine plus one isolated deterministic
/// stand-in for mechanical uses of LibGDX `MathUtils.random`.
///
/// Seven are persistent across floors (seeded once at run start):
///   `monster_rng`, `event_rng`, `merchant_rng`, `card_rng`,
///   `treasure_rng`, `relic_rng`, `potion_rng`
///
/// Five are re-seeded per floor via `generate_floor_seeds()`:
///   `monster_hp_rng`, `ai_rng`, `shuffle_rng`, `card_random_rng`, `misc_rng`
///
/// `math_rng` is not a normal STS dungeon RNG stream. It exists because a few
/// Java mechanical outcomes incorrectly use LibGDX `MathUtils.random`, whose
/// global state is also consumed by UI effects. The simulator consumes
/// `math_rng` only for those mechanical outcomes, never for UI-only effects.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RngPool {
    // Persistent (run-level)
    pub monster_rng: StsRng,
    pub event_rng: StsRng,
    pub merchant_rng: StsRng,
    pub card_rng: StsRng,
    pub treasure_rng: StsRng,
    pub relic_rng: StsRng,
    pub potion_rng: StsRng,
    // Per-floor (re-seeded on room transition)
    pub monster_hp_rng: StsRng,
    pub ai_rng: StsRng,
    pub shuffle_rng: StsRng,
    pub card_random_rng: StsRng,
    pub misc_rng: StsRng,
    // Simulator-owned isolation for mechanically relevant MathUtils.random.
    pub math_rng: StsRng,
}

impl Default for RngPool {
    fn default() -> Self {
        Self::new(0)
    }
}

impl RngPool {
    /// Initialize all streams from a single game seed (mirrors `generateSeeds()`).
    pub fn new(seed: u64) -> Self {
        let mut pool = Self {
            monster_rng: StsRng::new(seed),
            event_rng: StsRng::new(seed),
            merchant_rng: StsRng::new(seed),
            card_rng: StsRng::new(seed),
            treasure_rng: StsRng::new(seed),
            relic_rng: StsRng::new(seed),
            potion_rng: StsRng::new(seed),
            monster_hp_rng: StsRng::new(seed),
            ai_rng: StsRng::new(seed),
            shuffle_rng: StsRng::new(seed),
            card_random_rng: StsRng::new(seed),
            misc_rng: StsRng::new(seed),
            math_rng: StsRng::new(seed),
        };
        pool.bind_trace_owners();
        pool
    }

    /// Re-seed per-floor streams (mirrors `nextRoomTransition()` in Java).
    ///
    /// ```java
    /// monsterHpRng  = new Random(Settings.seed + (long)floorNum);
    /// aiRng         = new Random(Settings.seed + (long)floorNum);
    /// shuffleRng    = new Random(Settings.seed + (long)floorNum);
    /// cardRandomRng = new Random(Settings.seed + (long)floorNum);
    /// miscRng       = new Random(Settings.seed + (long)floorNum);
    /// ```
    pub fn generate_floor_seeds(&mut self, seed: u64, floor_num: i32) {
        let floor_seed = seed.wrapping_add(floor_num as u64);
        self.monster_hp_rng = StsRng::new(floor_seed);
        self.ai_rng = StsRng::new(floor_seed);
        self.shuffle_rng = StsRng::new(floor_seed);
        self.card_random_rng = StsRng::new(floor_seed);
        self.misc_rng = StsRng::new(floor_seed);
        self.bind_trace_owners();
    }

    /// Restore runtime-only owner metadata omitted from checkpoints.
    #[doc(hidden)]
    pub fn bind_trace_owners(&mut self) {
        for (owner, rng) in [
            ("run.monster", &mut self.monster_rng),
            ("run.event", &mut self.event_rng),
            ("run.merchant", &mut self.merchant_rng),
            ("run.card", &mut self.card_rng),
            ("run.treasure", &mut self.treasure_rng),
            ("run.relic", &mut self.relic_rng),
            ("run.potion", &mut self.potion_rng),
            ("floor.monster_hp", &mut self.monster_hp_rng),
            ("floor.ai", &mut self.ai_rng),
            ("floor.shuffle", &mut self.shuffle_rng),
            ("floor.card_random", &mut self.card_random_rng),
            ("floor.misc", &mut self.misc_rng),
            ("sim.math", &mut self.math_rng),
        ] {
            rng.bind_trace_owner(owner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{begin_sts_rng_trace, StsRng, StsRngCall};

    #[test]
    fn bounded_int_matches_original_random_xs128_state_vector() {
        let mut rng = StsRng::from_state(0x8182_FDA8_6E79_9352, 0x3D32_09D9_FF1E_FF5E, 0);

        assert_eq!(
            [rng.random(13), rng.random(13), rng.random(13)],
            [12, 5, 11]
        );
    }

    #[test]
    fn advance_counter_to_matches_java_set_counter_random_boolean_consumption() {
        let mut actual = StsRng::new(42);
        let mut expected = actual.clone();

        actual.advance_counter_to(250);
        for _ in 0..250 {
            let _ = expected.random_boolean();
        }

        assert_eq!(actual, expected);

        actual.advance_counter_to(100);
        assert_eq!(
            actual, expected,
            "Java Random.setCounter does not rewind when the counter is already past the target"
        );
    }

    #[test]
    fn seed_counter_duplicate_replays_java_constructor_calls_and_owner() {
        let mut source = StsRng::new(42);
        source.bind_trace_owner("run.event");
        let _ = source.random_f32();
        let _ = source.random(7);
        let guard = begin_sts_rng_trace().unwrap();

        let duplicate = source.duplicate_from_seed_and_counter(42);
        let events = guard.finish().unwrap();

        assert_eq!(duplicate, source);
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|event| {
            event.owner == Some("run.event")
                && matches!(event.call, StsRngCall::RandomInt { range: 999, .. })
        }));
    }

    #[test]
    fn scoped_trace_records_exact_calls_without_changing_rng_semantics() {
        let mut traced = StsRng::new(42);
        let mut expected = traced.clone();
        let guard = begin_sts_rng_trace().unwrap();
        let int = traced.random(7);
        let int_range = traced.random_range(3, 9);
        let boolean = traced.random_boolean();
        let chance = traced.random_boolean_chance(0.33);
        let float = traced.random_f32();
        let float_range = traced.random_f32_range(2.5);
        let float_min_max = traced.random_f32_min_max(-1.0, 4.0);
        let long = traced.random_long();
        let events = guard.finish().unwrap();

        assert_eq!(int, expected.random(7));
        assert_eq!(int_range, expected.random_range(3, 9));
        assert_eq!(boolean, expected.random_boolean());
        assert_eq!(chance, expected.random_boolean_chance(0.33));
        assert_eq!(float, expected.random_f32());
        assert_eq!(float_range, expected.random_f32_range(2.5));
        assert_eq!(float_min_max, expected.random_f32_min_max(-1.0, 4.0));
        assert_eq!(long, expected.random_long());
        assert_eq!(traced, expected);
        assert!(events.iter().all(|event| event.owner.is_none()));
        assert_eq!(
            events
                .into_iter()
                .map(|event| event.call)
                .collect::<Vec<_>>(),
            vec![
                StsRngCall::RandomInt {
                    range: 7,
                    result: int,
                },
                StsRngCall::RandomIntRange {
                    start: 3,
                    end: 9,
                    result: int_range,
                },
                StsRngCall::RandomBoolean { result: boolean },
                StsRngCall::RandomBooleanChance {
                    chance: 0.33,
                    result: chance,
                },
                StsRngCall::RandomFloat { result: float },
                StsRngCall::RandomFloatRange {
                    range: 2.5,
                    result: float_range,
                },
                StsRngCall::RandomFloatMinMax {
                    minimum: -1.0,
                    maximum: 4.0,
                    result: float_min_max,
                },
                StsRngCall::RandomLongBits { result: long },
            ]
        );
    }

    #[test]
    fn dropped_or_nested_trace_scopes_cannot_pollute_later_actions() {
        let mut rng = StsRng::new(9);
        let abandoned = begin_sts_rng_trace().unwrap();
        assert!(begin_sts_rng_trace().is_err());
        let _ = rng.random_boolean();
        drop(abandoned);

        let clean = begin_sts_rng_trace().unwrap();
        assert!(clean.finish().unwrap().is_empty());
    }

    #[test]
    fn set_counter_semantics_are_visible_as_boolean_calls() {
        let mut rng = StsRng::new(11);
        let guard = begin_sts_rng_trace().unwrap();
        rng.advance_counter_to(3);
        let events = guard.finish().unwrap();

        assert_eq!(events.len(), 3);
        assert!(events
            .iter()
            .all(|event| matches!(event.call, StsRngCall::RandomBoolean { .. })));
    }

    #[test]
    fn trace_owner_is_absent_from_checkpoints_and_semantic_equality() {
        let plain = StsRng::new(17);
        let mut named = plain.clone();
        named.bind_trace_owner("run.card");

        assert_eq!(named, plain);
        let encoded = serde_json::to_value(&named).unwrap();
        assert!(encoded.get("trace_owner").is_none());
        assert_eq!(serde_json::from_value::<StsRng>(encoded).unwrap(), plain);
    }
}
