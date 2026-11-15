//! Port of `features/module/modules/combat/*` (slice 2 start): the small
//! top-level combat modules. Game-facing calls stay behind the same seams
//! upstream uses (events, `player`, `shouldBeAttacked`), injected so the
//! logic itself stays exact.

use crate::category::COMBAT;
use crate::module::Module;
use crate::value::{boolean, float, float_range, int, int_range, ValueKind};

// ----- ModuleNoMissCooldown.kt -----

/// Upstream: `ModuleNoMissCooldown`, two booleans, no logic body (the hooks
/// consume the settings from the attack path).
pub fn no_miss_cooldown() -> Module {
    Module::new(
        "NoMissCooldown",
        COMBAT,
        vec![
            // Disables the miss-cooldown of 10 ticks when missing an attack.
            boolean("RemoveAttackCooldown", true),
            // Cancels the attack when missing an attack.
            boolean("CancelAttackOnMiss", false),
        ],
    )
}

// ----- EntityMarginEvent seam + ModuleHitbox.kt -----

/// Upstream event `EntityMarginEvent` (world entity margin).
pub struct EntityMarginEvent {
    /// True when upstream's `event.entity.shouldBeAttacked()` holds.
    pub entity_should_be_attacked: bool,
    pub margin: f32,
}

/// Upstream: `ModuleHitbox` — enlarges the hitbox of other entities.
pub fn hitbox() -> Module {
    Module::new(
        "Hitbox",
        COMBAT,
        vec![
            float("Size", 0.1, 0.0, 1.0),      // tagBy(this)
            boolean("ApplyToDebugHitbox", true),
            boolean("ApplyToComponent", true),
        ],
    )
}

/// Upstream `marginHandler`: `if (event.entity.shouldBeAttacked()) event.margin = size`.
pub fn hitbox_margin_handler(module: &Module, event: &mut EntityMarginEvent) {
    let size = match module.setting("Size").map(|v| &v.kind) {
        Some(ValueKind::Float(v, _, _)) => *v as f32,
        _ => 0.1,
    };
    if event.entity_should_be_attacked {
        event.margin = size;
    }
}

// ----- PlayerPostTickEvent seam + ModuleKeepSprint.kt -----

/// The slice of the upstream `player` global this module reads.
#[derive(Clone, Copy, Debug)]
pub struct KeepSprintPlayer {
    pub is_sprinting: bool,
    pub hurt_time: i32,
}

/// Upstream `percentageChance` (`config/utils/PercentageChance.kt`):
/// `0f -> false; 100f -> true; else randomGetter.nextFloat(100f) < v`.
/// The random source is injected; the default maps to ThreadLocalRandom.
pub fn chance_is_hit(percentage: f64, random_0_to_100: impl FnOnce() -> f64) -> bool {
    match percentage {
        p if p == 0.0 => false,
        p if p == 100.0 => true,
        p => random_0_to_100() < p,
    }
}

pub struct KeepSprint {
    pub module: Module,
    /// Upstream `var sprinting` (prevents getting slowed multiple times in a tick).
    pub sprinting: bool,
}

/// Upstream settings: Motion 100..100 in 0..100, MotionWhenHurt 100..100 in
/// 0..100, HurtTime 1..10 in 1..10, percentageChance("Chance", 100).
pub fn keep_sprint() -> KeepSprint {
    KeepSprint {
        module: Module::new(
            "KeepSprint",
            COMBAT,
            vec![
                float_range("Motion", 100.0, 100.0, 0.0, 100.0),
                float_range("MotionWhenHurt", 100.0, 100.0, 0.0, 100.0),
                int_range("HurtTime", 1, 10, 1, 10),
                int("Chance", 100, 0, 100),
            ],
        ),
        sprinting: false,
    }
}

impl KeepSprint {
    /// Upstream `postTickHandler`: `sprinting = player.isSprinting`.
    pub fn on_player_post_tick(&mut self, player: &KeepSprintPlayer) {
        self.sprinting = player.is_sprinting;
    }

    fn range_random(a: f64, b: f64, x: f64) -> f64 {
        // ClosedFloatingPointRange.random(): uniform in [a, b].
        a + (b - a) * x
    }

    /// Upstream `getMotion()`:
    /// if (!chance.asBoolean) return 0.6
    /// else when { player.hurtTime in hurtTime -> motionWhenHurt; else -> motion }.random() / 100.0
    pub fn get_motion(
        &self,
        player: &KeepSprintPlayer,
        random_unit: impl FnOnce() -> f64,
        random_0_to_100: impl FnOnce() -> f64,
    ) -> f64 {
        let chance = match self.module.setting("Chance").map(|v| &v.kind) {
            Some(ValueKind::Int(v, _, _)) => *v as f64,
            _ => 100.0,
        };
        if !chance_is_hit(chance, random_0_to_100) {
            return 0.6;
        }
        let hurt = match self.module.setting("HurtTime").map(|v| &v.kind) {
            Some(ValueKind::IntRange(a, b, _, _)) => (*a..=*b).contains(&i64::from(player.hurt_time)),
            _ => false,
        };
        let (lo, hi) = match self.module.setting(if hurt { "MotionWhenHurt" } else { "Motion" }).map(|v| &v.kind) {
            Some(ValueKind::FloatRange(a, b, _, _)) => (*a, *b),
            _ => (100.0, 100.0),
        };
        Self::range_random(lo, hi, random_unit()) / 100.0
    }
}


// ----- ModuleKick.KickModeEnum + ModuleAutoLeave.kt -----

/// Upstream `ModuleKick.KickModeEnum` (exploit/ModuleKick.kt), tags kept 1:1.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KickMode {
    Quit,
    InvalidPacket,
    SelfHurt,
    IllegalChat,
    IllegalSwitchitem,
    IllegalInteract,
}

impl KickMode {
    pub fn tag(&self) -> &'static str {
        match self {
            KickMode::Quit => "Quit",
            KickMode::InvalidPacket => "InvalidPacket",
            KickMode::SelfHurt => "SelfHurt",
            KickMode::IllegalChat => "IllegalChat",
            KickMode::IllegalSwitchitem => "IllegalSwitchitem",
            KickMode::IllegalInteract => "IllegalInteract",
        }
    }
    pub const ALL_TAGS: [&'static str; 6] = ["Quit", "InvalidPacket", "SelfHurt", "IllegalChat", "IllegalSwitchitem", "IllegalInteract"];
}

/// Upstream `tickConditional` (`event/CoroutineTicker.kt`):
/// ticks == 0 -> !breakLoop; else counts ticks until breakLoop or count >= ticks,
/// passing only when the count reached ticks without a break.
fn tick_conditional_step(threshold: i64, break_loop: impl Fn() -> bool, count: i64) -> (i64, bool) {
    // One game tick of upstream tickConditional. Returns (new_count, passed).
    if break_loop() {
        return (0, false);
    }
    // Don't wait if ticks is 0: passes immediately once the break is false.
    if threshold == 0 {
        return (0, true);
    }
    let count = count + 1;
    (count, count >= threshold)
}

/// The slice of `player`/`mc` AutoLeave reads.
pub struct AutoLeavePlayer {
    pub instabuild: bool,
    pub singleplayer: bool,
    /// Either hand holds an item with DataComponents.DEATH_PROTECTION.
    pub can_heal_death_protection: bool,
    pub health: f32,
}

pub struct AutoLeave {
    pub module: Module,
    /// Upstream coroutine state: Some(running count) while counting, None fresh.
    counting: Option<i64>,
    /// The threshold drawn when the current counting started.
    threshold: i64,
}

/// Upstream: `ModuleAutoLeave` — leaves the server when health is low.
pub fn auto_leave() -> AutoLeave {
    AutoLeave {
        module: Module::new(
            "AutoLeave",
            COMBAT,
            vec![
                float("Health", 8.0, 0.0, 20.0),
                int_range("Delay", 0, 0, 0, 60),
                crate::value::choice("Mode", "Quit", &KickMode::ALL_TAGS),
            ],
        ),
        counting: None,
        threshold: 0,
    }
}

impl AutoLeave {
    fn health_threshold(&self) -> f32 {
        match self.module.setting("Health").map(|v| &v.kind) {
            Some(ValueKind::Float(v, _, _)) => *v as f32,
            _ => 8.0,
        }
    }

    /// One game tick. Returns the KickMode when the count passed (upstream then
    /// calls ModuleKick.kick(mode) and sets enabled = false).
    pub fn on_tick(
        &mut self,
        player: &AutoLeavePlayer,
        delay_random: impl FnOnce() -> (i64, i64),
    ) -> Option<KickMode> {
        let mode = match self.module.setting("Mode").map(|v| &v.kind) {
            Some(ValueKind::Choice(sel, _)) if sel == "InvalidPacket" => KickMode::InvalidPacket,
            Some(ValueKind::Choice(sel, _)) if sel == "SelfHurt" => KickMode::SelfHurt,
            Some(ValueKind::Choice(sel, _)) if sel == "IllegalChat" => KickMode::IllegalChat,
            Some(ValueKind::Choice(sel, _)) if sel == "IllegalSwitchitem" => KickMode::IllegalSwitchitem,
            Some(ValueKind::Choice(sel, _)) if sel == "IllegalInteract" => KickMode::IllegalInteract,
            _ => KickMode::Quit,
        };
        // A fresh threshold is drawn every time the counting (re)starts.
        if self.counting.is_none() {
            let (from, to) = delay_random();
            self.threshold = if to < from { from } else { fastrand_range(from, to) };
            self.counting = Some(0);
        }
        let threshold = self.threshold;
        // Upstream breakLoop: conditions that mean "do not leave".
        let break_loop = || {
            player.instabuild
                || player.singleplayer
                || player.can_heal_death_protection
                || player.health >= self.health_threshold()
        };
        let (count, passed) = tick_conditional_step(threshold, break_loop, self.counting.unwrap_or(0));
        if passed {
            self.counting = None;
            self.module.running = false;
            Some(mode)
        } else {
            // A break restarts the coroutine fresh next tick (count reset inside
            // the step); a quiet tick keeps counting.
            self.counting = if count == 0 { None } else { Some(count) };
            None
        }
    }
}

fn fastrand_range(from: i64, to: i64) -> i64 {
    // IntRange.random(): inclusive [from, to]. Seed source is the engine seam.
    use std::time::{SystemTime, UNIX_EPOCH};
    let span = (to - from + 1).max(1) as u64;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0);
    from + (nanos % span) as i64
}

// ----- ModuleMaceKill.kt -----

pub struct MaceKillPlayer {
    /// player.mainHandItem.item == Items.MACE
    pub holding_mace: bool,
    pub on_ground: bool,
    /// player.boundingBox, as an offset-check function: true when the box moved
    /// up by `offset` blocks would collide (world.getBlockCollisions not allEmpty).
    pub collides_at: Box<dyn Fn(f64) -> bool>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WarpCall {
    /// Some(dy) for a warp to position().add(0, height, 0); None for the
    /// player's current position.
    pub target_dy: Option<f64>,
    pub on_ground: bool,
}

/// Upstream: `ModuleMaceKill` — fakes fall height before a mace hit.
pub fn mace_kill() -> Module {
    Module::new("MaceKill", COMBAT, vec![int("FallHeight", 22, 1, 170)])
}

fn determine_height(fall_height: i64, collides_at: &dyn Fn(f64) -> bool) -> i64 {
    for i in 1..=fall_height {
        let offset = (fall_height - i + 1) as f64;
        if !collides_at(offset) {
            return offset as i64;
        }
    }
    0
}

/// Upstream `attackHandler`; returns the warp sequence for the engine seam.
pub fn mace_kill_attack_handler(module: &Module, player: &MaceKillPlayer) -> Option<Vec<WarpCall>> {
    if !player.holding_mace {
        return None;
    }
    let fall_height = match module.setting("FallHeight").map(|v| &v.kind) {
        Some(ValueKind::Int(v, _, _)) => *v,
        _ => 22,
    };
    let height = determine_height(fall_height, player.collides_at.as_ref());
    let mut calls = Vec::new();
    if height > 10 {
        // repeat(ceil(abs(height / 10.0)).toInt()) { player.warp(null, onGround = false) }
        let times = (height as f64 / 10.0).abs().ceil() as i64;
        for _ in 0..times {
            calls.push(WarpCall { target_dy: None, on_ground: false });
        }
    } else {
        // Do it at least twice to neutralize horizontal distance.
        for _ in 0..2 {
            calls.push(WarpCall { target_dy: None, on_ground: player.on_ground });
        }
    }
    calls.push(WarpCall { target_dy: Some(height as f64), on_ground: false });
    calls.push(WarpCall { target_dy: Some(0.0), on_ground: false });
    Some(calls)
}

// ----- Seams: use-item packet + ModuleSwordBlock.kt -----

/// Upstream `InteractionHand`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hand {
    MainHand,
    OffHand,
}

/// Item identity the module's logic reads (`isSword`, `is ShieldItem`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemKind {
    Sword,
    Shield,
    Other,
}

/// Upstream `ItemUseAnimation`; the module only distinguishes `!= BLOCK`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemUseAnimation {
    None,
    Eat,
    Drink,
    Block,
    Bow,
    Spear,
    Crossbow,
    Spyglass,
    TootHorn,
    Brush,
}

/// An item stack the module reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ItemStack {
    pub item_kind: ItemKind,
    pub use_animation: ItemUseAnimation,
}

impl ItemStack {
    /// Upstream `isSword`.
    fn is_sword(&self) -> bool {
        self.item_kind == ItemKind::Sword
    }

    /// Upstream `item is ShieldItem`.
    fn is_shield(&self) -> bool {
        self.item_kind == ItemKind::Shield
    }
}

/// Upstream `ServerboundUseItemPacket` (the fields the module touches).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ServerboundUseItemPacket {
    pub hand: Hand,
    pub sequence: i32,
    pub y_rot: f32,
    pub x_rot: f32,
}

/// Upstream event `PacketEvent`.
pub struct PacketEvent {
    pub packet: Packet,
    pub cancelled: bool,
}

pub enum Packet {
    UseItem(ServerboundUseItemPacket),
    Other,
}

/// The slice of the upstream `player` (or a rendered entity) this module reads.
#[derive(Clone, Copy, Debug)]
pub struct SwordBlockEntity {
    pub is_player: bool,
    pub main_hand_item: ItemStack,
    pub off_hand_item: ItemStack,
    pub y_rot: f32,
    pub x_rot: f32,
    /// `usingItemOrNull` paired with the hand that holds it (`isInHand`).
    pub using_item: Option<(ItemStack, Hand)>,
    /// `InputTracker.isPressedOnAny` on `mc.options.keyUse`.
    pub key_use_pressed: bool,
}

fn setting_bool(module: &Module, name: &str, default: bool) -> bool {
    match module.setting(name).map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => default,
    }
}

/// Upstream: `ModuleSwordBlock` — shows blocking with a shield as 1.8 sword
/// blocking. A sword that cannot block by itself (1.9 to 1.21.3) uses the
/// shield as well, which ViaVersion puts into the offhand for 1.8 servers.
/// Upstream aliases: ["OldBlocking"]. Upstream marks FakeOnPressing,
/// HideShieldSlot, ApplyToThirdPersonView and AlwaysHideShield
/// `doNotIncludeAlways` (config-export metadata only).
pub fn sword_block() -> Module {
    Module::new(
        "SwordBlock",
        COMBAT,
        vec![
            boolean("OnlyVisual", false),
            boolean("FakeOnPressing", false),
            boolean("HideShieldSlot", false),
            boolean("ApplyToThirdPersonView", true),
            boolean("AlwaysHideShield", false),
        ],
    )
}

/// Upstream extension `LivingEntity.shouldApplySwordBlockAnimation`:
/// `if (this === player && fakeOnPressing && mc.options.keyUse.isPressedOnAny)
/// true; usingItemOrNull ?: false; (isInHand(OFF_HAND) && shield) ||
/// (isInHand(MAIN_HAND) && !isBlocksAttacksExisting)`.
pub fn should_apply_sword_block_animation(
    module: &Module,
    entity: &SwordBlockEntity,
    blocks_attacks_existing: bool,
) -> bool {
    if entity.is_player
        && setting_bool(module, "FakeOnPressing", false)
        && entity.key_use_pressed
    {
        return true;
    }

    let Some((using_item, hand)) = entity.using_item else {
        return false;
    };
    (hand == Hand::OffHand && using_item.is_shield())
        || (hand == Hand::MainHand && !blocks_attacks_existing)
}

/// Upstream `shouldAnimateSwordBlock(entity, mainHandItem = entity.mainHandItem)`:
/// whether the sword in the main hand of `entity` is drawn blocking — while
/// AutoBlock shows the block pose, or while SwordBlock draws the shield it
/// blocks with as the sword. `shows_block_pose` is upstream
/// `KillAuraAutoBlock.showsBlockPose`.
pub fn should_animate_sword_block(
    module: &Module,
    entity: &SwordBlockEntity,
    shows_block_pose: bool,
    blocks_attacks_existing: bool,
    main_hand_item: ItemStack,
) -> bool {
    if !main_hand_item.is_sword() {
        return false;
    }

    (shows_block_pose && entity.is_player)
        || (module.running
            && should_apply_sword_block_animation(module, entity, blocks_attacks_existing))
}

/// Upstream `shouldHideOffhand(offHandStack = player.offhandItem,
/// mainHandStack = player.mainHandItem)`.
pub fn should_hide_offhand(module: &Module, off_hand_stack: ItemStack, main_hand_stack: ItemStack) -> bool {
    if !module.running {
        return false;
    }

    if !off_hand_stack.is_shield() {
        return false;
    }

    main_hand_stack.is_sword() || setting_bool(module, "AlwaysHideShield", false)
}

/// What the upstream packet-handler does with a use-item packet. The
/// `sequenceHandler` coroutine side (`waitTicks(1)` +
/// `interaction.startPrediction`) stays engine-side; the action carries
/// exactly what upstream sends.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SwordBlockPacketAction {
    /// `waitTicks(1)` then `interaction.startPrediction(world) { sequence ->
    /// ServerboundUseItemPacket(InteractionHand.OFF_HAND, sequence, player.yRot,
    /// player.xRot) }`
    SendOffHandAfterPrediction,
    /// `event.cancelEvent()` then send
    /// `ServerboundUseItemPacket(InteractionHand.OFF_HAND, packet.sequence,
    /// player.yRot, player.xRot)`.
    CancelAndSendOffHandSameSequence { sequence: i32 },
}

/// Upstream `packetHandler` (`sequenceHandler<PacketEvent>`): swap the
/// main-hand sword use for an off-hand shield use, so ViaVersion shows 1.8
/// sword blocking on the old combat protocol.
pub fn sword_block_packet_handler(
    module: &Module,
    event: &mut PacketEvent,
    player: &SwordBlockEntity,
    blocks_attacks_existing: bool,
) -> Option<SwordBlockPacketAction> {
    if setting_bool(module, "OnlyVisual", false) {
        return None; // return@sequenceHandler
    }

    // If we are already on the old combat protocol or anything blockable protocol,
    // we don't need to do anything
    if blocks_attacks_existing {
        return None;
    }

    let packet = match event.packet {
        Packet::UseItem(p) => p,
        Packet::Other => return None,
    };

    if packet.hand == Hand::MainHand {
        let item_in_hand = player.main_hand_item; // or activeItem

        if item_in_hand.is_sword() && item_in_hand.use_animation != ItemUseAnimation::Block {
            let off_hand_item = player.off_hand_item;
            if !off_hand_item.is_shield() {
                // Until "now" we should get a shield from the server
                Some(SwordBlockPacketAction::SendOffHandAfterPrediction)
            } else {
                event.cancelled = true;
                // We use the old sequence
                Some(SwordBlockPacketAction::CancelAndSendOffHandSameSequence {
                    sequence: packet.sequence,
                })
            }
        } else {
            None
        }
    } else {
        None
    }
}

/// The `interaction.startPrediction` body: the packet the prediction sends once
/// its sequence is known.
pub fn sword_block_prediction_packet(
    sequence: i32,
    player: &SwordBlockEntity,
) -> ServerboundUseItemPacket {
    ServerboundUseItemPacket {
        hand: Hand::OffHand,
        sequence,
        y_rot: player.y_rot,
        x_rot: player.x_rot,
    }
}

#[cfg(test)]
mod tests3 {
    use super::*;

    fn stack(kind: ItemKind, anim: ItemUseAnimation) -> ItemStack {
        ItemStack { item_kind: kind, use_animation: anim }
    }

    fn sword(anim: ItemUseAnimation) -> ItemStack {
        stack(ItemKind::Sword, anim)
    }

    fn other() -> ItemStack {
        stack(ItemKind::Other, ItemUseAnimation::None)
    }

    fn entity(main: ItemStack, off: ItemStack) -> SwordBlockEntity {
        SwordBlockEntity {
            is_player: false,
            main_hand_item: main,
            off_hand_item: off,
            y_rot: 12.5,
            x_rot: -3.25,
            using_item: None,
            key_use_pressed: false,
        }
    }

    fn module() -> Module {
        sword_block()
    }

    #[test]
    fn no_shield_offhand_takes_over_with_fresh_prediction() {
        let m = module();
        let mut e = entity(sword(ItemUseAnimation::None), other());
        e.is_player = true;
        let mut ev = PacketEvent {
            packet: Packet::UseItem(ServerboundUseItemPacket {
                hand: Hand::MainHand,
                sequence: 7,
                y_rot: 1.0,
                x_rot: 2.0,
            }),
            cancelled: false,
        };
        let action = sword_block_packet_handler(&m, &mut ev, &e, false);
        assert_eq!(action, Some(SwordBlockPacketAction::SendOffHandAfterPrediction));
        assert!(!ev.cancelled);
    }

    #[test]
    fn prediction_packet_is_offhand_with_new_sequence_and_player_rotation() {
        let e = entity(sword(ItemUseAnimation::None), other());
        let p = sword_block_prediction_packet(99, &e);
        assert_eq!(p.hand, Hand::OffHand);
        assert_eq!(p.sequence, 99);
        assert_eq!(p.y_rot, 12.5);
        assert_eq!(p.x_rot, -3.25);
    }

    #[test]
    fn shield_offhand_cancels_and_reuses_the_old_sequence() {
        let m = module();
        let e = entity(sword(ItemUseAnimation::None), stack(ItemKind::Shield, ItemUseAnimation::Block));
        let mut ev = PacketEvent {
            packet: Packet::UseItem(ServerboundUseItemPacket {
                hand: Hand::MainHand,
                sequence: 42,
                y_rot: 0.0,
                x_rot: 0.0,
            }),
            cancelled: false,
        };
        let action = sword_block_packet_handler(&m, &mut ev, &e, false);
        assert_eq!(
            action,
            Some(SwordBlockPacketAction::CancelAndSendOffHandSameSequence { sequence: 42 })
        );
        assert!(ev.cancelled);
    }

    #[test]
    fn only_visual_and_blocks_attacks_short_circuit() {
        let mut m = module();
        m.setting_mut("OnlyVisual").unwrap().kind = ValueKind::Bool(true);
        let e = entity(sword(ItemUseAnimation::None), other());
        let mut ev = PacketEvent {
            packet: Packet::UseItem(ServerboundUseItemPacket {
                hand: Hand::MainHand,
                sequence: 1,
                y_rot: 0.0,
                x_rot: 0.0,
            }),
            cancelled: false,
        };
        assert_eq!(sword_block_packet_handler(&m, &mut ev, &e, false), None);

        m.setting_mut("OnlyVisual").unwrap().kind = ValueKind::Bool(false);
        assert_eq!(sword_block_packet_handler(&m, &mut ev, &e, true), None);
    }

    #[test]
    fn block_animation_or_offhand_use_passes_through() {
        let m = module();
        let e = entity(sword(ItemUseAnimation::Block), other());
        let mut ev = PacketEvent {
            packet: Packet::UseItem(ServerboundUseItemPacket {
                hand: Hand::MainHand,
                sequence: 3,
                y_rot: 0.0,
                x_rot: 0.0,
            }),
            cancelled: false,
        };
        assert_eq!(sword_block_packet_handler(&m, &mut ev, &e, false), None);

        let mut ev2 = PacketEvent {
            packet: Packet::UseItem(ServerboundUseItemPacket {
                hand: Hand::OffHand,
                sequence: 3,
                y_rot: 0.0,
                x_rot: 0.0,
            }),
            cancelled: false,
        };
        assert_eq!(sword_block_packet_handler(&m, &mut ev2, &e, false), None);
    }

    #[test]
    fn animate_sword_block_gates() {
        let m = module();

        // Not a sword in the main hand: never.
        assert!(!should_animate_sword_block(&m, &entity(other(), other()), false, false, other()));

        // KillAuraAutoBlock.showsBlockPose && entity === player.
        let mut e = entity(sword(ItemUseAnimation::None), other());
        e.is_player = true;
        assert!(should_animate_sword_block(&m, &e, true, false, e.main_hand_item));

        // running && shouldApplySwordBlockAnimation: off-hand shield in use.
        let mut m2 = module();
        m2.running = true;
        let mut e2 = entity(sword(ItemUseAnimation::None), other());
        e2.using_item = Some((stack(ItemKind::Shield, ItemUseAnimation::Block), Hand::OffHand));
        assert!(should_animate_sword_block(&m2, &e2, false, false, e2.main_hand_item));

        // Not running and no kill-aura block pose: false.
        assert!(!should_animate_sword_block(&m, &e, false, false, e.main_hand_item));

        // Main-hand use animates on a pre-blocks-attacks protocol, but not
        // once the protocol has blocks attacks itself.
        let mut e3 = entity(sword(ItemUseAnimation::None), other());
        e3.using_item = Some((sword(ItemUseAnimation::None), Hand::MainHand));
        assert!(should_animate_sword_block(&m2, &e3, false, false, e3.main_hand_item));
        assert!(!should_animate_sword_block(&m2, &e3, false, true, e3.main_hand_item));
    }

    #[test]
    fn fake_on_pressing_applies_to_the_player_only() {
        let mut m = module();
        m.setting_mut("FakeOnPressing").unwrap().kind = ValueKind::Bool(true);
        m.running = true;

        let mut e = entity(sword(ItemUseAnimation::None), other());
        e.is_player = true;
        assert!(!should_apply_sword_block_animation(&m, &e, false));
        e.key_use_pressed = true;
        assert!(should_apply_sword_block_animation(&m, &e, false));

        // Another entity with the same state: no fake-on-pressing shortcut.
        let mut e2 = e;
        e2.is_player = false;
        e2.using_item = None;
        assert!(!should_apply_sword_block_animation(&m, &e2, false));
    }

    #[test]
    fn hide_offhand_gates() {
        let mut m = module();

        // Not running: never.
        assert!(!should_hide_offhand(
            &m,
            stack(ItemKind::Shield, ItemUseAnimation::None),
            sword(ItemUseAnimation::None)
        ));

        m.running = true;

        // Off hand not a shield: never.
        assert!(!should_hide_offhand(&m, other(), sword(ItemUseAnimation::None)));

        // Shield off hand with a sword main hand.
        assert!(should_hide_offhand(
            &m,
            stack(ItemKind::Shield, ItemUseAnimation::None),
            sword(ItemUseAnimation::None)
        ));

        // AlwaysHideShield covers a non-sword main hand.
        m.setting_mut("AlwaysHideShield").unwrap().kind = ValueKind::Bool(true);
        assert!(should_hide_offhand(
            &m,
            stack(ItemKind::Shield, ItemUseAnimation::None),
            other()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Json;

    #[test]
    fn no_miss_cooldown_defaults_match_upstream() {
        let m = no_miss_cooldown();
        assert_eq!(m.name, "NoMissCooldown");
        assert!(matches!(m.setting("RemoveAttackCooldown").unwrap().kind, ValueKind::Bool(true)));
        assert!(matches!(m.setting("CancelAttackOnMiss").unwrap().kind, ValueKind::Bool(false)));
        assert_eq!(m.category.tag, "combat");
    }

    #[test]
    fn hitbox_margin_applies_only_to_attackable_entities() {
        let m = hitbox();
        let mut ev = EntityMarginEvent { entity_should_be_attacked: true, margin: 0.0 };
        hitbox_margin_handler(&m, &mut ev);
        assert_eq!(ev.margin, 0.1);
        ev = EntityMarginEvent { entity_should_be_attacked: false, margin: 0.3 };
        hitbox_margin_handler(&m, &mut ev);
        assert_eq!(ev.margin, 0.3);
    }

    #[test]
    fn chance_gate_matches_percentage_semantics() {
        assert!(!chance_is_hit(0.0, || 50.0));
        assert!(chance_is_hit(100.0, || 0.0));
        assert!(chance_is_hit(50.0, || 49.9));
        assert!(!chance_is_hit(50.0, || 50.1));
    }

    #[test]
    fn keep_sprint_motion_falls_back_below_chance() {
        let ks = keep_sprint();
        let p = KeepSprintPlayer { is_sprinting: true, hurt_time: 0 };
        // Chance 100 always hits; HurtTime 0 is outside 1..10, so Motion.
        let m = ks.get_motion(&p, || 0.5, || 99.0);
        assert!((m - 1.0).abs() < 1e-9);
    }

    #[test]
    fn keep_sprint_motion_when_hurt_uses_hurt_range() {
        let ks = keep_sprint();
        let p = KeepSprintPlayer { is_sprinting: false, hurt_time: 5 };
        let m = ks.get_motion(&p, || 0.25, || 100.0);
        assert!((m - 1.0).abs() < 1e-9);
    }

    #[test]
    fn keep_sprint_motion_at_zero_chance_is_vanilla() {
        let mut ks = keep_sprint();
        ks.module
            .setting_mut("Chance")
            .unwrap()
            .set_from_json(&Json::Num(0.0))
            .unwrap();
        let p = KeepSprintPlayer { is_sprinting: false, hurt_time: 5 };
        assert_eq!(ks.get_motion(&p, || 0.0, || 0.0), 0.6);
    }

    #[test]
    fn post_tick_copies_sprinting_state() {
        let mut ks = keep_sprint();
        ks.on_player_post_tick(&KeepSprintPlayer { is_sprinting: true, hurt_time: 0 });
        assert!(ks.sprinting);
    }

    #[test]
    fn range_values_round_trip_through_range_adapter_shape() {
        let mut v = float_range("Motion", 100.0, 100.0, 0.0, 100.0);
        v.set_from_json(&crate::value::range_json(60.0, 80.0)).unwrap();
        assert_eq!(v.kind, ValueKind::FloatRange(60.0, 80.0, 0.0, 100.0));
        // Out-of-range input clamps like upstream deserializers.
        v.set_from_json(&crate::value::range_json(-5.0, 900.0)).unwrap();
        assert_eq!(v.kind, ValueKind::FloatRange(0.0, 100.0, 0.0, 100.0));
    }
}

#[cfg(test)]
mod tests2 {
    use super::*;

    fn low_player() -> AutoLeavePlayer {
        AutoLeavePlayer { instabuild: false, singleplayer: false, can_heal_death_protection: false, health: 4.0 }
    }

    #[test]
    fn auto_leave_passes_after_delay_ticks() {
        let mut al = auto_leave();
        // Delay 0..0 draws 0: passes immediately when breakLoop is false.
        let r = al.on_tick(&low_player(), || (0, 0));
        assert_eq!(r, Some(KickMode::Quit));
        assert!(!al.module.running, "enabled = false after the kick");
        // Delay 0..2: draws a threshold, then counts without breaking.
        let mut al = auto_leave();
        al.module.setting_mut("Delay").unwrap().set_from_json(&crate::value::range_json(2.0, 2.0)).unwrap();
        let r = al.on_tick(&low_player(), || (2, 2));
        assert_eq!(r, None, "first quiet tick keeps counting");
        let r = al.on_tick(&low_player(), || (2, 2));
        assert_eq!(r, Some(KickMode::Quit));
    }

    #[test]
    fn auto_leave_breaks_on_heal_or_high_health() {
        let mut al = auto_leave();
        let heal = AutoLeavePlayer { instabuild: false, singleplayer: false, can_heal_death_protection: true, health: 4.0 };
        assert_eq!(al.on_tick(&heal, || (0, 0)), None);
        let healthy = AutoLeavePlayer { instabuild: false, singleplayer: false, can_heal_death_protection: false, health: 9.0 };
        assert_eq!(al.on_tick(&healthy, || (0, 0)), None, "health >= threshold breaks");
        let mut al = auto_leave();
        let creative = AutoLeavePlayer { instabuild: true, singleplayer: false, can_heal_death_protection: false, health: 4.0 };
        assert_eq!(al.on_tick(&creative, || (0, 0)), None);
        let mut al = auto_leave();
        let single = AutoLeavePlayer { instabuild: false, singleplayer: true, can_heal_death_protection: false, health: 4.0 };
        assert_eq!(al.on_tick(&single, || (0, 0)), None);
    }

    #[test]
    fn auto_leave_mode_maps_to_kick_enum() {
        let mut al = auto_leave();
        al.module.setting_mut("Mode").unwrap().set_from_json(&crate::value::Json::Str("SelfHurt".into())).unwrap();
        let r = al.on_tick(&low_player(), || (0, 0));
        assert_eq!(r, Some(KickMode::SelfHurt));
    }

    #[test]
    fn mace_kill_ignores_non_mace_hands() {
        let m = mace_kill();
        let p = MaceKillPlayer { holding_mace: false, on_ground: true, collides_at: Box::new(|_| false) };
        assert_eq!(mace_kill_attack_handler(&m, &p), None);
    }

    #[test]
    fn mace_kill_low_height_warp_sequence() {
        let m = mace_kill();
        // Free space up to 4 blocks, collision above.
        let p = MaceKillPlayer { holding_mace: true, on_ground: true, collides_at: Box::new(|o| o > 4.0) };
        let calls = mace_kill_attack_handler(&m, &p).unwrap();
        assert_eq!(calls, vec![
            WarpCall { target_dy: None, on_ground: true },
            WarpCall { target_dy: None, on_ground: true },
            WarpCall { target_dy: Some(4.0), on_ground: false },
            WarpCall { target_dy: Some(0.0), on_ground: false },
        ]);
    }

    #[test]
    fn mace_kill_high_height_repeats_ten_block_warps() {
        let m = mace_kill();
        // Free space up to 35 blocks: ceil(35/10) = 4 pre-warps.
        let mut m = m;
        m.setting_mut("FallHeight").unwrap().set_from_json(&crate::value::Json::Num(35.0)).unwrap();
        let p = MaceKillPlayer { holding_mace: true, on_ground: false, collides_at: Box::new(|o| o > 35.0) };
        let calls = mace_kill_attack_handler(&m, &p).unwrap();
        assert_eq!(calls.len(), 4 + 2);
        assert!(calls[..4].iter().all(|c| *c == WarpCall { target_dy: None, on_ground: false }));
        assert_eq!(calls[4], WarpCall { target_dy: Some(35.0), on_ground: false });
        assert_eq!(calls[5], WarpCall { target_dy: Some(0.0), on_ground: false });
    }

    #[test]
    fn mace_kill_fully_blocked_height_is_zero() {
        let m = mace_kill();
        let p = MaceKillPlayer { holding_mace: true, on_ground: true, collides_at: Box::new(|_| true) };
        let calls = mace_kill_attack_handler(&m, &p).unwrap();
        assert_eq!(calls[2], WarpCall { target_dy: Some(0.0), on_ground: false });
    }
}
