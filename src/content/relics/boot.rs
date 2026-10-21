pub struct Boot;

impl Boot {
    pub fn on_attack_to_change_damage(damage: i32) -> i32 {
        if damage > 0 && damage < 5 {
            return 5;
        }
        damage
    }
}
