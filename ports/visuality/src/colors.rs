//! ColorsUtils.java and SlimeColors.java, verbatim constants.

pub const RED: u32 = 16735855;
pub const ORANGE: u32 = 16751185;
pub const YELLOW: u32 = 16318324;
pub const GREEN: u32 = 7077755;
pub const BLUE: u32 = 8245247;
pub const PURPLE: u32 = 16750831;

pub const RAINBOW: [u32; 6] = [RED, ORANGE, YELLOW, GREEN, BLUE, PURPLE];

pub const SLIME_VANILLA: u32 = 8978297;

pub fn get_random_color_of_rainbow(rng: &mut crate::rng::JavaRandom) -> u32 {
    RAINBOW[rng.next_int_bounded(RAINBOW.len() as u32) as usize]
}
