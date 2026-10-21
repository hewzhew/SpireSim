#[derive(Debug, Clone)]
pub struct EventContext {
    pub act_num: u8,
    pub ascension_level: u8,
    /// Java `Settings.isDailyRun`; disables Note For Yourself.
    pub is_daily_run: bool,
    /// Java `player.getPrefs().getInteger("ASCENSION_LEVEL")`; Note For
    /// Yourself is available on A1-A14 only when the current ascension is lower
    /// than the highest unlocked ascension for the profile.
    pub highest_unlocked_ascension_level: u8,
    pub floor_num: i32,
    /// Java `AbstractDungeon.currMapNode.y`; `None` mirrors the Java null
    /// current-node guard used by events such as Colosseum.
    pub map_current_y: Option<i32>,
    /// Java `AbstractDungeon.map.size()`.
    pub map_height: usize,
    pub gold: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    /// Java `CardCrawlGame.playtime`, in seconds. Used by the Act 3
    /// `SecretPortal` special one-time event gate.
    pub playtime_seconds: f32,

    // Conditions used for specific events (e.g., Fountain of Curse Cleansing requires a curse)
    pub has_curses: bool,

    pub tiny_chest_counter: i32,

    // Used by Moai Head
    pub has_golden_idol: bool,

    // Used by Juzu Bracelet (overrides monster roll inside event chance calculation)
    pub has_juzu_bracelet: bool,

    // Java EventHelper.roll() suppresses shop results when the previous room
    // is a ShopRoom. The roll happens before the current ? node replaces the
    // previous current room.
    pub previous_room_was_shop: bool,

    // Used by Nloth (requires at least 2 relics)
    pub relic_count: usize,
}
