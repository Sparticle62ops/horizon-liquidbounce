//! Port of `features/module/modules/combat/*` (slice 2 start): the small
//! top-level combat modules. Game-facing calls stay behind the same seams
//! upstream uses (events, `player`, `shouldBeAttacked`), injected so the
//! logic itself stays exact.

use crate::category::COMBAT;
use crate::module::Module;
use crate::value::{boolean, choice, float, float_range, int, int_range, items, multi_choice, Json, ValueKind};

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

// ----- Seams: blink + packets + ModuleFakeLag.kt -----

/// Upstream `net.minecraft.world.phys.Vec3` (the fields the module reads).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl std::ops::Sub for Vec3 {
    type Output = Vec3;

    fn sub(self, other: Vec3) -> Vec3 {
        Vec3 { x: self.x - other.x, y: self.y - other.y, z: self.z - other.z }
    }
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };

    pub fn dot(&self, other: Vec3) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn distance_to(&self, other: Vec3) -> f64 {
        ((self.x - other.x).powi(2)
            + (self.y - other.y).powi(2)
            + (self.z - other.z).powi(2))
            .sqrt()
    }
}

/// Two AABBs (min, max) intersect: every axis overlaps (`AABB.intersects`).
fn box_intersects(a_min: Vec3, a_max: Vec3, b_min: Vec3, b_max: Vec3) -> bool {
    a_min.x < b_max.x
        && a_max.x > b_min.x
        && a_min.y < b_max.y
        && a_max.y > b_min.y
        && a_min.z < b_max.z
        && a_max.z > b_min.z
}

/// Upstream `TransferOrigin`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransferOrigin {
    Outgoing,
    Incoming,
}

/// Upstream `BlinkManager.Action` (the variant this module sets).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlinkAction {
    Queue,
}

/// The packets FakeLag's logic distinguishes, flattened to their
/// distinguishing fields (seam for `Packet<*>`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FakeLagPacket {
    Interact,
    Attack,
    SpectatorAction,
    Punch,
    UseItemOn,
    SignUpdate,
    PlayerAction,
    ServerboundResourcePack,
    ClientboundPlayerPosition,
    SetEntityMotion { id: i64, movement: Vec3 },
    Explode { player_knockback: Option<Vec3> },
    SetHealth,
    Other,
}

/// Upstream event `BlinkPacketEvent`.
pub struct BlinkPacketEvent {
    pub origin: TransferOrigin,
    pub packet: FakeLagPacket,
    /// Upstream `BlinkManager.Action?`; None keeps the event unset.
    pub action: Option<BlinkAction>,
}

/// Upstream `Chronometer` (utils/client/Chronometer.kt) with the clock
/// injected: `reset()` records now, `hasAtLeastElapsed(ms)` is
/// `now - lastReset >= ms`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Chronometer {
    last_reset_ms: u64,
}

impl Chronometer {
    pub fn reset(&mut self, now_ms: u64) {
        self.last_reset_ms = now_ms;
    }

    pub fn has_at_least_elapsed(&self, ms: u64, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_reset_ms) >= ms
    }
}

/// Upstream `EvadingPacket` (ModuleAutoDodge seam).
#[derive(Clone, Copy, Debug)]
pub struct EvadingPacket {
    pub idx: usize,
    pub ticks_to_impact: Option<u64>,
}

/// Upstream `ModuleAutoDodge` surface FakeLag reads.
pub trait FakeLagAutoDodge {
    fn enabled(&self) -> bool;
    /// `getInflictedHit(position)` — None = no inflicted hit.
    fn get_inflicted_hit(&self, position: Vec3) -> Option<()>;
    fn find_avoiding_arrow_position(&self) -> Option<EvadingPacket>;
}

/// Upstream `BlinkManager` surface FakeLag reads (`positions`, `isAboveTime`,
/// `flush`). `now_ms` stands in for `System.currentTimeMillis()`.
pub trait FakeLagBlink {
    fn positions(&self) -> &[Vec3];
    /// `isAboveTime(delay)`: oldest queued packet's age is at least `delay`.
    fn is_above_time(&self, delay: u64) -> bool;
    fn flush_outgoing(&mut self);
    fn flush_up_to(&mut self, idx: usize);
}

/// An entity snapshot as read by `getEntitiesBoxInRange`.
#[derive(Clone, Copy, Debug)]
pub struct FakeLagEntity {
    pub position: Vec3,
    /// Entity bounding box (only `intersects` is read).
    pub box_min: Vec3,
    pub box_max: Vec3,
    /// Upstream `it != player && it.shouldBeAttacked()` was already applied.
    pub matches_predicate: bool,
}

/// Upstream world surface FakeLag reads.
pub trait FakeLagWorld {
    /// `world.findEnemy(range)`: any attackable enemy in the range.
    fn find_enemy(&self, min_range: f32, max_range: f32) -> bool;
    /// `world.getEntitiesBoxInRange(position, range) { it != player && it.shouldBeAttacked() }`.
    fn entities_box_in_range(&self, mid_pos: Vec3, range: f64) -> Vec<FakeLagEntity>;
    /// `player.dimensions.makeBoundingBox(position)` as (min, max).
    fn make_player_box(&self, position: Vec3) -> (Vec3, Vec3);
}

/// The slice of the upstream `player` this module reads.
#[derive(Clone, Copy, Debug)]
pub struct FakeLagPlayer {
    pub id: i64,
    pub position: Vec3,
    pub is_dead_or_dying: bool,
    pub is_in_water: bool,
    pub is_using_item: bool,
    /// `player.useItem.isConsumable` (has the CONSUMABLE component).
    pub use_item_is_consumable: bool,
}

/// Upstream `FlushOn` tags with their `testPacket` predicates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlushOn {
    EntityInteract,
    BlockInteract,
    Action,
}

impl FlushOn {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::EntityInteract => "EntityInteract",
            Self::BlockInteract => "BlockInteract",
            Self::Action => "Action",
        }
    }

    /// Upstream `testPacket`.
    pub fn test(&self, packet: &FakeLagPacket) -> bool {
        match self {
            Self::EntityInteract => matches!(
                packet,
                FakeLagPacket::Interact
                    | FakeLagPacket::Attack
                    | FakeLagPacket::SpectatorAction
                    | FakeLagPacket::Punch
            ),
            Self::BlockInteract => {
                matches!(packet, FakeLagPacket::UseItemOn | FakeLagPacket::SignUpdate)
            }
            Self::Action => matches!(packet, FakeLagPacket::PlayerAction),
        }
    }
}

/// Upstream `Mode` tags.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FakeLagMode {
    Constant,
    Dynamic,
}

fn fake_lag_mode(module: &Module) -> Option<FakeLagMode> {
    match module.setting("Mode").map(|v| &v.kind) {
        Some(ValueKind::Choice(sel, _)) if sel == "Constant" => Some(FakeLagMode::Constant),
        Some(ValueKind::Choice(sel, _)) if sel == "Dynamic" => Some(FakeLagMode::Dynamic),
        _ => None,
    }
}

fn flush_on_selected(module: &Module) -> Vec<FlushOn> {
    match module.setting("FlushOn").map(|v| &v.kind) {
        Some(ValueKind::MultiChoice(sel, _)) => sel
            .iter()
            .filter_map(|t| match t.as_str() {
                "EntityInteract" => Some(FlushOn::EntityInteract),
                "BlockInteract" => Some(FlushOn::BlockInteract),
                "Action" => Some(FlushOn::Action),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Upstream `FlushOn.ENTITY_INTERACT in flushOn` etc.
fn flush_on_contains(module: &Module, entry: FlushOn) -> bool {
    flush_on_selected(module).contains(&entry)
}

/// Upstream `flushOn.matchesAny(packet)`: any selected entry's predicate
/// tests the packet.
fn flush_on_matches_any(module: &Module, packet: &FakeLagPacket) -> bool {
    flush_on_selected(module).iter().any(|e| e.test(packet))
}

fn setting_int(module: &Module, name: &str, default: i64) -> i64 {
    match module.setting(name).map(|v| &v.kind) {
        Some(ValueKind::Int(v, _, _)) => *v,
        _ => default,
    }
}

fn setting_float_range(module: &Module, name: &str, default: (f64, f64)) -> (f64, f64) {
    module.setting(name).and_then(|v| v.kind.float_range()).unwrap_or(default)
}

fn setting_int_range(module: &Module, name: &str, default: (i64, i64)) -> (i64, i64) {
    module.setting(name).and_then(|v| v.kind.int_range()).unwrap_or(default)
}

/// Upstream: `ModuleFakeLag` — holds back packets to prevent you from being
/// hit by an enemy.
pub fn fake_lag() -> Module {
    Module::new(
        "FakeLag",
        COMBAT,
        vec![
            float_range("Range", 2.0, 5.0, 0.0, 10.0),
            int_range("Delay", 300, 600, 0, 1000), // "ms"
            int("RecoilTime", 250, 0, 1000),       // "ms"
            choice("Mode", "Dynamic", &["Constant", "Dynamic"]), // tagBy(this)
            multi_choice(
                "FlushOn",
                &["EntityInteract", "BlockInteract", "Action"],
                &["EntityInteract", "BlockInteract", "Action"],
            ),
        ],
    )
}

/// Upstream module state: `nextDelay`, `chronometer`, `isEnemyNearby`.
#[derive(Clone, Copy, Debug)]
pub struct FakeLagState {
    /// Upstream `var nextDelay = delay.random()`.
    pub next_delay: i64,
    pub chronometer: Chronometer,
    pub is_enemy_nearby: bool,
}

pub fn fake_lag_state(initial_next_delay: i64) -> FakeLagState {
    FakeLagState {
        next_delay: initial_next_delay,
        chronometer: Chronometer::default(),
        is_enemy_nearby: false,
    }
}

pub struct FakeLagHooks<'a, N: FnMut(String, String)> {
    pub world: &'a mut dyn FakeLagWorld,
    pub blink: &'a mut dyn FakeLagBlink,
    pub auto_dodge: &'a mut dyn FakeLagAutoDodge,
    /// Upstream `notification(title, message, NotificationEvent.Severity.INFO)`.
    pub notify: &'a mut N,
    /// Upstream `ModuleAutoShoot.constantLag`.
    pub auto_shoot_constant_lag: bool,
    /// Upstream `ModuleAutoShoot.targetTracker.target == null`.
    pub auto_shoot_has_target: bool,
    /// Upstream `mc.gui.screen() != null`.
    pub screen_open: bool,
}

/// Upstream `gameTickHandler` (`handler<GameTickEvent>`): track whether an
/// enemy is nearby, and let AutoDodge pick the blink flush when enabled.
pub fn fake_lag_game_tick<N: FnMut(String, String)>(
    module: &Module,
    state: &mut FakeLagState,
    hooks: &mut FakeLagHooks<N>,
) {
    let (from, to) = setting_float_range(module, "Range", (2.0, 5.0));
    state.is_enemy_nearby = hooks.world.find_enemy(from as f32, to as f32);

    if !hooks.auto_dodge.enabled() {
        return;
    }

    let Some(&position) = hooks.blink.positions().first() else {
        return;
    };

    if hooks.auto_dodge.get_inflicted_hit(position).is_none() {
        return;
    }

    let evading = hooks.auto_dodge.find_avoiding_arrow_position();

    // We have found no packet that avoids getting hit? Then we default to blinking.
    // AutoDoge might save the situation...
    let Some(evading_packet) = evading else {
        (hooks.notify)(String::from("FakeLag"), String::from("Unable to evade arrow. Blinking."));
        hooks.blink.flush_outgoing();
        return;
    };

    if evading_packet.ticks_to_impact.is_some() {
        (hooks.notify)(String::from("FakeLag"), String::from("Trying to evade arrow..."));
    } else {
        (hooks.notify)(String::from("FakeLag"), String::from("Arrow evaded."));
    }
    hooks.blink.flush_up_to(evading_packet.idx + 1);
}

/// Upstream `fakeLagHandler` (`handler<BlinkPacketEvent>`). `random_delay`
/// stands in for `delay.random()`.
pub fn fake_lag_blink_packet_handler<N: FnMut(String, String)>(
    module: &Module,
    state: &mut FakeLagState,
    event: &mut BlinkPacketEvent,
    player: &FakeLagPlayer,
    hooks: &mut FakeLagHooks<N>,
    now_ms: u64,
    random_delay: fn(i64, i64) -> i64,
) {
    if event.origin != TransferOrigin::Outgoing
        || player.is_dead_or_dying
        || player.is_in_water
        || hooks.screen_open
    {
        return;
    }

    if !state
        .chronometer
        .has_at_least_elapsed(setting_int(module, "RecoilTime", 250) as u64, now_ms)
    {
        return;
    }

    if hooks.blink.is_above_time(state.next_delay.max(0) as u64) {
        let (lo, hi) = setting_int_range(module, "Delay", (300, 600));
        state.next_delay = random_delay(lo, hi);
        return;
    }

    if flush_on_matches_any(module, &event.packet) {
        state.chronometer.reset(now_ms);
        return;
    }

    match event.packet {
        FakeLagPacket::ClientboundPlayerPosition | FakeLagPacket::ServerboundResourcePack => {
            state.chronometer.reset(now_ms);
            return;
        }

        FakeLagPacket::Interact
        | FakeLagPacket::Attack
        | FakeLagPacket::SpectatorAction
        | FakeLagPacket::Punch => {
            if flush_on_contains(module, FlushOn::EntityInteract) {
                state.chronometer.reset(now_ms);
                return;
            }
        }

        // Flush on knockback
        FakeLagPacket::SetEntityMotion { id, movement } => {
            if id == player.id && movement != Vec3::ZERO {
                state.chronometer.reset(now_ms);
                return;
            }
        }

        // Flush on explosion
        FakeLagPacket::Explode { player_knockback } => {
            if let Some(knockback) = player_knockback {
                if knockback != Vec3::ZERO {
                    state.chronometer.reset(now_ms);
                    return;
                }
            }
        }

        // Flush on damage
        FakeLagPacket::SetHealth => {
            state.chronometer.reset(now_ms);
            return;
        }
        _ => {}
    }

    // We don't want to lag when we are using an item that is not a food, milk bucket or potion.
    if player.is_using_item && player.use_item_is_consumable {
        return;
    }

    // Support auto shoot with fake lag
    if module.running && hooks.auto_shoot_constant_lag && !hooks.auto_shoot_has_target {
        event.action = Some(BlinkAction::Queue);
        return;
    }

    match fake_lag_mode(module).unwrap_or(FakeLagMode::Dynamic) {
        FakeLagMode::Constant => event.action = Some(BlinkAction::Queue),
        FakeLagMode::Dynamic => {
            // If there is an enemy in range, we want to lag.
            if !state.is_enemy_nearby {
                return;
            }

            let Some(&position) = hooks.blink.positions().first() else {
                event.action = Some(BlinkAction::Queue);
                return;
            };
            let (player_box_min, player_box_max) = hooks.world.make_player_box(position);

            // todo: implement if enemy is facing old player position

            let (from, to) = setting_float_range(module, "Range", (2.0, 5.0));
            let entities = hooks.world.entities_box_in_range(position, to);

            // If there are no entities, we don't want to lag.
            if entities.is_empty() {
                return;
            }

            let intersects = entities
                .iter()
                .any(|e| box_intersects(e.box_min, e.box_max, player_box_min, player_box_max));
            let server_distance = entities
                .iter()
                .map(|e| e.position.distance_to(position))
                .fold(f64::INFINITY, f64::min);
            let client_distance = entities
                .iter()
                .map(|e| e.position.distance_to(player.position))
                .fold(f64::INFINITY, f64::min);

            // If the server position is not closer than the client position, we keep lagging.
            // Also, we don't want to lag if the player is intersecting with an entity.
            if server_distance < client_distance || intersects {
                return;
            }

            event.action = Some(BlinkAction::Queue);
        }
    }
}

/// Upstream `onDisabled`: `isEnemyNearby = false`.
pub fn fake_lag_on_disabled(state: &mut FakeLagState) {
    state.is_enemy_nearby = false;
}
// ----- Seams + ModuleSuperKnockback.kt -----

/// Upstream `AttackEntityEvent` flattened to the attacked entity snapshot.
#[derive(Clone, Copy, Debug)]
pub struct SkTarget {
    /// Upstream `enemy is LivingEntity`.
    pub is_living_entity: bool,
    /// `enemy.hurtTime` (0 when the target is not a living entity).
    pub hurt_time: i64,
    /// `enemy.lookAngle`.
    pub look_angle: Vec3,
    /// `enemy.position()`.
    pub position: Vec3,
}

/// The slice of the upstream `player` SuperKnockback reads.
#[derive(Clone, Copy, Debug)]
pub struct SkPlayer {
    pub position: Vec3,
    pub on_ground: bool,
    pub is_inside_water_or_bubble_column: bool,
    pub is_sprinting: bool,
    pub was_sprinting: bool,
    /// `player.input.movementForward`.
    pub movement_forward: f32,
    /// `player.input.movementSideways`.
    pub movement_sideways: f32,
    /// `player.input.hasForwardImpulse()`.
    pub has_forward_impulse: bool,
}

/// Upstream `network.sendStartSprinting()/sendStopSprinting()` seam.
pub trait SkNetwork {
    fn send_start_sprinting(&mut self);
    fn send_stop_sprinting(&mut self);
}

pub struct SkHooks<'a> {
    /// Upstream `ModuleCriticals.wouldDoCriticalHit()`.
    pub criticals_would_hit: bool,
    /// Upstream `debugParameter("State") { ... }` labels, in order.
    pub debug: &'a mut Vec<String>,
    pub network: &'a mut dyn SkNetwork,
}

fn sk_debug(hooks: &mut SkHooks, label: &str) {
    hooks.debug.push(label.to_string());
}

/// Upstream `SprintEvent.Source`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SprintEventSource {
    MovementTick,
    Input,
    Other,
}

pub struct SprintEvent {
    pub source: SprintEventSource,
    pub sprint: bool,
}

/// Upstream `DirectionalInput` (only `NONE` is written by this module).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DirectionalInput {
    None,
    Other,
}

pub struct MovementInputEvent {
    pub directional_input: DirectionalInput,
}

/// Upstream `Mode` tags.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SkMode {
    Packet,
    SprintTap,
    WTap,
}

fn sk_mode(module: &Module) -> Option<SkMode> {
    match module.setting("Mode").map(|v| &v.kind) {
        Some(ValueKind::Choice(sel, _)) if sel == "Packet" => Some(SkMode::Packet),
        Some(ValueKind::Choice(sel, _)) if sel == "SprintTap" => Some(SkMode::SprintTap),
        Some(ValueKind::Choice(sel, _)) if sel == "WTap" => Some(SkMode::WTap),
        _ => None,
    }
}

/// Upstream `Conditions` tags with their predicates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SkCondition {
    OnlyFacing,
    OnlyOnGround,
    NotInWater,
}

impl SkCondition {
    fn tag(&self) -> &'static str {
        match self {
            Self::OnlyFacing => "OnlyFacing",
            Self::OnlyOnGround => "OnlyOnGround",
            Self::NotInWater => "NotInWater",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "OnlyFacing" => Some(Self::OnlyFacing),
            "OnlyOnGround" => Some(Self::OnlyOnGround),
            "NotInWater" => Some(Self::NotInWater),
            _ => None,
        }
    }
}

fn sk_conditions_selected(module: &Module) -> Vec<SkCondition> {
    match module.setting("Conditions").map(|v| &v.kind) {
        Some(ValueKind::MultiChoice(sel, _)) => sel.iter().filter_map(|t| SkCondition::from_tag(t)).collect(),
        _ => Vec::new(),
    }
}

/// Upstream `Conditions.matchesAll(target)`: every selected predicate tests true.
pub fn sk_conditions_match_all(module: &Module, player: &SkPlayer, target: &SkTarget) -> bool {
    sk_conditions_selected(module).iter().all(|c| match c {
        SkCondition::OnlyFacing => {
            target.look_angle.dot(player.position - target.position) >= 0.0
        }
        SkCondition::OnlyOnGround => player.on_ground,
        SkCondition::NotInWater => !player.is_inside_water_or_bubble_column,
    })
}

fn sk_hurt_time(module: &Module) -> i64 {
    match module.setting("HurtTime").map(|v| &v.kind) {
        Some(ValueKind::Int(v, _, _)) => *v,
        _ => 10,
    }
}

/// Upstream `percentageChance("Chance", 100f)`'s `asBoolean`: 0 -> false,
/// 100 -> true, else `random.nextFloat(100) < percentage`. `roll` stands in
/// for the random draw in 0..100.
pub fn sk_chance_as_boolean(module: &Module, roll: f64) -> bool {
    let v = match module.setting("Chance").map(|x| &x.kind) {
        Some(ValueKind::Float(f, _, _)) => *f,
        _ => 100.0,
    };
    if v == 0.0 {
        false
    } else if v == 100.0 {
        true
    } else {
        roll < v
    }
}

fn sk_only_on_move_enabled(module: &Module) -> bool {
    match module.setting("OnlyOnMove").map(|v| &v.kind) {
        Some(ValueKind::Bool(b)) => *b,
        _ => true,
    }
}

fn sk_only_forward(module: &Module) -> bool {
    match module.setting("OnlyForward").map(|v| &v.kind) {
        Some(ValueKind::Bool(b)) => *b,
        _ => true,
    }
}

/// Upstream `shouldOperate(target)`.
pub fn sk_should_operate(module: &Module, player: &SkPlayer, target: &SkTarget) -> bool {
    if sk_only_on_move_enabled(module) {
        let is_moving_sideways = player.movement_sideways != 0.0;
        let is_moving = player.movement_forward != 0.0 || is_moving_sideways;

        if !is_moving || (sk_only_forward(module) && is_moving_sideways) {
            return false;
        }
    }

    sk_conditions_match_all(module, player, target)
}

/// Upstream `shouldStopSprinting(event)`.
pub fn sk_should_stop_sprinting(
    module: &Module,
    player: &SkPlayer,
    target: &SkTarget,
    criticals_would_hit: bool,
    chance_roll: f64,
) -> bool {
    if !player.is_sprinting || !player.was_sprinting {
        return false;
    }

    target.is_living_entity
        && target.hurt_time <= sk_hurt_time(module)
        && sk_chance_as_boolean(module, chance_roll)
        && !criticals_would_hit
}

/// Upstream: `ModuleSuperKnockback` — increases knockback dealt to other
/// entities (alias `WTap`). The `Mode` choice groups Packet/SprintTap/WTap;
/// `OnlyOnMove` is a toggleable group carried as its enabled + OnlyForward
/// booleans.
pub fn super_knockback() -> Module {
    Module::new(
        "SuperKnockback",
        COMBAT,
        vec![
            choice("Mode", "Packet", &["Packet", "SprintTap", "WTap"]), // tagBy(this)
            int("HurtTime", 10, 0, 10),
            float("Chance", 100.0, 0.0, 100.0), // "%"
            multi_choice(
                "Conditions",
                &["NotInWater"],
                &["OnlyFacing", "OnlyOnGround", "NotInWater"],
            ),
            boolean("OnlyOnMove", true),
            boolean("OnlyForward", true),
        ],
    )
}

/// Upstream SprintTap's `cancelSprint` + sequence, WTap's `inSequence` /
/// `cancelMovement` + sequence. The `sequenceHandler` coroutines are carried
/// as per-tick phase machines (`waitTicks`/`tickUntil`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SprintTapPhase {
    /// `tickUntil { !player.isSprinting && !player.wasSprinting }`.
    WaitStopSprinting,
    /// `waitTicks(reSprintTicks.random())`; the count is ticks left.
    WaitReSprint(u64),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WTapPhase {
    /// `waitTicks(ticksUntilMovementBlock.random())`; the count is ticks left.
    WaitMovementBlock(u64),
    /// `tickUntil { !player.input.hasForwardImpulse() }`.
    WaitForwardImpulseClear,
    /// `waitTicks(ticksUntilAllowedMovement.random())`; the count is ticks left.
    WaitAllowedMovement(u64),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SuperKnockbackState {
    pub cancel_sprint: bool,
    pub sprint_tap: Option<SprintTapPhase>,
    /// `reSprintTicks.random()` pick held for the WaitReSprint hand-off.
    pub re_sprint_ticks: u64,
    pub in_sequence: bool,
    pub cancel_movement: bool,
    pub w_tap: Option<WTapPhase>,
    /// `ticksUntilAllowedMovement.random()` pick held for the
    /// WaitAllowedMovement hand-off.
    pub until_allowed_movement_ticks: u64,
}

pub fn super_knockback_state() -> SuperKnockbackState {
    SuperKnockbackState::default()
}

/// Upstream Packet mode's `attackHandler` (`handler<AttackEntityEvent>`).
pub fn sk_packet_attack_handler(
    module: &Module,
    player: &mut SkPlayer,
    target: &SkTarget,
    hooks: &mut SkHooks,
    chance_roll: f64,
) {
    if !sk_should_operate(module, player, target) {
        return;
    }

    if target.is_living_entity
        && target.hurt_time <= sk_hurt_time(module)
        && sk_chance_as_boolean(module, chance_roll)
        && !hooks.criticals_would_hit
    {
        if player.is_sprinting {
            hooks.network.send_stop_sprinting();
        }

        hooks.network.send_start_sprinting();
        hooks.network.send_stop_sprinting();
        hooks.network.send_start_sprinting();

        player.is_sprinting = true;
        player.was_sprinting = true;
    }
}

/// Upstream SprintTap's `attackHandler` (`sequenceHandler<AttackEntityEvent>`),
/// start of the sequence. `re_sprint` stands in for `reSprintTicks.random()`.
pub fn sk_sprint_tap_attack(
    module: &Module,
    state: &mut SuperKnockbackState,
    player: &SkPlayer,
    target: &SkTarget,
    hooks: &mut SkHooks,
    chance_roll: f64,
    re_sprint: u64,
) {
    if !sk_should_operate(module, player, target)
        || !sk_should_stop_sprinting(
            module,
            player,
            target,
            hooks.criticals_would_hit,
            chance_roll,
        )
        || state.cancel_sprint
    {
        return;
    }

    sk_debug(hooks, "Disallowing Sprint");
    state.cancel_sprint = true;
    state.sprint_tap = Some(SprintTapPhase::WaitStopSprinting);
    state.re_sprint_ticks = re_sprint;
}

/// Upstream WTap's `attackHandler` (`sequenceHandler<AttackEntityEvent>`),
/// start of the sequence. `until_block` / `until_allowed` stand in for the
/// random picks of `ticksUntilMovementBlock` / `ticksUntilAllowedMovement`.
pub fn sk_w_tap_attack(
    module: &Module,
    state: &mut SuperKnockbackState,
    player: &SkPlayer,
    target: &SkTarget,
    hooks: &mut SkHooks,
    chance_roll: f64,
    until_block: u64,
    until_allowed: u64,
) {
    if !sk_should_operate(module, player, target)
        || !sk_should_stop_sprinting(
            module,
            player,
            target,
            hooks.criticals_would_hit,
            chance_roll,
        )
        || state.in_sequence
    {
        return;
    }

    state.in_sequence = true;
    sk_debug(hooks, "Waiting for Movement Block");
    state.w_tap = Some(WTapPhase::WaitMovementBlock(until_block));
    state.until_allowed_movement_ticks = until_allowed;
}

/// Upstream SprintTap's `movementHandler` (`handler<SprintEvent>`,
/// CRITICAL_MODIFICATION priority).
pub fn sk_sprint_handler(state: &SuperKnockbackState, event: &mut SprintEvent) {
    if state.cancel_sprint
        && (event.source == SprintEventSource::MovementTick
            || event.source == SprintEventSource::Input)
    {
        event.sprint = false;
    }
}

/// Upstream WTap's `movementHandler` (`handler<MovementInputEvent>`).
pub fn sk_movement_input_handler(state: &SuperKnockbackState, event: &mut MovementInputEvent) {
    if state.in_sequence && state.cancel_movement {
        event.directional_input = DirectionalInput::None;
    }
}

/// Upstream sequence continuation (`tickUntil`/`waitTicks` bodies), driven
/// once per game tick while a sequence is open.
pub fn sk_tick(state: &mut SuperKnockbackState, player: &SkPlayer, hooks: &mut SkHooks) {
    if let Some(phase) = state.sprint_tap {
        match phase {
            SprintTapPhase::WaitStopSprinting => {
                // Upstream `tickUntil { val player = mc.player ?: return@tickUntil true; ... }`.
                if !player.is_sprinting && !player.was_sprinting {
                    sk_debug(hooks, "Waiting for ReSprint");
                    state.sprint_tap = Some(SprintTapPhase::WaitReSprint(state.re_sprint_ticks));
                }
            }
            SprintTapPhase::WaitReSprint(left) => {
                if left <= 1 {
                    sk_debug(hooks, "Allowing Sprint");
                    state.cancel_sprint = false;
                    state.sprint_tap = None;
                } else {
                    state.sprint_tap = Some(SprintTapPhase::WaitReSprint(left - 1));
                }
            }
        }
    }

    if let Some(phase) = state.w_tap {
        match phase {
            WTapPhase::WaitMovementBlock(left) => {
                if left <= 1 {
                    sk_debug(hooks, "Disallowing Movement");
                    state.cancel_movement = true;
                    state.w_tap = Some(WTapPhase::WaitForwardImpulseClear);
                } else {
                    state.w_tap = Some(WTapPhase::WaitMovementBlock(left - 1));
                }
            }
            WTapPhase::WaitForwardImpulseClear => {
                if !player.has_forward_impulse {
                    sk_debug(hooks, "Waiting for Allowed Movement");
                    state.w_tap = Some(WTapPhase::WaitAllowedMovement(
                        state.until_allowed_movement_ticks,
                    ));
                }
            }
            WTapPhase::WaitAllowedMovement(left) => {
                if left <= 1 {
                    sk_debug(hooks, "Allowing Movement");
                    state.cancel_movement = false;
                    state.in_sequence = false;
                    state.w_tap = None;
                } else {
                    state.w_tap = Some(WTapPhase::WaitAllowedMovement(left - 1));
                }
            }
        }
    }
}

/// Upstream SprintTap `disable()`: `cancelSprint = false`.
pub fn sk_sprint_tap_disable(state: &mut SuperKnockbackState) {
    state.cancel_sprint = false;
    state.sprint_tap = None;
}

/// Upstream WTap `disable()`: `cancelMovement = false; inSequence = false`.
pub fn sk_w_tap_disable(state: &mut SuperKnockbackState) {
    state.cancel_movement = false;
    state.in_sequence = false;
    state.w_tap = None;
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



#[cfg(test)]
mod tests4 {
    use super::*;

    struct MockBlink {
        positions: Vec<Vec3>,
        oldest_age_ms: u64,
        flushed_outgoing: bool,
        flushed_up_to: Vec<usize>,
    }

    impl MockBlink {
        fn with_positions(positions: Vec<Vec3>) -> Self {
            Self { positions, oldest_age_ms: 0, flushed_outgoing: false, flushed_up_to: Vec::new() }
        }
    }

    impl FakeLagBlink for MockBlink {
        fn positions(&self) -> &[Vec3] { &self.positions }
        fn is_above_time(&self, delay: u64) -> bool { self.oldest_age_ms >= delay }
        fn flush_outgoing(&mut self) { self.flushed_outgoing = true; }
        fn flush_up_to(&mut self, idx: usize) { self.flushed_up_to.push(idx); }
    }

    struct MockAutoDodge {
        enabled: bool,
        inflicted: Option<()>,
        evading: Option<EvadingPacket>,
    }

    impl FakeLagAutoDodge for MockAutoDodge {
        fn enabled(&self) -> bool { self.enabled }
        fn get_inflicted_hit(&self, _position: Vec3) -> Option<()> { self.inflicted }
        fn find_avoiding_arrow_position(&self) -> Option<EvadingPacket> { self.evading }
    }

    struct MockWorld {
        enemy_found: bool,
        entities: Vec<FakeLagEntity>,
        player_box: (Vec3, Vec3),
    }

    impl FakeLagWorld for MockWorld {
        fn find_enemy(&self, _min_range: f32, _max_range: f32) -> bool { self.enemy_found }
        fn entities_box_in_range(&self, _mid_pos: Vec3, _range: f64) -> Vec<FakeLagEntity> {
            self.entities.clone()
        }
        fn make_player_box(&self, _position: Vec3) -> (Vec3, Vec3) { self.player_box }
    }

    fn entity_at(position: Vec3) -> FakeLagEntity {
        FakeLagEntity {
            position,
            box_min: Vec3 { x: position.x - 0.3, y: position.y, z: position.z - 0.3 },
            box_max: Vec3 { x: position.x + 0.3, y: position.y + 1.8, z: position.z + 0.3 },
            matches_predicate: true,
        }
    }

    fn player() -> FakeLagPlayer {
        FakeLagPlayer {
            id: 7,
            position: Vec3 { x: 0.0, y: 64.0, z: 0.0 },
            is_dead_or_dying: false,
            is_in_water: false,
            is_using_item: false,
            use_item_is_consumable: false,
        }
    }

    fn hooks<'a, N: FnMut(String, String)>(
        world: &'a mut MockWorld,
        blink: &'a mut MockBlink,
        dodge: &'a mut MockAutoDodge,
        notify: &'a mut N,
    ) -> FakeLagHooks<'a, N> {
        FakeLagHooks {
            world,
            blink,
            auto_dodge: dodge,
            notify,
            auto_shoot_constant_lag: false,
            auto_shoot_has_target: false,
            screen_open: false,
        }
    }

    fn outgoing(packet: FakeLagPacket) -> BlinkPacketEvent {
        BlinkPacketEvent { origin: TransferOrigin::Outgoing, packet, action: None }
    }

    #[test]
    fn flush_on_multi_choice_round_trips_and_rejects_unknown_tags() {
        let m = fake_lag();
        let json = m.setting("FlushOn").unwrap().to_json();
        assert!(matches!(json, crate::value::Json::Arr(_)));

        let mut m2 = fake_lag();
        let arr = crate::value::Json::Arr(vec![
            crate::value::Json::Str("Action".into()),
            crate::value::Json::Str("EntityInteract".into()),
        ]);
        m2.setting_mut("FlushOn").unwrap().set_from_json(&arr).unwrap();
        assert!(flush_on_matches_any(&m2, &FakeLagPacket::PlayerAction));
        assert!(flush_on_matches_any(&m2, &FakeLagPacket::Attack));
        assert!(!flush_on_matches_any(&m2, &FakeLagPacket::UseItemOn));

        let bad = crate::value::Json::Arr(vec![crate::value::Json::Str("Nope".into())]);
        assert!(m2.setting_mut("FlushOn").unwrap().set_from_json(&bad).is_err());
    }

    #[test]
    fn game_tick_tracks_enemy_and_flushes_for_auto_dodge() {
        let m = fake_lag();
        let mut state = fake_lag_state(400);

        // Enemy tracking.
        let mut world = MockWorld { enemy_found: true, entities: Vec::new(), player_box: (Vec3::ZERO, Vec3::ZERO) };
        let mut blink = MockBlink::with_positions(vec![]);
        let mut dodge = MockAutoDodge { enabled: false, inflicted: None, evading: None };
        let mut notes: Vec<String> = Vec::new();
        let mut notify = |t: String, msg: String| notes.push(format!("{t}: {msg}"));
        fake_lag_game_tick(&m, &mut state, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify));
        assert!(state.is_enemy_nearby);

        // AutoDodge enabled but no blink positions: nothing more happens.
        let mut dodge = MockAutoDodge { enabled: true, inflicted: Some(()), evading: None };
        let mut notes: Vec<String> = Vec::new();
        let mut notify = |t: String, msg: String| notes.push(format!("{t}: {msg}"));
        fake_lag_game_tick(&m, &mut state, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify));
        assert!(notes.is_empty());

        // No evading packet: flush everything outgoing with the fallback notice.
        let mut blink = MockBlink::with_positions(vec![Vec3 { x: 1.0, y: 64.0, z: 1.0 }]);
        let mut notes: Vec<String> = Vec::new();
        let mut notify = |t: String, msg: String| notes.push(format!("{t}: {msg}"));
        fake_lag_game_tick(&m, &mut state, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify));
        assert_eq!(notes, vec!["FakeLag: Unable to evade arrow. Blinking.".to_string()]);
        assert!(blink.flushed_outgoing);

        // Evading packet with ticks to impact: flush up to idx + 1.
        let mut dodge = MockAutoDodge {
            enabled: true,
            inflicted: Some(()),
            evading: Some(EvadingPacket { idx: 4, ticks_to_impact: Some(3) }),
        };
        let mut blink = MockBlink::with_positions(vec![Vec3 { x: 1.0, y: 64.0, z: 1.0 }]);
        let mut notes: Vec<String> = Vec::new();
        let mut notify = |t: String, msg: String| notes.push(format!("{t}: {msg}"));
        fake_lag_game_tick(&m, &mut state, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify));
        assert_eq!(notes, vec!["FakeLag: Trying to evade arrow...".to_string()]);
        assert_eq!(blink.flushed_up_to, vec![5]);
    }

    #[test]
    fn blink_handler_gates() {
        let m = fake_lag();
        let mut state = fake_lag_state(400);
        let mut world = MockWorld { enemy_found: false, entities: Vec::new(), player_box: (Vec3::ZERO, Vec3::ZERO) };
        let mut blink = MockBlink::with_positions(vec![]);
        let mut dodge = MockAutoDodge { enabled: false, inflicted: None, evading: None };
        let mut notes: Vec<String> = Vec::new();
        let mut notify = |t: String, msg: String| notes.push(format!("{t}: {msg}"));
        let p = player();

        // Incoming origin: nothing.
        let mut ev = BlinkPacketEvent { origin: TransferOrigin::Incoming, packet: FakeLagPacket::Other, action: None };
        fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &p, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify), 10_000, |lo, _| lo);
        assert_eq!(ev.action, None);

        // Dead player: nothing.
        let mut dead = p;
        dead.is_dead_or_dying = true;
        let mut ev = outgoing(FakeLagPacket::Other);
        fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &dead, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify), 10_000, |lo, _| lo);
        assert_eq!(ev.action, None);

        // Recoil time not elapsed: nothing (last reset 10_000 - 100 = still fresh).
        state.chronometer.reset(9_900);
        let mut ev = outgoing(FakeLagPacket::Other);
        fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &p, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify), 10_000, |lo, _| lo);
        assert_eq!(ev.action, None);

        // Above time: nextDelay is re-rolled to the random pick, nothing queued.
        state.chronometer.reset(9_000);
        blink.oldest_age_ms = 500; // >= nextDelay (400)
        let mut ev = outgoing(FakeLagPacket::Other);
        fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &p, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify), 10_000, |_, hi| hi);
        assert_eq!(state.next_delay, 600); // hi picked
        assert_eq!(ev.action, None);
    }

    #[test]
    fn flush_triggers_reset_the_chronometer() {
        let m = fake_lag();
        let mut world = MockWorld { enemy_found: false, entities: Vec::new(), player_box: (Vec3::ZERO, Vec3::ZERO) };
        let mut dodge = MockAutoDodge { enabled: false, inflicted: None, evading: None };
        let p = player();

        for packet in [
            FakeLagPacket::PlayerAction,                      // selected FlushOn match
            FakeLagPacket::ClientboundPlayerPosition,         // position packet
            FakeLagPacket::ServerboundResourcePack,
            FakeLagPacket::Attack,                            // EntityInteract family, selected
            FakeLagPacket::SetEntityMotion { id: 7, movement: Vec3 { x: 0.1, y: 0.0, z: 0.0 } },
            FakeLagPacket::Explode { player_knockback: Some(Vec3 { x: 0.0, y: 0.5, z: 0.0 }) },
            FakeLagPacket::SetHealth,
        ] {
            let mut state = fake_lag_state(400);
            state.chronometer.reset(9_000);
            let mut blink = MockBlink::with_positions(vec![]);
            let mut notes: Vec<String> = Vec::new();
            let mut notify = |t: String, msg: String| notes.push(format!("{t}: {msg}"));
            let mut ev = outgoing(packet);
            fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &p, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify), 10_000, |lo, _| lo);
            // A reset happened at now: the recoil window is fresh again.
            assert!(!state.chronometer.has_at_least_elapsed(250, 10_000), "no reset for {packet:?}");
            assert_eq!(ev.action, None);
        }
    }

    #[test]
    fn knockback_for_someone_else_or_zero_does_not_flush_and_consumables_do_not_lag() {
        let m = fake_lag();
        let mut world = MockWorld { enemy_found: false, entities: Vec::new(), player_box: (Vec3::ZERO, Vec3::ZERO) };
        let mut blink = MockBlink::with_positions(vec![]);
        let mut dodge = MockAutoDodge { enabled: false, inflicted: None, evading: None };
        let mut notes: Vec<String> = Vec::new();
        let mut notify = |t: String, msg: String| notes.push(format!("{t}: {msg}"));
        let mut p = player();

        // Knockback for another entity: falls through, no reset (recoil still elapsed),
        // and dynamic mode with no enemy nearby does not queue.
        let mut state = fake_lag_state(400);
        state.chronometer.reset(9_000);
        let mut ev = outgoing(FakeLagPacket::SetEntityMotion { id: 99, movement: Vec3 { x: 1.0, y: 0.0, z: 0.0 } });
        fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &p, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify), 10_000, |lo, _| lo);
        assert!(state.chronometer.has_at_least_elapsed(250, 10_000));
        assert_eq!(ev.action, None);

        // Using a consumable: no lag.
        p.is_using_item = true;
        p.use_item_is_consumable = true;
        let mut state = fake_lag_state(400);
        state.chronometer.reset(9_000);
        let mut ev = outgoing(FakeLagPacket::Other);
        fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &p, &mut hooks(&mut world, &mut blink, &mut dodge, &mut notify), 10_000, |lo, _| lo);
        assert_eq!(ev.action, None);
    }

    #[test]
    fn constant_mode_and_auto_shoot_support_queue() {
        let mut m = fake_lag();
        let mut world = MockWorld { enemy_found: false, entities: Vec::new(), player_box: (Vec3::ZERO, Vec3::ZERO) };
        let mut blink = MockBlink::with_positions(vec![]);
        let mut dodge = MockAutoDodge { enabled: false, inflicted: None, evading: None };
        let p = player();

        // AutoShoot with constant lag and no target queues even with no enemy.
        m.running = true;
        let mut noop = |_: String, _: String| {};
        let mut hk = hooks(&mut world, &mut blink, &mut dodge, &mut noop);
        hk.auto_shoot_constant_lag = true;
        let mut state = fake_lag_state(400);
        state.chronometer.reset(9_000);
        let mut ev = outgoing(FakeLagPacket::Other);
        fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &p, &mut hk, 10_000, |lo, _| lo);
        assert_eq!(ev.action, Some(BlinkAction::Queue));

        // Constant mode always queues.
        let mut m2 = fake_lag();
        m2.setting_mut("Mode").unwrap().kind = ValueKind::Choice("Constant".into(), vec!["Constant".into(), "Dynamic".into()]);
        let mut noop = |_: String, _: String| {};
        let mut hk = hooks(&mut world, &mut blink, &mut dodge, &mut noop);
        let mut state = fake_lag_state(400);
        state.chronometer.reset(9_000);
        let mut ev = outgoing(FakeLagPacket::Other);
        fake_lag_blink_packet_handler(&m2, &mut state, &mut ev, &p, &mut hk, 10_000, |lo, _| lo);
        assert_eq!(ev.action, Some(BlinkAction::Queue));
    }

    #[test]
    fn dynamic_mode_distance_and_intersection_rules() {
        let m = fake_lag();
        let mut dodge = MockAutoDodge { enabled: false, inflicted: None, evading: None };
        let p = player();
        let server_pos = Vec3 { x: 10.0, y: 64.0, z: 0.0 };

        let run = |world: &mut MockWorld, blink: &mut MockBlink, dodge: &mut MockAutoDodge| {
            let mut state = fake_lag_state(400);
            state.chronometer.reset(9_000);
            state.is_enemy_nearby = true;
            let mut ev = outgoing(FakeLagPacket::Other);
            fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &p, &mut hooks(world, blink, dodge, &mut |_, _| {}), 10_000, |lo, _| lo);
            ev.action
        };

        // No enemy nearby: no lag.
        let mut world = MockWorld { enemy_found: false, entities: vec![entity_at(server_pos)], player_box: (Vec3::ZERO, Vec3::ZERO) };
        let mut blink = MockBlink::with_positions(vec![server_pos]);
        {
            let mut state = fake_lag_state(400);
            state.chronometer.reset(9_000);
            let mut ev = outgoing(FakeLagPacket::Other);
            fake_lag_blink_packet_handler(&m, &mut state, &mut ev, &p, &mut hooks(&mut world, &mut blink, &mut dodge, &mut |_, _| {}), 10_000, |lo, _| lo);
            assert_eq!(ev.action, None);
        }

        // No blink positions yet: queue.
        let mut world = MockWorld { enemy_found: true, entities: vec![entity_at(server_pos)], player_box: (Vec3::ZERO, Vec3::ZERO) };
        let mut blink = MockBlink::with_positions(vec![]);
        assert_eq!(run(&mut world, &mut blink, &mut dodge), Some(BlinkAction::Queue));

        // No entities in range: no lag.
        let mut world = MockWorld { enemy_found: true, entities: vec![], player_box: (Vec3::ZERO, Vec3::ZERO) };
        let mut blink = MockBlink::with_positions(vec![server_pos]);
        assert_eq!(run(&mut world, &mut blink, &mut dodge), None);

        // Server position closer than client: keep lagging (no queue).
        let mut world = MockWorld {
            enemy_found: true,
            entities: vec![entity_at(Vec3 { x: 12.0, y: 64.0, z: 0.0 })],
            player_box: (Vec3::ZERO, Vec3::ZERO),
        };
        let mut blink = MockBlink::with_positions(vec![server_pos]);
        assert_eq!(run(&mut world, &mut blink, &mut dodge), None);

        // Player box intersects the entity: no lag.
        let mut world = MockWorld {
            enemy_found: true,
            entities: vec![entity_at(Vec3 { x: 20.0, y: 64.0, z: 0.0 })],
            player_box: (Vec3 { x: 19.0, y: 64.0, z: -0.3 }, Vec3 { x: 21.0, y: 65.8, z: 0.3 }),
        };
        let mut blink = MockBlink::with_positions(vec![server_pos]);
        assert_eq!(run(&mut world, &mut blink, &mut dodge), None);

        // Client position closer than server, no intersection: queue.
        let mut world = MockWorld {
            enemy_found: true,
            entities: vec![entity_at(Vec3 { x: 2.0, y: 64.0, z: 0.0 })],
            player_box: (Vec3 { x: -0.3, y: 64.0, z: -0.3 }, Vec3 { x: 0.3, y: 65.8, z: 0.3 }),
        };
        let mut blink = MockBlink::with_positions(vec![server_pos]);
        assert_eq!(run(&mut world, &mut blink, &mut dodge), Some(BlinkAction::Queue));
    }
}

#[cfg(test)]
mod tests5 {
    use super::*;

    struct MockNet {
        calls: Vec<&'static str>,
    }

    impl MockNet {
        fn new() -> Self {
            Self { calls: Vec::new() }
        }
    }

    impl SkNetwork for MockNet {
        fn send_start_sprinting(&mut self) {
            self.calls.push("start");
        }
        fn send_stop_sprinting(&mut self) {
            self.calls.push("stop");
        }
    }

    fn sprinting_player() -> SkPlayer {
        SkPlayer {
            position: Vec3 { x: 0.0, y: 64.0, z: 0.0 },
            on_ground: true,
            is_inside_water_or_bubble_column: false,
            is_sprinting: true,
            was_sprinting: true,
            movement_forward: 1.0,
            movement_sideways: 0.0,
            has_forward_impulse: true,
        }
    }

    fn living_target() -> SkTarget {
        SkTarget {
            is_living_entity: true,
            hurt_time: 0,
            look_angle: Vec3 { x: -1.0, y: 0.0, z: 0.0 }, // faces the player at +x
            position: Vec3 { x: 3.0, y: 64.0, z: 0.0 },
        }
    }

    fn hooks<'a>(net: &'a mut MockNet, debug: &'a mut Vec<String>) -> SkHooks<'a> {
        SkHooks { criticals_would_hit: false, debug, network: net }
    }

    #[test]
    fn settings_shape_and_mode_selection() {
        let m = super_knockback();
        assert_eq!(m.setting("Mode").unwrap().to_json(), crate::value::Json::Str("Packet".into()));
        match &m.setting("HurtTime").unwrap().kind {
            ValueKind::Int(10, 0, 10) => {}
            other => panic!("HurtTime {other:?}"),
        }
        match &m.setting("Chance").unwrap().kind {
            ValueKind::Float(100.0, 0.0, 100.0) => {}
            other => panic!("Chance {other:?}"),
        }
        assert_eq!(
            m.setting("Conditions").unwrap().to_json(),
            crate::value::Json::Arr(vec![crate::value::Json::Str("NotInWater".into())])
        );
        assert!(matches!(m.setting("OnlyOnMove").unwrap().kind, ValueKind::Bool(true)));

        let mut m2 = super_knockback();
        m2.setting_mut("Mode").unwrap().kind =
            ValueKind::Choice("WTap".into(), vec!["Packet".into(), "SprintTap".into(), "WTap".into()]);
        assert_eq!(sk_mode(&m2), Some(SkMode::WTap));
    }

    #[test]
    fn chance_as_boolean_edge_cases() {
        let m = super_knockback();
        assert!(sk_chance_as_boolean(&m, 0.0)); // 100% always true
        assert!(sk_chance_as_boolean(&m, 99.999));

        let mut m0 = super_knockback();
        m0.setting_mut("Chance").unwrap().kind = ValueKind::Float(0.0, 0.0, 100.0);
        assert!(!sk_chance_as_boolean(&m0, 0.0));
        assert!(!sk_chance_as_boolean(&m0, 50.0));

        let mut m50 = super_knockback();
        m50.setting_mut("Chance").unwrap().kind = ValueKind::Float(50.0, 0.0, 100.0);
        assert!(sk_chance_as_boolean(&m50, 49.9));
        assert!(!sk_chance_as_boolean(&m50, 50.0));
    }

    #[test]
    fn should_operate_movement_and_conditions() {
        let m = super_knockback();
        let player = sprinting_player();
        let target = living_target();

        // Moving forward: passes OnlyOnMove + NotInWater.
        assert!(sk_should_operate(&m, &player, &target));

        // Standing still: fails OnlyOnMove.
        let still = SkPlayer { movement_forward: 0.0, ..player };
        assert!(!sk_should_operate(&m, &still, &target));

        // Moving sideways with OnlyForward: fails.
        let sideways = SkPlayer { movement_forward: 0.0, movement_sideways: 1.0, ..player };
        assert!(!sk_should_operate(&m, &sideways, &target));

        // Moving sideways with OnlyForward off: passes.
        let mut m2 = super_knockback();
        m2.setting_mut("OnlyForward").unwrap().kind = ValueKind::Bool(false);
        assert!(sk_should_operate(&m2, &sideways, &target));

        // NotInWater condition: player in water fails.
        let in_water = SkPlayer { is_inside_water_or_bubble_column: true, ..player };
        assert!(!sk_should_operate(&m, &in_water, &target));

        // OnlyOnGround condition: player in the air fails.
        let mut m3 = super_knockback();
        m3.setting_mut("Conditions").unwrap().kind = ValueKind::MultiChoice(
            vec!["OnlyOnGround".into()],
            vec!["OnlyFacing".into(), "OnlyOnGround".into(), "NotInWater".into()],
        );
        let airborne = SkPlayer { on_ground: false, ..player };
        assert!(!sk_should_operate(&m3, &airborne, &target));
        assert!(sk_should_operate(&m3, &player, &target));

        // OnlyFacing: player behind the target (look angle dot <= 0) fails;
        // facing player passes. Player at 0,0 target at 3,0: direction to player is -x.
        let mut m4 = super_knockback();
        m4.setting_mut("Conditions").unwrap().kind = ValueKind::MultiChoice(
            vec!["OnlyFacing".into()],
            vec!["OnlyFacing".into(), "OnlyOnGround".into(), "NotInWater".into()],
        );
        let facing = SkTarget { look_angle: Vec3 { x: -1.0, y: 0.0, z: 0.0 }, ..living_target() };
        let away = SkTarget { look_angle: Vec3 { x: 1.0, y: 0.0, z: 0.0 }, ..living_target() };
        assert!(sk_should_operate(&m4, &player, &facing));
        assert!(!sk_should_operate(&m4, &player, &away));
    }

    #[test]
    fn packet_mode_sends_the_w_tap_packet_order() {
        let m = super_knockback();
        let mut player = sprinting_player();
        let target = living_target();
        let mut net = MockNet::new();
        let mut debug: Vec<String> = Vec::new();
        sk_packet_attack_handler(&m, &mut player, &target, &mut hooks(&mut net, &mut debug), 100.0);
        assert_eq!(net.calls, vec!["stop", "start", "stop", "start"]);
        assert!(player.is_sprinting && player.was_sprinting);

        // Not sprinting: no stop first.
        let mut player = sprinting_player();
        player.is_sprinting = false;
        player.was_sprinting = false;
        let mut net = MockNet::new();
        let mut debug: Vec<String> = Vec::new();
        sk_packet_attack_handler(&m, &mut player, &target, &mut hooks(&mut net, &mut debug), 100.0);
        assert_eq!(net.calls, vec!["start", "stop", "start"]);

        // HurtTime above the limit: nothing.
        let stale = SkTarget { hurt_time: 11, ..living_target() };
        let mut player = sprinting_player();
        let mut net = MockNet::new();
        let mut debug: Vec<String> = Vec::new();
        sk_packet_attack_handler(&m, &mut player, &stale, &mut hooks(&mut net, &mut debug), 100.0);
        assert!(net.calls.is_empty());

        // Non-living entity: nothing.
        let block = SkTarget { is_living_entity: false, ..living_target() };
        let mut player = sprinting_player();
        let mut net = MockNet::new();
        let mut debug: Vec<String> = Vec::new();
        sk_packet_attack_handler(&m, &mut player, &block, &mut hooks(&mut net, &mut debug), 100.0);
        assert!(net.calls.is_empty());

        // Criticals would hit: nothing.
        let mut player = sprinting_player();
        let mut net = MockNet::new();
        let mut debug: Vec<String> = Vec::new();
        let mut hk = hooks(&mut net, &mut debug);
        hk.criticals_would_hit = true;
        sk_packet_attack_handler(&m, &mut player, &target, &mut hk, 100.0);
        assert!(net.calls.is_empty());

        // Not operating (standing still): nothing.
        let still = SkPlayer { movement_forward: 0.0, ..sprinting_player() };
        let mut player = still;
        let mut net = MockNet::new();
        let mut debug: Vec<String> = Vec::new();
        sk_packet_attack_handler(&m, &mut player, &target, &mut hooks(&mut net, &mut debug), 100.0);
        assert!(net.calls.is_empty());
    }

    #[test]
    fn sprint_tap_sequence_lifecycle() {
        let m = super_knockback();
        let player = sprinting_player();
        let target = living_target();
        let mut state = super_knockback_state();
        let mut net = MockNet::new();
        let mut debug: Vec<String> = Vec::new();

        sk_sprint_tap_attack(&m, &mut state, &player, &target, &mut hooks(&mut net, &mut debug), 100.0, 2);
        assert!(state.cancel_sprint);
        assert_eq!(state.sprint_tap, Some(SprintTapPhase::WaitStopSprinting));
        assert_eq!(debug, vec!["Disallowing Sprint".to_string()]);

        // Sprint event is blocked while cancelled.
        let mut ev = SprintEvent { source: SprintEventSource::MovementTick, sprint: true };
        sk_sprint_handler(&state, &mut ev);
        assert!(!ev.sprint);
        let mut ev = SprintEvent { source: SprintEventSource::Input, sprint: true };
        sk_sprint_handler(&state, &mut ev);
        assert!(!ev.sprint);
        let mut ev = SprintEvent { source: SprintEventSource::Other, sprint: true };
        sk_sprint_handler(&state, &mut ev);
        assert!(ev.sprint); // other sources pass

        // Still sprinting: tickUntil keeps waiting.
        let mut debug2: Vec<String> = Vec::new();
        sk_tick(&mut state, &player, &mut hooks(&mut net, &mut debug2));
        assert_eq!(state.sprint_tap, Some(SprintTapPhase::WaitStopSprinting));

        // Player stopped sprinting: waits ReSprint ticks.
        let stopped = SkPlayer { is_sprinting: false, was_sprinting: false, ..player };
        let mut debug2: Vec<String> = Vec::new();
        sk_tick(&mut state, &stopped, &mut hooks(&mut net, &mut debug2));
        assert_eq!(state.sprint_tap, Some(SprintTapPhase::WaitReSprint(2)));
        assert_eq!(debug2, vec!["Waiting for ReSprint".to_string()]);

        // Two ticks later the sprint is allowed again.
        sk_tick(&mut state, &stopped, &mut hooks(&mut net, &mut debug2));
        sk_tick(&mut state, &stopped, &mut hooks(&mut net, &mut debug2));
        assert_eq!(state.sprint_tap, None);
        assert!(!state.cancel_sprint);

        // A second attack while the sequence already ended works again.
        let mut debug2: Vec<String> = Vec::new();
        sk_sprint_tap_attack(&m, &mut state, &stopped, &target, &mut hooks(&mut net, &mut debug2), 100.0, 0);
        // shouldStopSprinting requires isSprinting && wasSprinting
        assert!(!state.cancel_sprint);

        // Disable resets.
        sk_sprint_tap_disable(&mut state);
        assert!(!state.cancel_sprint && state.sprint_tap.is_none());
    }

    #[test]
    fn sprint_tap_attack_gates() {
        let m = super_knockback();
        let player = sprinting_player();
        let target = living_target();
        let mut state = super_knockback_state();
        let mut net = MockNet::new();

        // Not sprinting (shouldStopSprinting false): no sequence.
        let calm = SkPlayer { is_sprinting: false, was_sprinting: false, ..player };
        let mut debug: Vec<String> = Vec::new();
        sk_sprint_tap_attack(&m, &mut state, &calm, &target, &mut hooks(&mut net, &mut debug), 100.0, 1);
        assert!(!state.cancel_sprint && state.sprint_tap.is_none());

        // Criticals would hit: no sequence.
        let mut debug: Vec<String> = Vec::new();
        let mut hk = hooks(&mut net, &mut debug);
        hk.criticals_would_hit = true;
        sk_sprint_tap_attack(&m, &mut state, &player, &target, &mut hk, 100.0, 1);
        assert!(!state.cancel_sprint);

        // Already cancelled: no re-entry.
        state.cancel_sprint = true;
        let mut debug: Vec<String> = Vec::new();
        sk_sprint_tap_attack(&m, &mut state, &player, &target, &mut hooks(&mut net, &mut debug), 100.0, 1);
        assert_eq!(state.sprint_tap, None);
    }

    #[test]
    fn w_tap_sequence_lifecycle() {
        let m = super_knockback();
        let player = sprinting_player();
        let target = living_target();
        let mut state = super_knockback_state();
        let mut net = MockNet::new();
        let mut debug: Vec<String> = Vec::new();

        sk_w_tap_attack(&m, &mut state, &player, &target, &mut hooks(&mut net, &mut debug), 100.0, 1, 2);
        assert!(state.in_sequence);
        assert_eq!(state.w_tap, Some(WTapPhase::WaitMovementBlock(1)));
        assert_eq!(debug, vec!["Waiting for Movement Block".to_string()]);

        // Movement is NOT yet blocked during the block countdown.
        let mut ev = MovementInputEvent { directional_input: DirectionalInput::Other };
        sk_movement_input_handler(&state, &mut ev);
        assert_eq!(ev.directional_input, DirectionalInput::Other);

        // Countdown tick.
        sk_tick(&mut state, &player, &mut hooks(&mut net, &mut debug));
        assert_eq!(state.w_tap, Some(WTapPhase::WaitForwardImpulseClear));
        assert_eq!(debug.last().unwrap(), "Disallowing Movement");

        // Now movement input is zeroed.
        let mut ev = MovementInputEvent { directional_input: DirectionalInput::Other };
        sk_movement_input_handler(&state, &mut ev);
        assert_eq!(ev.directional_input, DirectionalInput::None);

        // Forward impulse still held: tickUntil waits.
        sk_tick(&mut state, &player, &mut hooks(&mut net, &mut debug));
        assert_eq!(state.w_tap, Some(WTapPhase::WaitForwardImpulseClear));

        // Impulse cleared: waits allowed movement.
        let quiet = SkPlayer { has_forward_impulse: false, ..player };
        sk_tick(&mut state, &quiet, &mut hooks(&mut net, &mut debug));
        assert_eq!(state.w_tap, Some(WTapPhase::WaitAllowedMovement(2)));
        assert_eq!(debug.last().unwrap(), "Waiting for Allowed Movement");

        // Two ticks later movement is allowed again.
        sk_tick(&mut state, &quiet, &mut hooks(&mut net, &mut debug));
        sk_tick(&mut state, &quiet, &mut hooks(&mut net, &mut debug));
        assert_eq!(state.w_tap, None);
        assert!(!state.cancel_movement && !state.in_sequence);

        // Disable resets.
        sk_w_tap_disable(&mut state);
        assert!(!state.cancel_movement && !state.in_sequence && state.w_tap.is_none());
    }

    #[test]
    fn w_tap_no_reentry_while_in_sequence() {
        let m = super_knockback();
        let player = sprinting_player();
        let target = living_target();
        let mut state = super_knockback_state();
        let mut net = MockNet::new();

        let mut debug: Vec<String> = Vec::new();
        sk_w_tap_attack(&m, &mut state, &player, &target, &mut hooks(&mut net, &mut debug), 100.0, 0, 0);
        assert!(state.in_sequence);

        let mut debug: Vec<String> = Vec::new();
        sk_w_tap_attack(&m, &mut state, &player, &target, &mut hooks(&mut net, &mut debug), 100.0, 0, 0);
        // Still in the first phase, not re-entered.
        assert_eq!(state.w_tap, Some(WTapPhase::WaitMovementBlock(0)));
        assert_eq!(debug.len(), 0);
    }
}

// ----- Rotation math (upstream utils/aiming/data/Rotation.kt) -----

/// Upstream `Rotation` (yaw/pitch in degrees). `isNormalized` is dropped:
/// every constructor here already produces wrapped values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

/// Upstream `Mth.wrapDegrees`.
pub fn wrap_degrees(value: f32) -> f32 {
    let d = (value as f64) % 360.0;
    let d = if d >= 180.0 { d - 360.0 } else { d };
    let d = if d < -180.0 { d + 360.0 } else { d };
    d as f32
}

/// Upstream `Rotation.fromRotationVec`.
pub fn rotation_from_vec(diff_x: f64, diff_y: f64, diff_z: f64) -> Rotation {
    Rotation {
        yaw: wrap_degrees((diff_z.atan2(diff_x) as f32).to_degrees() - 90.0),
        pitch: wrap_degrees(
            -(diff_y.atan2((diff_x * diff_x + diff_z * diff_z).sqrt()) as f32).to_degrees(),
        ),
    }
}

impl Rotation {
    /// Upstream `Rotation.lookingAt(point, from)`.
    pub fn looking_at(point: Vec3, from: Vec3) -> Rotation {
        rotation_from_vec(point.x - from.x, point.y - from.y, point.z - from.z)
    }

    /// Upstream `directionVector` (`Vec3.directionFromRotation(pitch, yaw)`).
    pub fn direction_vector(&self) -> Vec3 {
        let f = self.pitch as f64 * (std::f64::consts::PI / 180.0);
        let f1 = -(self.yaw as f64) * (std::f64::consts::PI / 180.0);
        let f2 = f1.cos();
        let f3 = f1.sin();
        let f4 = f.cos();
        let f5 = f.sin();
        Vec3 { x: f3 * f4, y: -f5, z: f2 * f4 }
    }

    /// Upstream `directionAngleTo`: angle between both direction vectors.
    pub fn direction_angle_to(&self, other: Rotation) -> f32 {
        let d = self.direction_vector();
        let o = other.direction_vector();
        let cx = d.y * o.z - d.z * o.y;
        let cy = d.z * o.x - d.x * o.z;
        let cz = d.x * o.y - d.y * o.x;
        let cross = (cx * cx + cy * cy + cz * cz).sqrt();
        (cross.atan2(d.dot(o)) as f32).to_degrees()
    }

    /// Upstream `interpolateTo` (`fma(factor, other - this, this)`).
    pub fn interpolate_to(&self, other: Rotation, factor: f32) -> Rotation {
        Rotation {
            yaw: factor * (other.yaw - self.yaw) + self.yaw,
            pitch: factor * (other.pitch - self.pitch) + self.pitch,
        }
    }
}

/// Upstream `sq` (`utils/math`).
fn sq(v: f64) -> f64 {
    v * v
}

// ----- KillAuraRequirements (upstream .../killaura/KillAuraRequirements.kt) -----

/// Upstream `KillAuraRequirements` tags.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KillAuraRequirement {
    Click,
    MouseMovement,
    Weapon,
    EmptyHand,
    VanillaName,
    NotBreaking,
}

impl KillAuraRequirement {
    pub fn tag(self) -> &'static str {
        match self {
            KillAuraRequirement::Click => "Click",
            KillAuraRequirement::MouseMovement => "MouseMovement",
            KillAuraRequirement::Weapon => "Weapon",
            KillAuraRequirement::EmptyHand => "EmptyHand",
            KillAuraRequirement::VanillaName => "VanillaName",
            KillAuraRequirement::NotBreaking => "NotBreaking",
        }
    }

    pub fn from_tag(tag: &str) -> Option<KillAuraRequirement> {
        Some(match tag {
            "Click" => KillAuraRequirement::Click,
            "MouseMovement" => KillAuraRequirement::MouseMovement,
            "Weapon" => KillAuraRequirement::Weapon,
            "EmptyHand" => KillAuraRequirement::EmptyHand,
            "VanillaName" => KillAuraRequirement::VanillaName,
            "NotBreaking" => KillAuraRequirement::NotBreaking,
            _ => return None,
        })
    }
}

/// The client/main-hand inputs the upstream per-tag predicates read, one
/// snapshot the caller (or mock) supplies.
#[derive(Clone, Copy, Debug, Default)]
pub struct KillAuraRequirementInputs {
    /// `mc.options.keyAttack.isPressedOnAny || keyAttack.wasPressedRecently(250)`
    pub click_pressed: bool,
    /// `wasMouseMovedRecently(100)`
    pub mouse_moved_recently: bool,
    /// `player.mainHandItem.isWeapon()` (sword, past-1.8 axe, mace or knockback book)
    pub main_hand_is_weapon: bool,
    /// `player.mainHandItem.isEmpty`
    pub main_hand_is_empty: bool,
    /// `player.mainHandItem.customName != null`
    pub main_hand_has_custom_name: bool,
    /// `mc.gameMode?.isDestroying == false` carried as not_breaking
    pub not_breaking: bool,
}

/// Upstream `KillAuraRequirements.getAsBoolean`.
pub fn killaura_requirement_as_boolean(
    req: KillAuraRequirement,
    inputs: &KillAuraRequirementInputs,
) -> bool {
    match req {
        KillAuraRequirement::Click => inputs.click_pressed,
        KillAuraRequirement::MouseMovement => inputs.mouse_moved_recently,
        KillAuraRequirement::Weapon => inputs.main_hand_is_weapon,
        KillAuraRequirement::EmptyHand => inputs.main_hand_is_empty,
        KillAuraRequirement::VanillaName => !inputs.main_hand_has_custom_name,
        KillAuraRequirement::NotBreaking => inputs.not_breaking,
    }
}

/// Upstream `requires.all { it.asBoolean }` over the module's `Requires`
/// multi-enum setting.
pub fn killaura_requirements_met(
    module: &Module,
    inputs: &KillAuraRequirementInputs,
) -> bool {
    let Some(ValueKind::MultiChoice(selected, _)) =
        module.setting("Requires").map(|v| &v.kind)
    else {
        return false;
    };
    selected.iter().all(|tag| {
        KillAuraRequirement::from_tag(tag)
            .map(|r| killaura_requirement_as_boolean(r, inputs))
            .unwrap_or(false)
    })
}

/// Upstream `RotationManager.setRotationTarget(rotation, priority, requester)`
/// record; `considerInventory` is the `toRotationTarget` flag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotationTargetCall {
    pub rotation: Rotation,
    pub consider_inventory: bool,
}

// ----- Seams + ModuleAutoRod.kt -----

/// Upstream `FishingHook` slice AutoRod reads each tick.
#[derive(Clone, Copy, Debug)]
pub struct ArFishingBobber {
    /// `fishingBobberEntity.hookedIn != null`
    pub hooked_in: bool,
    /// `fishingBobberEntity.knownMovement`
    pub known_movement: Vec3,
}

/// Upstream `HotbarItemSlot` slice AutoRod reads (OffhandWithHotbar rod slot).
#[derive(Clone, Copy, Debug)]
pub struct ArRodSlot {
    /// `slot.isSelected`
    pub is_selected: bool,
    /// `slot.itemStack.is(Items.FISHING_ROD)`
    pub is_fishing_rod: bool,
    /// `slot.inventorySlot`, handed to `useHotbarSlotOrOffhand`.
    pub inventory_slot: i64,
}

/// Upstream `LivingEntity` target slice AutoRod reads.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ArTarget {
    /// `enemy.getActualHealth()`
    pub health: f64,
}

/// Upstream `player` slice AutoRod reads.
#[derive(Clone, Debug)]
pub struct ArPlayer {
    pub eye_position: Vec3,
    pub health: f64,
    /// `player.mainHandItem.item` registry id (matched against HoldingItemsForIgnore).
    pub main_hand_item: String,
    /// `player.isUsingItem`
    pub is_using_item: bool,
    /// `player.mainHandItem.isConsumable()`
    pub main_hand_is_consumable: bool,
    /// `player.offhandItem.isConsumable()`
    pub offhand_is_consumable: bool,
}

/// Engine seams AutoRod needs. `targets()` returns the tracker's entries in
/// its standing priority order (DISTANCE); `distance_to_sqr` reports the
/// live `player.distanceToSqr(enemy)` the condition lambdas re-evaluate.
pub trait ArWorld {
    fn targets(&mut self) -> Vec<ArTarget>;
    fn distance_to_sqr(&mut self, target: &ArTarget) -> f64;
    /// `world.entityGetter.firstOrNull(EntityTypes.FISHING_BOBBER) { it.playerOwner === player }`
    fn find_own_bobber(&mut self) -> Option<ArFishingBobber>;
    /// `Slots.OffhandWithHotbar.findSlot(Items.FISHING_ROD)`
    fn find_rod_slot(&mut self) -> Option<ArRodSlot>;
    /// `player.hasLineOfSight(enemy)`
    fn has_line_of_sight(&mut self, target: &ArTarget) -> bool;
    /// `PointTracker.findPoint(eyes, target, 1)` → the aimed point position.
    fn find_point(&mut self, eyes: Vec3, target: &ArTarget) -> Option<Vec3>;
    /// `SituationalProjectileAngleCalculator.calculateAngleForEntity(TrajectoryInfo.FISHING_ROD, target)`
    fn projectile_angle(&mut self, target: &ArTarget) -> Option<Rotation>;
}

/// Upstream `SilentHotbar` + `interaction` seams.
pub trait ArHotbar {
    /// `useHotbarSlotOrOffhand(slot, ticksUntilReset, swingMode).consumesAction()`
    fn use_hotbar_slot_or_offhand(
        &mut self,
        slot: &ArRodSlot,
        ticks_until_reset: i64,
        swing_mode: &str,
    ) -> bool;
    /// `interaction.releaseUsingItemInTickLoop()`
    fn release_using_item_in_tick_loop(&mut self);
    /// `SilentHotbar.resetSlot(this)`
    fn reset_slot(&mut self);
}

pub struct ArHooks<'a> {
    pub world: &'a mut dyn ArWorld,
    pub hotbar: &'a mut dyn ArHotbar,
    /// `RotationManager.serverRotation`
    pub server_rotation: Rotation,
    /// `RotationManager.setRotationTarget` calls, in order.
    pub rotation_targets: &'a mut Vec<RotationTargetCall>,
    /// `ModuleBlink.running` / `ModuleScaffold.running` / `ModuleFreeze.running`
    pub blink_running: bool,
    pub scaffold_running: bool,
    pub freeze_running: bool,
    /// `InventoryManager.isInventoryOpen` (Ignore.OpenInventory, first clause)
    pub inventory_open: bool,
    /// `mc.gui.screen() is AbstractContainerScreen` (Ignore.OpenInventory, second clause)
    pub container_screen_open: bool,
    /// Debug feed (`debugParameter`), shared shape with SuperKnockback.
    pub debug: &'a mut Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArGravityType {
    Linear,
    Projectile,
}

fn ar_gravity_type(module: &Module) -> ArGravityType {
    match module.setting("GravityType").and_then(|v| v.kind.choice()) {
        Some("Projectile") => ArGravityType::Projectile,
        _ => ArGravityType::Linear,
    }
}

/// Upstream `Ignore` tags, carried as their `asBoolean` inputs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArIgnore {
    OpenInventory,
    UsingItem,
    HoldingConsumable,
}

fn ar_ignores_any(module: &Module, player: &ArPlayer, hooks: &ArHooks) -> bool {
    let Some(ValueKind::MultiChoice(selected, _)) =
        module.setting("Ignore").map(|v| &v.kind)
    else {
        return false;
    };
    selected.iter().any(|tag| match tag.as_str() {
        "OpenInventory" => hooks.inventory_open || hooks.container_screen_open,
        "UsingItem" => player.is_using_item,
        "HoldingConsumable" => player.main_hand_is_consumable || player.offhand_is_consumable,
        _ => false,
    })
}

/// Upstream `requirementsMet`.
pub fn ar_requirements_met(
    module: &Module,
    state: &AutoRodState,
    player: &ArPlayer,
    hooks: &mut ArHooks,
    inputs: &KillAuraRequirementInputs,
) -> bool {
    killaura_requirements_met(module, inputs)
        && !ar_ignores_any(module, player, hooks)
        && player.health > ar_min_health(module)
        && state.available_rod_slot.is_some()
        && !ar_holding_ignored_item(module, player)
        && !hooks.blink_running
        && !hooks.scaffold_running
        && !hooks.freeze_running
        && (ar_max_enemies_nearby(module) == 0
            || hooks.world.targets().len() <= ar_max_enemies_nearby(module) as usize)
}

fn ar_holding_ignored_item(module: &Module, player: &ArPlayer) -> bool {
    match module.setting("HoldingItemsForIgnore").map(|v| &v.kind) {
        Some(ValueKind::Items(list)) => list.iter().any(|it| *it == player.main_hand_item),
        _ => false,
    }
}

/// Per-tick `computedOn(FIRST_PRIORITY)` refreshes: the bobber recomputes
/// every tick, the rod slot keeps its cache while it is still the selected
/// rod slot, else falls back to `Slots.OffhandWithHotbar.findSlot`.
pub fn ar_refresh_per_tick(module: &Module, state: &mut AutoRodState, hooks: &mut ArHooks) {
    state.bobber = hooks.world.find_own_bobber();
    let keep = state
        .available_rod_slot
        .take()
        .filter(|s| s.is_selected && s.is_fishing_rod);
    state.available_rod_slot = keep.or_else(|| hooks.world.find_rod_slot());
}

/// Upstream `tickConditional(hitTimeout) { ... }` condition, evaluated each
/// tick while the sequence is open.
fn ar_pull_condition(
    module: &Module,
    state: &AutoRodState,
    hooks: &mut ArHooks,
    target: &ArTarget,
    min_range_sq: f64,
    max_range_sq: f64,
) -> bool {
    let distance = hooks.world.distance_to_sqr(target);
    state.bobber.map_or(false, |b| b.hooked_in)
        || state.bobber.map_or(false, |b| b.known_movement == Vec3::ZERO)
        || (ar_pull_on_out_of_range(module)
            && !(distance >= min_range_sq && distance <= max_range_sq))
}

/// Upstream `handleAutoRod` (`tickHandler`), as a per-tick phase machine.
/// `cooldown_pick` and `slot_reset_pick` stand in for `cooldown.random()` /
/// `slotResetDelay.random()`; `scan_extra_pick` for `scanExtraRange.random()`.
pub fn auto_rod_tick(
    module: &Module,
    state: &mut AutoRodState,
    player: &ArPlayer,
    hooks: &mut ArHooks,
    inputs: &KillAuraRequirementInputs,
    cooldown_pick: i64,
    slot_reset_pick: i64,
    scan_extra_pick: f64,
) {
    ar_refresh_per_tick(module, state, hooks);
    hooks
        .debug
        .push(format!("fishingBobberEntity.hookedEntity={}", state.bobber.map_or("null".to_string(), |_| "hooked".to_string())));

    // Suspended coroutine continues here: no requirements check re-runs.
    if let Some(phase) = state.phase.take() {
        match phase {
            ArPhase::WaitPull {
                slot,
                target,
                min_range_sq,
                max_range_sq,
                timeout_left,
            } => {
                if ar_pull_condition(module, state, hooks, &target, min_range_sq, max_range_sq)
                    || timeout_left <= 1
                {
                    // 4. pull; 5. reset slot (overrides the previous action)
                    hooks.hotbar.use_hotbar_slot_or_offhand(
                        &slot,
                        slot_reset_pick,
                        &ar_swing_mode(module).to_string(),
                    );
                    state.phase = Some(ArPhase::WaitCooldown { left: cooldown_pick });
                } else {
                    state.phase = Some(ArPhase::WaitPull {
                        slot,
                        target,
                        min_range_sq,
                        max_range_sq,
                        timeout_left: timeout_left - 1,
                    });
                }
                return;
            }
            ArPhase::WaitCooldown { left } => {
                if left > 1 {
                    state.phase = Some(ArPhase::WaitCooldown { left: left - 1 });
                }
                return;
            }
        }
    }

    if !ar_requirements_met(module, state, player, hooks, inputs) {
        return;
    }

    let Some(slot) = state.available_rod_slot else {
        return;
    };

    // `targetTracker.target` standing target (tracker-maintained).
    let Some(target) = hooks.world.targets().first().copied() else {
        return;
    };
    state.target = Some(target);

    let Some(rotation) = ar_gravity_apply(module, player, hooks.world, &target) else {
        return;
    };
    let rotation_difference = hooks.server_rotation.direction_angle_to(rotation);
    if rotation_difference > ar_aim_off_threshold(module) {
        return;
    }

    // If the player used rod manually, skip use.
    if state.bobber.is_none() {
        // 1. select rod; 2. push. The player should hold the rod until pulling.
        let ticks_until_reset = ar_slot_reset_delay(module).1 + ar_hit_timeout(module);
        let swing = ar_swing_mode(module).to_string();
        if !hooks
            .hotbar
            .use_hotbar_slot_or_offhand(&slot, ticks_until_reset, &swing)
        {
            // Action failed
            return;
        }
        state.current_scan_extra_range = scan_extra_pick;
    }

    let range = ar_range(module);
    let max_range_sq = sq(range.1 + state.current_scan_extra_range);
    let min_range_sq = sq(range.0);

    // 3. timeout / hit entity / no movement / out of range
    state.phase = Some(ArPhase::WaitPull {
        slot,
        target,
        min_range_sq,
        max_range_sq,
        timeout_left: ar_hit_timeout(module),
    });
}

/// Upstream `rotationUpdateHandler`.
pub fn auto_rod_rotation_update(
    module: &Module,
    state: &mut AutoRodState,
    player: &ArPlayer,
    hooks: &mut ArHooks,
    inputs: &KillAuraRequirementInputs,
) {
    ar_refresh_per_tick(module, state, hooks);
    if !ar_requirements_met(module, state, player, hooks, inputs) {
        // targetTracker.reset()
        state.target = None;
        return;
    }

    let range = ar_range(module);
    let max_range_sq = sq(range.1 + state.current_scan_extra_range);
    let min_range_sq = sq(range.0);

    // `targetTracker.selectFirst { ... }` — sets `.target` too.
    let candidates = hooks.world.targets();
    let target = candidates.iter().copied().find(|enemy| {
        let distance = hooks.world.distance_to_sqr(enemy);
        distance >= min_range_sq
            && distance <= max_range_sq
            && hooks.world.has_line_of_sight(enemy)
            && enemy.health > ar_min_target_health(module)
    });
    let Some(target) = target else {
        return;
    };
    state.target = Some(target);

    let Some(rotation) = ar_gravity_apply(module, player, hooks.world, &target) else {
        return;
    };
    hooks.rotation_targets.push(RotationTargetCall {
        rotation,
        consider_inventory: false,
    });
}

/// Upstream `GravityType.apply(target)`.
fn ar_gravity_apply(
    module: &Module,
    player: &ArPlayer,
    world: &mut dyn ArWorld,
    target: &ArTarget,
) -> Option<Rotation> {
    match ar_gravity_type(module) {
        ArGravityType::Linear => {
            let eyes = player.eye_position;
            let point = world.find_point(eyes, target)?;
            Some(Rotation::looking_at(point, eyes))
        }
        ArGravityType::Projectile => world.projectile_angle(target),
    }
}

/// Upstream `onDisabled`.
pub fn auto_rod_on_disabled(state: &mut AutoRodState, hooks: &mut ArHooks) {
    state.target = None; // targetTracker.reset()
    if hooks.world.find_own_bobber().is_some() {
        hooks.hotbar.release_using_item_in_tick_loop();
    }
    state.bobber = None;
    state.available_rod_slot = None;
    hooks.hotbar.reset_slot(); // SilentHotbar.resetSlot(this)
}

/// Coroutine phase machine for the `tickHandler` sequence.
#[derive(Clone, Copy, Debug)]
pub enum ArPhase {
    /// `tickConditional(hitTimeout) { ... }` while the rod is selected;
    /// the range bounds are the captured `minRangeSq`/`maxRangeSq`.
    WaitPull {
        slot: ArRodSlot,
        target: ArTarget,
        min_range_sq: f64,
        max_range_sq: f64,
        timeout_left: i64,
    },
    /// `waitTicks(cooldown.random())` after the pull action.
    WaitCooldown { left: i64 },
}

#[derive(Clone, Debug, Default)]
pub struct AutoRodState {
    /// `currentScanExtraRange` (random pick of `ScanExtraRange`).
    pub current_scan_extra_range: f64,
    /// `fishingBobberEntity` (recomputed every tick).
    pub bobber: Option<ArFishingBobber>,
    /// `availableRodSlot` (cached while still the selected rod slot).
    pub available_rod_slot: Option<ArRodSlot>,
    /// `targetTracker.target`
    pub target: Option<ArTarget>,
    /// Suspended coroutine phase.
    pub phase: Option<ArPhase>,
}

pub fn auto_rod_state(initial_scan_extra: f64) -> AutoRodState {
    AutoRodState { current_scan_extra_range: initial_scan_extra, ..Default::default() }
}

pub fn auto_rod() -> Module {
    Module::new(
        "AutoRod",
        COMBAT,
        vec![
            choice("GravityType", "Linear", &["Linear", "Projectile"]),
            float_range("Range", 3.5, 5.0, 2.0, 10.0),
            float_range("ScanExtraRange", 0.0, 0.0, 0.0, 5.0),
            int("MaxEnemiesNearby", 1, 0, 10), // 0 = no limit
            float("MinHealth", 10.0, 1.0, 20.0),
            float("MinTargetHealth", 4.0, 1.0, 20.0),
            // enumSetOf(EMPTY_HAND).complement()
            multi_choice(
                "Requires",
                &["Click", "MouseMovement", "Weapon", "VanillaName", "NotBreaking"],
                &["Click", "MouseMovement", "Weapon", "EmptyHand", "VanillaName", "NotBreaking"],
            ),
            multi_choice("Ignore", &[], &["OpenInventory", "UsingItem", "HoldingConsumable"]),
            items(
                "HoldingItemsForIgnore",
                &[
                    "minecraft:bow",
                    "minecraft:crossbow",
                    "minecraft:trident",
                    "minecraft:fire_charge",
                    "minecraft:ender_pearl",
                ],
            ),
            float("AimOffThreshold", 5.0, 2.0, 10.0),
            choice("SwingMode", "DoNotHide", &["DoNotHide", "HideForClient"]),
            int("HitTimeout", 30, 5, 200),
            boolean("PullOnOutOfRange", true),
            int_range("SlotResetDelay", 0, 0, 0, 20),
            int_range("Cooldown", 4, 8, 1, 50),
        ],
    )
}

fn ar_min_health(module: &Module) -> f64 {
    match module.setting("MinHealth").map(|v| &v.kind) {
        Some(ValueKind::Float(v, _, _)) => *v,
        _ => 10.0,
    }
}

fn ar_min_target_health(module: &Module) -> f64 {
    match module.setting("MinTargetHealth").map(|v| &v.kind) {
        Some(ValueKind::Float(v, _, _)) => *v,
        _ => 4.0,
    }
}

fn ar_max_enemies_nearby(module: &Module) -> i64 {
    match module.setting("MaxEnemiesNearby").map(|v| &v.kind) {
        Some(ValueKind::Int(v, _, _)) => *v,
        _ => 1,
    }
}

fn ar_aim_off_threshold(module: &Module) -> f32 {
    match module.setting("AimOffThreshold").map(|v| &v.kind) {
        Some(ValueKind::Float(v, _, _)) => *v as f32,
        _ => 5.0,
    }
}

fn ar_hit_timeout(module: &Module) -> i64 {
    match module.setting("HitTimeout").map(|v| &v.kind) {
        Some(ValueKind::Int(v, _, _)) => *v,
        _ => 30,
    }
}

fn ar_pull_on_out_of_range(module: &Module) -> bool {
    match module.setting("PullOnOutOfRange").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => true,
    }
}

fn ar_range(module: &Module) -> (f64, f64) {
    match module.setting("Range").map(|v| &v.kind) {
        Some(ValueKind::FloatRange(a, b, _, _)) => (*a, *b),
        _ => (3.5, 5.0),
    }
}

fn ar_slot_reset_delay(module: &Module) -> (i64, i64) {
    match module.setting("SlotResetDelay").map(|v| &v.kind) {
        Some(ValueKind::IntRange(a, b, _, _)) => (*a, *b),
        _ => (0, 0),
    }
}

fn ar_swing_mode(module: &Module) -> ArSwingMode {
    match module.setting("SwingMode").and_then(|v| v.kind.choice()) {
        Some("HideForClient") => ArSwingMode::HideForClient,
        _ => ArSwingMode::DoNotHide,
    }
}

/// Upstream `SwingMode` (the module-facing slice: tag + serverSwing).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArSwingMode {
    DoNotHide,
    HideForClient,
    HideForBoth,
    HideForServer,
}

impl ArSwingMode {
    pub fn tag(self) -> &'static str {
        match self {
            ArSwingMode::DoNotHide => "DoNotHide",
            ArSwingMode::HideForBoth => "HideForBoth",
            ArSwingMode::HideForClient => "HideForClient",
            ArSwingMode::HideForServer => "HideForServer",
        }
    }
}

impl std::fmt::Display for ArSwingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.tag())
    }
}

// ----- Seams + ModuleAutoWeapon.kt -----

/// Upstream `ItemStack` slice AutoWeapon reads.
#[derive(Clone, Debug)]
pub struct AwItemStack {
    /// Seam identity for the upstream `itemStack == bestItem` checks.
    pub id: u64,
    pub is_empty: bool,
    /// WeaponType tags whose predicate tests true on this stack (e.g. ["Sword"]).
    pub weapon_tags: Vec<String>,
    /// `stack.attackDamage`
    pub attack_damage: f64,
    /// `stack.attackSpeed`
    pub attack_speed: f64,
    /// `stack.getEnchantment(Enchantments.KNOCKBACK)`
    pub knockback_level: i64,
    /// `stack.isAxe`
    pub is_axe: bool,
    /// `stack.item is MaceItem`
    pub is_mace: bool,
}

impl AwItemStack {
    /// Upstream `WeaponType.matchesAny(set, stack)`.
    pub fn matches_any(&self, selected: &[String]) -> bool {
        selected.iter().any(|t| self.weapon_tags.iter().any(|w| w == t))
    }

    /// `WeaponType.test(stack)` for one tag.
    pub fn test(&self, tag: &str) -> bool {
        self.weapon_tags.iter().any(|w| w == tag)
    }
}

/// Upstream `HotbarItemSlot` slice AutoWeapon reads.
#[derive(Clone, Debug)]
pub struct AwSlot {
    /// `slot.inventorySlot` (hotbar index).
    pub inventory_slot: i64,
    pub stack: AwItemStack,
    /// Upstream `ItemCategorization.getItemFacets(slot)` weapon facets, the
    /// `WeaponItemFacet.COMPARATOR` order collapsed to a rank. Smaller is
    /// better: upstream `.maxOrNull()` picks the COMPARATOR's largest
    /// element (the best weapon). `None` = no weapon facet on this slot.
    pub facet_rank: Option<i64>,
}

pub trait AwInventory {
    /// `Slots.Hotbar` slots with stacks and weapon facets.
    fn hotbar(&mut self) -> Vec<AwSlot>;
}

/// Upstream `player` slice AutoWeapon reads.
#[derive(Clone, Debug)]
pub struct AwPlayer {
    /// `player.mainHandItem` (the willShieldBreak/willMaceSmash checks).
    pub main_hand: AwItemStack,
    /// `MaceItem.canSmashAttack(player)` (false on <=1.8)
    pub can_smash_attack: bool,
    /// `player.hasCooldown`
    pub has_cooldown: bool,
    /// `player.isUsingItem && player.usingItemHand == MAIN_HAND && player.useItem.isConsumable`
    pub using_consumable_main_hand: bool,
    /// `SilentHotbar.isSlotModifiedBy(ModuleAutoBuff)`
    pub slot_modified_by_auto_buff: bool,
}

pub struct AwHooks<'a> {
    pub inventory: &'a mut dyn AwInventory,
    /// `ModuleMaceKill.enabled`
    pub mace_kill_enabled: bool,
    /// `isOlderThanOrEqual1_8`
    pub is_older_or_equal_1_8: bool,
    /// `SilentHotbar.selectSlotSilently(this, slot, switchBack)` calls, in order.
    pub selections: &'a mut Vec<(i64, i64)>,
    /// `interaction.ensureHasSentCarriedItem()` call count.
    pub ensure_sent_carried_item_calls: &'a mut usize,
    /// `SilentHotbar.resetSlot(this)` call count (`onDisabled`).
    pub reset_slot_calls: &'a mut usize,
}

/// Upstream `AttackEntityEvent` flattened to the attacked entity.
pub struct AwAttackTarget {
    /// `event.entity as? LivingEntity ?: return`
    pub is_living_entity: bool,
    /// `target.wouldBlockHit`
    pub would_block_hit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AwPriority {
    Default,
    Knockback,
    Damage,
    AttackSpeed,
}

fn aw_priority(module: &Module) -> AwPriority {
    match module.setting("Priority").and_then(|v| v.kind.choice()) {
        Some("Knockback") => AwPriority::Knockback,
        Some("Damage") => AwPriority::Damage,
        Some("AttackSpeed") => AwPriority::AttackSpeed,
        _ => AwPriority::Default,
    }
}

fn aw_preferred(module: &Module) -> Vec<String> {
    match module.setting("Preferred").map(|v| &v.kind) {
        Some(ValueKind::MultiChoice(sel, _)) => sel.clone(),
        _ => Vec::new(),
    }
}

fn aw_change_on_has(module: &Module, tag: &str) -> bool {
    match module.setting("ChangeOn").map(|v| &v.kind) {
        Some(ValueKind::MultiChoice(sel, _)) => sel.iter().any(|t| t == tag),
        _ => false,
    }
}

fn aw_auto_shield_break(module: &Module) -> bool {
    match module.setting("AutoShieldBreak").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => true,
    }
}

fn aw_auto_mace(module: &Module) -> bool {
    match module.setting("AutoMace").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => true,
    }
}

fn aw_switch_back(module: &Module) -> i64 {
    match module.setting("SwitchBack").map(|v| &v.kind) {
        Some(ValueKind::Int(v, _, _)) => *v,
        _ => 20,
    }
}

/// Upstream `isBusy`: prioritize Auto Buff or consuming an item over Auto Weapon.
fn aw_is_busy(player: &AwPlayer) -> bool {
    player.slot_modified_by_auto_buff || player.using_consumable_main_hand
}

/// Upstream `canMaceSmash`.
fn aw_can_mace_smash(player: &AwPlayer, hooks: &AwHooks) -> bool {
    (!hooks.is_older_or_equal_1_8 && player.can_smash_attack) || hooks.mace_kill_enabled
}

/// Upstream `getBest*Item`: hotbar stacks filtered by the preferred weapon
/// types, `maxByOrNull` the metric (first element kept on ties).
fn aw_best_by(module: &Module, hooks: &mut AwHooks, pick: fn(&AwItemStack) -> f64) -> Option<u64> {
    let slots = hooks.inventory.hotbar();
    let preferred = aw_preferred(module);
    let mut best: Option<(f64, u64)> = None;
    for s in &slots {
        if s.stack.is_empty || !s.stack.matches_any(&preferred) {
            continue;
        }
        let v = pick(&s.stack);
        match best {
            None => best = Some((v, s.stack.id)),
            Some((bv, _)) if v > bv => best = Some((v, s.stack.id)),
            _ => {}
        }
    }
    best.map(|(_, id)| id)
}

/// Upstream `determineWeaponSlot(target, enforceShield)`; returns the
/// chosen hotbar slot's `inventorySlot`.
pub fn aw_determine_weapon_slot(
    module: &Module,
    player: &AwPlayer,
    hooks: &mut AwHooks,
    target: Option<&AwAttackTarget>,
    enforce_shield: bool,
) -> Option<i64> {
    let requires_shield =
        aw_auto_shield_break(module) && (enforce_shield || target.map(|t| t.would_block_hit).unwrap_or(false));
    let requires_mace = aw_auto_mace(module) && aw_can_mace_smash(player, hooks);
    let preferred = aw_preferred(module);
    let priority = aw_priority(module);

    // getBest* over `Slots.Hotbar.stacks` (evaluated before the facet filter,
    // same as upstream's `when` arms).
    let best_knockback = aw_best_by(module, hooks, |s| s.knockback_level as f64);
    let best_damage = aw_best_by(module, hooks, |s| s.attack_damage);
    let best_speed = aw_best_by(module, hooks, |s| s.attack_speed);

    // `Slots.Hotbar.flatMap { itemCategorization.getItemFacets(it)
    //     .filterIsInstance<WeaponItemFacet>() }.filter { ... }.maxOrNull()`
    let slots = hooks.inventory.hotbar();
    let mut best: Option<(i64, i64)> = None; // (inventory_slot, facet_rank)
    for slot in &slots {
        let Some(rank) = slot.facet_rank else {
            continue;
        };
        let stack = &slot.stack;
        let ok = if requires_mace {
            // A mace's smash attack cannot be blocked by a shield
            stack.test("Mace")
        } else if requires_shield {
            // An axe will stun the target if it is blocking with a shield
            stack.test("Axe")
        } else if priority == AwPriority::Knockback {
            Some(stack.id) == best_knockback
        } else if priority == AwPriority::Damage {
            Some(stack.id) == best_damage
        } else if priority == AwPriority::AttackSpeed {
            Some(stack.id) == best_speed
        } else {
            // Fall back to a preferred weapon when no special case applies
            stack.matches_any(&preferred)
        };
        if !ok {
            continue;
        }
        // `maxOrNull()`: the best COMPARATOR element; rank is smaller-is-better.
        if best.map_or(true, |(_, r)| rank < r) {
            best = Some((slot.inventory_slot, rank));
        }
    }
    best.map(|(slot, _)| slot)
}

/// Upstream `attackHandler`.
pub fn auto_weapon_attack_handler(
    module: &Module,
    player: &AwPlayer,
    hooks: &mut AwHooks,
    target: &AwAttackTarget,
) {
    if !target.is_living_entity {
        return;
    }
    let Some(weapon_slot) =
        aw_determine_weapon_slot(module, player, hooks, Some(target), false)
    else {
        return;
    };

    if aw_is_busy(player) || !aw_change_on_has(module, "OnAttack") {
        return;
    }

    hooks.selections.push((weapon_slot, aw_switch_back(module)));
    // [ClientPlayerInteractionManager.attackEntity] will sync the selected
    // slot, so we can do that here already.
    *hooks.ensure_sent_carried_item_calls += 1;
}

/// Upstream `onTarget(entity)`: prepare AutoWeapon for the given entity.
pub fn auto_weapon_on_target(
    module: &Module,
    player: &AwPlayer,
    hooks: &mut AwHooks,
    target: Option<&AwAttackTarget>,
) {
    if !module.running
        || !target.map(|t| t.is_living_entity).unwrap_or(false)
        || aw_is_busy(player)
        || !aw_change_on_has(module, "OnTarget")
    {
        return;
    }

    if let Some(slot) = aw_determine_weapon_slot(module, player, hooks, target, false) {
        hooks.selections.push((slot, aw_switch_back(module)));
    }
}

/// Upstream `onDisabled`.
pub fn auto_weapon_on_disabled(hooks: &mut AwHooks) {
    *hooks.reset_slot_calls += 1; // SilentHotbar.resetSlot(this)
}

/// Upstream `willShieldBreak`.
pub fn auto_weapon_will_shield_break(
    module: &Module,
    player: &AwPlayer,
    hooks: &mut AwHooks,
) -> bool {
    if hooks.is_older_or_equal_1_8 {
        return false;
    }

    // If we have an axe in our main hand, we will break the shield
    if player.main_hand.is_axe {
        return true;
    }

    // If we are not going to switch to an axe, we will not break the shield
    aw_determine_weapon_slot(module, player, hooks, None, true).map_or(false, |slot| {
        hooks
            .inventory
            .hotbar()
            .into_iter()
            .find(|s| s.inventory_slot == slot)
            .map_or(false, |s| s.stack.is_axe)
    })
}

/// Upstream `willMaceSmash`.
pub fn auto_weapon_will_mace_smash(
    module: &Module,
    player: &AwPlayer,
    hooks: &mut AwHooks,
) -> bool {
    if !aw_can_mace_smash(player, hooks) {
        return false;
    }

    if player.main_hand.is_mace {
        return true;
    }

    aw_determine_weapon_slot(module, player, hooks, None, false).map_or(false, |slot| {
        hooks
            .inventory
            .hotbar()
            .into_iter()
            .find(|s| s.inventory_slot == slot)
            .map_or(false, |s| s.stack.is_mace)
    })
}

/// Upstream `getAttackSpeed(original)`: the attack speed of the determined
/// weapon, or `original` if no weapon is selected or ChangeOn lacks OnAttack.
pub fn auto_weapon_get_attack_speed(
    module: &Module,
    player: &AwPlayer,
    hooks: &mut AwHooks,
    original: f64,
) -> f64 {
    if !module.running || !aw_change_on_has(module, "OnAttack") || !player.has_cooldown {
        return original;
    }

    let Some(slot) = aw_determine_weapon_slot(module, player, hooks, None, false) else {
        return original;
    };
    hooks
        .inventory
        .hotbar()
        .into_iter()
        .find(|s| s.inventory_slot == slot)
        .map_or(original, |s| s.stack.attack_speed)
}

pub fn auto_weapon() -> Module {
    Module::new(
        "AutoWeapon",
        COMBAT,
        vec![
            multi_choice(
                "Preferred",
                &["Sword"],
                &[
                    "Any", "Sword", "Axe", "Mace", "Spear", "Pickaxe", "Shovel", "Hoe",
                    "Knockback", "FireAspect",
                ],
            ),
            choice("Priority", "Default", &["Default", "Knockback", "Damage", "AttackSpeed"]),
            boolean("AutoShieldBreak", true),
            boolean("AutoMace", true),
            int("SwitchBack", 20, 1, 300),
            multi_choice("ChangeOn", &["OnAttack"], &["OnAttack", "OnTarget"]),
        ],
    )
}

// ----- Seams + ModuleAimbot.kt -----

/// Upstream `Entity` slice Aimbot reads.
#[derive(Clone, Debug)]
pub struct AbEntity {
    pub is_end_crystal: bool,
    pub is_living_entity: bool,
    /// `RotationUtil.crosshairCosToEntity(entity)`, the crystals sort key.
    pub crosshair_cos: f64,
}

/// Upstream `PointTracker.findPoint` result slice (pos + box).
#[derive(Clone, Copy, Debug)]
pub struct AbPoint {
    pub pos: Vec3,
    pub box_min: Vec3,
    pub box_max: Vec3,
}

/// Upstream `RotationWithVector` (rotation + hit location).
#[derive(Clone, Copy, Debug)]
pub struct AbRotationWithVector {
    pub rotation: Rotation,
    pub location: Vec3,
}

/// Engine seams Aimbot needs. `targets()` returns the tracker's entries in
/// its standing priority order (DIRECTION, range-bounded by `Range`).
pub trait AbWorld {
    /// `targetTracker.targets()`
    fn targets(&mut self) -> Vec<AbEntity>;
    /// `world.getEntitiesBoxInRange(player.eyePosition, maxRange) { it is EndCrystal && targetTracker.isInFov(it) }`
    fn crystals_in_range(&mut self, eyes: Vec3, max_range: f64) -> Vec<AbEntity>;
    /// `isLookingAtEntity(fromEntity, toEntity, rotation, range, throughWallsRange)`
    fn look_at_hit(
        &mut self,
        entity: &AbEntity,
        rotation: Rotation,
        range: f64,
        through_walls_range: f64,
    ) -> Option<Vec3>;
    /// `PointTracker.findPoint(eyes, entity)`
    fn find_point(&mut self, eyes: Vec3, entity: &AbEntity) -> Option<AbPoint>;
    /// `raytraceBox(eyes, point.box, range, wallsRange, rotationPreference)`
    /// with `LeastDifferencePreference.leastDifferenceToLastPoint` folded in.
    fn raytrace_box(
        &mut self,
        eyes: Vec3,
        point: &AbPoint,
        range: f64,
        walls_range: f64,
    ) -> Option<AbRotationWithVector>;
}

/// Upstream angle-smooth processors (`angleSmooth.activeMode.process(...)`),
/// one seam call per rotation update.
pub trait AbAngleSmooth {
    fn process(
        &mut self,
        mode: &str,
        player_rotation: Rotation,
        target_rotation: Rotation,
    ) -> Option<Rotation>;
}

/// Upstream `RotationUtil.applyMouseTurnDelta(rotation, cursorDeltaX, cursorDeltaY)`.
pub trait AbMouseDelta {
    fn apply(&mut self, rotation: Rotation, delta_x: f64, delta_y: f64) -> Rotation;
}

pub struct AbHooks<'a> {
    pub world: &'a mut dyn AbWorld,
    pub angle_smooth: &'a mut dyn AbAngleSmooth,
    pub mouse_delta: &'a mut dyn AbMouseDelta,
    /// `mc.gui.screen()`
    pub screen_open: bool,
    /// `InventoryManager.isInventoryOpen`
    pub inventory_open: bool,
    /// `mc.gui.screen() is AbstractContainerScreen<*>`
    pub container_screen_open: bool,
    /// `mc.options.keyUse.isPressedOnAny`
    pub use_key_pressed: bool,
    /// `Timer.timerSpeed`
    pub timer_speed: f32,
    /// `player.rotation`
    pub player_rotation_raw: Rotation,
    /// `player.eyePosition`
    pub eye_position: Vec3,
    /// `ModuleAutoWeapon.onTarget(entity)` calls with a living target set.
    pub on_target_calls: &'a mut usize,
    /// `player.setRotation(...)` calls, in order.
    pub set_rotations: &'a mut Vec<Rotation>,
}

#[derive(Clone, Debug, Default)]
pub struct AimbotState {
    pub target_rotation: Option<Rotation>,
    pub player_rotation: Option<Rotation>,
    /// `targetTracker.target` standing target (tracker-maintained).
    pub target_set: bool,
}

pub fn aimbot_state() -> AimbotState {
    AimbotState::default()
}

fn ab_range(module: &Module) -> f64 {
    match module.setting("Range").map(|v| &v.kind) {
        Some(ValueKind::Float(v, _, _)) => *v,
        _ => 4.2,
    }
}

/// Upstream `WallRange` with its `onChange { minOf(range.get(), it) }` clamp.
fn ab_wall_range(module: &Module) -> f64 {
    let wall = match module.setting("WallRange").map(|v| &v.kind) {
        Some(ValueKind::Float(v, _, _)) => *v,
        _ => 0.0,
    };
    wall.min(ab_range(module))
}

fn ab_lazy_rotation(module: &Module) -> bool {
    match module.setting("LazyRotation").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => false,
    }
}

fn ab_crystals_enabled(module: &Module) -> bool {
    match module.setting("Crystals").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => false,
    }
}

fn ab_crystals_only(module: &Module) -> bool {
    match module.setting("CrystalsOnly").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => false,
    }
}

fn ab_crystals_only_while_using(module: &Module) -> bool {
    match module.setting("CrystalsOnlyWhileUsing").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => false,
    }
}

fn ab_axis_has(module: &Module, tag: &str) -> bool {
    match module.setting("Axis").map(|v| &v.kind) {
        Some(ValueKind::MultiChoice(sel, _)) => sel.iter().any(|t| t == tag),
        _ => false,
    }
}

fn ab_ignore_has(module: &Module, tag: &str) -> bool {
    match module.setting("Ignore").map(|v| &v.kind) {
        Some(ValueKind::MultiChoice(sel, _)) => sel.iter().any(|t| t == tag),
        _ => false,
    }
}

fn ab_angle_smooth(module: &Module) -> String {
    match module.setting("AngleSmooth").and_then(|v| v.kind.choice()) {
        Some(s) => s.to_string(),
        None => "InterpolationAngleSmooth".to_string(),
    }
}

/// Upstream `requirementsMet`.
fn ab_requirements_met(module: &Module, hooks: &AbHooks, inputs: &KillAuraRequirementInputs) -> bool {
    !hooks.screen_open && killaura_requirements_met(module, inputs)
}

/// Upstream `Crystals.targets()`.
fn ab_crystal_targets(module: &Module, hooks: &mut AbHooks) -> Vec<AbEntity> {
    if ab_crystals_only_while_using(module) && !hooks.use_key_pressed {
        return Vec::new();
    }

    let mut crystals = hooks
        .world
        .crystals_in_range(hooks.eye_position, ab_range(module));
    // `crystals.sortByDescending(RotationUtil::crosshairCosToEntity)`
    crystals.sort_by(|a, b| {
        b.crosshair_cos
            .partial_cmp(&a.crosshair_cos)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    crystals
}

/// Upstream `targets()`.
fn ab_targets(module: &Module, hooks: &mut AbHooks) -> Vec<AbEntity> {
    if !ab_crystals_enabled(module) {
        return hooks.world.targets();
    }

    let mut targets = ab_crystal_targets(module, hooks);
    if !ab_crystals_only(module) {
        targets.extend(hooks.world.targets());
    }
    targets
}

/// Upstream `findNextTargetRotation()`.
pub fn ab_find_next_target_rotation(
    module: &Module,
    state: &mut AimbotState,
    hooks: &mut AbHooks,
) -> Option<(AbEntity, AbRotationWithVector)> {
    for entity in ab_targets(module, hooks) {
        if ab_lazy_rotation(module) {
            let current_rotation = hooks.player_rotation_raw;
            let current_hit = hooks.world.look_at_hit(
                &entity,
                current_rotation,
                ab_range(module),
                ab_wall_range(module),
            );

            if let Some(location) = current_hit {
                state.target_set = true; // targetTracker.target = entity
                return Some((
                    entity,
                    AbRotationWithVector { rotation: current_rotation, location },
                ));
            }
        }

        let eyes = hooks.eye_position;
        let Some(point) = hooks.world.find_point(eyes, &entity) else {
            continue;
        };

        let Some(rotation) = hooks
            .world
            .raytrace_box(eyes, &point, ab_range(module), ab_wall_range(module))
        else {
            continue;
        };

        state.target_set = true; // targetTracker.target = entity
        return Some((entity, rotation));
    }

    state.target_set = false; // targetTracker.reset()
    None
}

/// Upstream `tickHandler` (`RotationUpdateEvent`).
pub fn aimbot_rotation_update(
    module: &Module,
    state: &mut AimbotState,
    hooks: &mut AbHooks,
    inputs: &KillAuraRequirementInputs,
) {
    state.player_rotation = Some(hooks.player_rotation_raw);

    if !ab_requirements_met(module, hooks, inputs) {
        state.target_set = false; // targetTracker.reset()
        state.target_rotation = None;
        return;
    }

    state.target_rotation = ab_find_next_target_rotation(module, state, hooks).and_then(
        |(_target, rotation)| {
            hooks
                .angle_smooth
                .process(&ab_angle_smooth(module), hooks.player_rotation_raw, rotation.rotation)
        },
    );

    // Update Auto Weapon
    if state.target_set {
        *hooks.on_target_calls += 1;
    }
}

/// Upstream `renderHandler` (`WorldRenderEvent`).
pub fn aimbot_render(
    module: &Module,
    state: &AimbotState,
    partial_ticks: f32,
    hooks: &mut AbHooks,
) {
    if !ab_ignore_has(module, "Screen") && hooks.screen_open {
        return;
    }

    if !ab_ignore_has(module, "Container")
        && (hooks.inventory_open || hooks.container_screen_open)
    {
        return;
    }

    aimbot_look_at(module, state, partial_ticks, hooks);
}

/// Upstream `lookAt(partialTicks)`: looks at the target rotation with
/// interpolation based on the timer speed and partial ticks.
pub fn aimbot_look_at(
    module: &Module,
    state: &AimbotState,
    partial_ticks: f32,
    hooks: &mut AbHooks,
) {
    let (Some(player_rotation), Some(target_rotation)) =
        (state.player_rotation, state.target_rotation)
    else {
        return;
    };
    let timer_speed = hooks.timer_speed;
    let interpolated = player_rotation.interpolate_to(target_rotation, timer_speed * partial_ticks);

    let yaw = if ab_axis_has(module, "Horizontal") {
        interpolated.yaw
    } else {
        player_rotation.yaw
    };
    let pitch = if ab_axis_has(module, "Vertical") {
        interpolated.pitch
    } else {
        player_rotation.pitch
    };
    hooks.set_rotations.push(Rotation { yaw, pitch });
}

/// Upstream `mouseMovement` (`MouseRotationEvent`).
pub fn aimbot_mouse_movement(
    state: &mut AimbotState,
    cursor_delta_x: f64,
    cursor_delta_y: f64,
    hooks: &mut AbHooks,
) {
    if let Some(rotation) = state.player_rotation {
        state.player_rotation = Some(hooks.mouse_delta.apply(rotation, cursor_delta_x, cursor_delta_y));
    }

    if let Some(rotation) = state.target_rotation {
        state.target_rotation = Some(hooks.mouse_delta.apply(rotation, cursor_delta_x, cursor_delta_y));
    }
}

/// Upstream `onDisabled`.
pub fn aimbot_on_disabled(state: &mut AimbotState) {
    state.target_set = false; // targetTracker.reset()
    state.target_rotation = None;
}

pub fn aimbot() -> Module {
    Module::new(
        "Aimbot", // aliases AimAssist / AutoAim
        COMBAT,
        vec![
            float("Range", 4.2, 1.0, 8.0),
            float("WallRange", 0.0, 0.0, 8.0),
            boolean("LazyRotation", false),
            // ToggleableValueGroup `Crystals` flattened to booleans.
            boolean("Crystals", false),
            boolean("CrystalsOnly", false),
            boolean("CrystalsOnlyWhileUsing", false),
            multi_choice(
                "Requires",
                &[],
                &["Click", "MouseMovement", "Weapon", "EmptyHand", "VanillaName", "NotBreaking"],
            ),
            choice(
                "AngleSmooth",
                "InterpolationAngleSmooth",
                &["InterpolationAngleSmooth", "SigmoidAngleSmooth", "LinearAngleSmooth"],
            ),
            multi_choice("Axis", &["Horizontal", "Vertical"], &["Horizontal", "Vertical"]),
            multi_choice("Ignore", &[], &["Screen", "Container"]),
        ],
    )
}

// ----- Seams + ModuleAutoShoot.kt -----

/// Upstream `LivingEntity` target slice AutoShoot reads.
#[derive(Clone, Copy, Debug)]
pub struct AsTarget;

/// Upstream `HotbarItemSlot` slice AutoShoot reads.
#[derive(Clone, Debug)]
pub struct AsSlot {
    /// `slot.hotbarIndex` (null for offhand slots).
    pub hotbar_index: Option<i64>,
    /// `SilentHotbar.serversideSlot`
    pub serverside_slot: i64,
    /// The held item's registry id, e.g. "minecraft:egg".
    pub item: String,
    /// `it.itemStack.isEmpty`
    pub is_empty: bool,
    /// `slot.useHand` ("Main" / "Off").
    pub use_hand: String,
}

impl AsSlot {
    /// Upstream `HotbarItemSlot.isSelectionNeeded`.
    fn is_selection_needed(&self) -> bool {
        self.hotbar_index.map_or(false, |idx| idx != self.serverside_slot)
    }
}

/// The `findClosestSlot` predicate of the `ThrowableType` mode, serialized.
#[derive(Clone, Debug)]
pub enum AsThrowableMode {
    /// `Slots.OffhandWithHotbar.findClosestSlot { it.item is EggItem || it.item is SnowballItem }`
    EggAndSnowball,
    /// `Slots.OffhandWithHotbar.findClosestSlot { !it.isEmpty && filter(it.item, items) }`
    /// with the mode's `Filter`/`Items` settings.
    Custom { filter: String, items: Vec<String> },
}

impl AsThrowableMode {
    /// Upstream `Filter(item, items)`: Whitelist = contained, Blacklist = not.
    fn matches(&self, slot: &AsSlot) -> bool {
        match self {
            AsThrowableMode::EggAndSnowball => {
                slot.item == "minecraft:egg" || slot.item == "minecraft:snowball"
            }
            AsThrowableMode::Custom { filter, items } => {
                let contains = items.iter().any(|it| *it == slot.item);
                match filter.as_str() {
                    "Blacklist" => !slot.is_empty && !contains,
                    _ => !slot.is_empty && contains,
                }
            }
        }
    }
}

pub trait AsInventory {
    /// `Slots.OffhandWithHotbar.findClosestSlot { predicate }`; the engine owns
    /// the scan order and slot identity.
    fn find_closest_slot(&mut self, mode: &AsThrowableMode) -> Option<AsSlot>;
}

/// Engine seams AutoShoot needs.
pub trait AsWorld {
    /// `targetTracker.selectFirst(player::hasLineOfSight)`: the first target
    /// with line of sight; the tracker's `.target` follows the selection.
    fn select_first_with_los(&mut self) -> Option<AsTarget>;
    /// `player.eyePosition`
    fn eye_position(&mut self) -> Vec3;
    /// `PointTracker.findPoint(eyes, target, 1)`
    fn find_point(&mut self, eyes: Vec3, target: &AsTarget) -> Option<Vec3>;
    /// `SituationalProjectileAngleCalculator.calculateAngleForEntity(TrajectoryInfo.GENERIC, target)`
    fn projectile_angle(&mut self, target: &AsTarget) -> Option<Rotation>;
}

/// Upstream `SilentHotbar` + `Clicker` + `useItem` seams.
pub trait AsHotbar {
    /// `SilentHotbar.selectSlotSilently(this, slot, tickUntilReset)`; returns
    /// the post-select re-check of `isSelectionNeeded` (true = still not held).
    fn select_slot_silently(&mut self, slot: &AsSlot, tick_until_reset: i64) -> bool;
    /// `SilentHotbar.resetSlot(ModuleAutoShoot)`
    fn reset_slot(&mut self);
    /// `useItem(slot.useHand, swingMode).consumesAction()`
    fn use_item(&mut self, slot: &AsSlot, swing_mode: &str) -> bool;
    /// `Clicker.click`: whether the clicker allowed a click this tick.
    fn click_allowed(&mut self) -> bool;
}

pub struct AsHooks<'a> {
    pub world: &'a mut dyn AsWorld,
    pub hotbar: &'a mut dyn AsHotbar,
    pub inventory: &'a mut dyn AsInventory,
    /// `RotationManager.serverRotation`
    pub server_rotation: Rotation,
    /// `RotationManager.setRotationTarget` calls, in order.
    pub rotation_targets: &'a mut Vec<RotationTargetCall>,
    /// `CombatManager.isInCombat`
    pub in_combat: bool,
    /// `player.isUsingItem`
    pub player_is_using_item: bool,
    /// `InventoryManager.isInventoryOpen`
    pub inventory_open: bool,
    /// `ModuleKillAura.running`
    pub kill_aura_running: bool,
}

#[derive(Clone, Debug, Default)]
pub struct AutoShootState {
    /// `targetTracker.target` standing target (tracker-maintained).
    pub target: Option<AsTarget>,
}

pub fn auto_shoot_state() -> AutoShootState {
    AutoShootState::default()
}

fn as_select_slot_automatically(module: &Module) -> bool {
    match module.setting("SelectSlotAutomatically").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => true,
    }
}

fn as_tick_until_reset(module: &Module) -> i64 {
    match module.setting("TicksUntilSlotReset").map(|v| &v.kind) {
        Some(ValueKind::Int(v, _, _)) => *v,
        _ => 1,
    }
}

fn as_consider_inventory(module: &Module) -> bool {
    match module.setting("ConsiderInventory").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => true,
    }
}

fn as_requires_kill_aura(module: &Module) -> bool {
    match module.setting("RequiresKillAura").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => false,
    }
}

fn as_not_during_combat(module: &Module) -> bool {
    match module.setting("NotDuringCombat").map(|v| &v.kind) {
        Some(ValueKind::Bool(v)) => *v,
        _ => false,
    }
}

fn as_aim_off_threshold(module: &Module) -> f32 {
    match module.setting("AimOffThreshold").map(|v| &v.kind) {
        Some(ValueKind::Float(v, _, _)) => *v as f32,
        _ => 2.0,
    }
}

fn as_swing_mode(module: &Module) -> ArSwingMode {
    match module.setting("SwingMode").and_then(|v| v.kind.choice()) {
        Some("HideForClient") => ArSwingMode::HideForClient,
        _ => ArSwingMode::DoNotHide,
    }
}

/// Upstream `HotbarItemSlot.trySelect`.
fn as_try_select(module: &Module, slot: &AsSlot, hooks: &mut AsHooks) -> bool {
    if slot.is_selection_needed() {
        // Select the slot if we are not holding it.
        if !as_select_slot_automatically(module) {
            return false;
        }
        // If we are not holding the slot, we can't shoot.
        let still_needed =
            hooks.hotbar.select_slot_silently(slot, as_tick_until_reset(module));
        if still_needed {
            return false;
        }
    }
    true
}

/// Upstream `shouldPauseForKillAura`.
fn as_should_pause_for_kill_aura(module: &Module, state: &mut AutoShootState, hooks: &AsHooks) -> bool {
    if as_requires_kill_aura(module) && !hooks.kill_aura_running {
        state.target = None; // targetTracker.reset()
        return true;
    }
    false
}

/// Upstream `getThrowableSlot`.
fn as_throwable_slot(module: &Module, hooks: &mut AsHooks) -> Option<AsSlot> {
    let mode = as_throwable_mode(module);
    let slot = hooks.inventory.find_closest_slot(&mode)?;
    if as_try_select(module, &slot, hooks) {
        Some(slot)
    } else {
        None
    }
}

/// Upstream `throwableType.activeMode` (Custom mode carries its settings).
fn as_throwable_mode(module: &Module) -> AsThrowableMode {
    let kind = module.setting("ThrowableType").and_then(|v| v.kind.choice());
    if kind == Some("Custom") {
        let filter = module
            .setting("Filter")
            .and_then(|v| v.kind.choice())
            .unwrap_or("Whitelist")
            .to_string();
        let items = module
            .setting("Items")
            .map(|v| &v.kind)
            .and_then(ValueKind::items)
            .cloned()
            .unwrap_or_default();
        AsThrowableMode::Custom { filter, items }
    } else {
        AsThrowableMode::EggAndSnowball
    }
}

/// Upstream `GravityType.from(slot)`: Auto resolves by the held item
/// (egg/snowball → Projectile, else Linear), other tags pass through.
fn as_rotation_for(
    module: &Module,
    hooks: &mut AsHooks,
    target: &AsTarget,
    slot: &AsSlot,
) -> Option<Rotation> {
    let setting = module
        .setting("GravityType")
        .and_then(|v| v.kind.choice())
        .unwrap_or("Auto");
    let gravity: String = match setting {
        "Auto" => {
            if slot.item == "minecraft:egg" || slot.item == "minecraft:snowball" {
                "Projectile".to_string()
            } else {
                "Linear".to_string()
            }
        }
        other => other.to_string(),
    };
    match gravity.as_str() {
        "Projectile" => hooks.world.projectile_angle(target),
        "Linear" => {
            // On linear we likely don't need to care about gravity, but
            // instead aim exactly at the hitbox of the target.
            let eyes = hooks.world.eye_position();
            let point = hooks.world.find_point(eyes, target)?;
            Some(Rotation::looking_at(point, eyes))
        }
        _ => None,
    }
}

/// Upstream `simulatedTickHandler` (`RotationUpdateEvent`).
pub fn auto_shoot_rotation_update(module: &Module, state: &mut AutoShootState, hooks: &mut AsHooks) {
    if as_should_pause_for_kill_aura(module, state, hooks) {
        return;
    }

    // Find the recommended target. Check if we can see the enemy.
    state.target = hooks.world.select_first_with_los();
    let Some(target) = state.target else {
        return;
    };

    if as_not_during_combat(module) && hooks.in_combat {
        return;
    }

    // Check if we have a throwable, if not we can't shoot.
    let Some(slot) = as_throwable_slot(module, hooks) else {
        return;
    };
    let Some(rotation) = as_rotation_for(module, hooks, &target, &slot) else {
        return;
    };

    // Set the rotation with the usage priority of 2.
    hooks.rotation_targets.push(RotationTargetCall {
        rotation,
        consider_inventory: as_consider_inventory(module),
    });
}

/// Upstream `handleAutoShoot` (`tickHandler`).
pub fn auto_shoot_tick(module: &Module, state: &mut AutoShootState, hooks: &mut AsHooks) {
    if as_should_pause_for_kill_aura(module, state, hooks) {
        return;
    }

    let Some(target) = state.target else {
        return;
    };

    if as_not_during_combat(module) && hooks.in_combat {
        return;
    }

    // Check if we have a throwable, if not we can't shoot.
    let Some(slot) = as_throwable_slot(module, hooks) else {
        return;
    };
    let Some(rotation) = as_rotation_for(module, hooks, &target, &slot) else {
        return;
    };

    // Check the difference between server and client rotation.
    let rotation_difference = hooks.server_rotation.direction_angle_to(rotation);

    // Check if we are not aiming at the target yet.
    if rotation_difference > as_aim_off_threshold(module) {
        return;
    }

    // Check if we are still aiming at the target.
    if hooks.hotbar.click_allowed() {
        if hooks.player_is_using_item || (as_consider_inventory(module) && hooks.inventory_open) {
            return;
        }
        hooks
            .hotbar
            .use_item(&slot, &as_swing_mode(module).to_string());
    }
}

/// Upstream `onDisabled`.
pub fn auto_shoot_on_disabled(state: &mut AutoShootState, hooks: &mut AsHooks) {
    state.target = None; // targetTracker.reset()
    hooks.hotbar.reset_slot(); // SilentHotbar.resetSlot(ModuleAutoShoot)
}

pub fn auto_shoot() -> Module {
    Module::new(
        "AutoShoot",
        COMBAT,
        vec![
            choice("ThrowableType", "EggAndSnowball", &["EggAndSnowball", "Custom"]),
            choice("GravityType", "Auto", &["Auto", "Linear", "Projectile"]),
            // Clicker(this, mc.options.keyUse, itemCooldown = null) group.
            boolean("Clicker", true),
            choice("Technique", "Human", &["Human", "Constant"]),
            int_range("CPS", 11, 14, 1, 20),
            int("MaxPerTick", 2, 1, 5),
            float_range("Range", 3.0, 6.0, 1.0, 256.0),
            float("AimOffThreshold", 2.0, 0.5, 10.0),
            choice("SwingMode", "DoNotHide", &["DoNotHide", "HideForClient"]),
            boolean("SelectSlotAutomatically", true),
            int("TicksUntilSlotReset", 1, 0, 20),
            boolean("ConsiderInventory", true),
            boolean("RequiresKillAura", false),
            boolean("NotDuringCombat", false),
            boolean("ConstantLag", false),
            // Custom mode's settings.
            choice("Filter", "Whitelist", &["Whitelist", "Blacklist"]),
            items("Items", &["minecraft:egg", "minecraft:snowball"]),
        ],
    )
}

#[cfg(test)]
mod tests6 {
    use super::*;

    fn json_str(s: &str) -> Json {
        Json::Str(s.to_string())
    }

    fn json_items(entries: &[&str]) -> Json {
        Json::Arr(entries.iter().map(|s| Json::Str(s.to_string())).collect())
    }

    fn req_inputs(click: bool, mouse: bool, weapon: bool, empty: bool, custom: bool, not_breaking: bool) -> KillAuraRequirementInputs {
        KillAuraRequirementInputs {
            click_pressed: click,
            mouse_moved_recently: mouse,
            main_hand_is_weapon: weapon,
            main_hand_is_empty: empty,
            main_hand_has_custom_name: custom,
            not_breaking,
        }
    }

    fn all_req_inputs() -> KillAuraRequirementInputs {
        req_inputs(true, true, true, false, false, true)
    }

    // ----- Rotation math -----

    #[test]
    fn wrap_degrees_wraps_into_180() {
        assert_eq!(wrap_degrees(0.0), 0.0);
        assert_eq!(wrap_degrees(360.0), 0.0);
        assert_eq!(wrap_degrees(190.0), -170.0);
        assert_eq!(wrap_degrees(-190.0), 170.0);
    }

    #[test]
    fn rotation_from_vec_matches_upstream() {
        // diff = (+X): yaw = atan2(0,1)*deg - 90 = -90, pitch = -atan2(0, 1) = 0
        let r = rotation_from_vec(1.0, 0.0, 0.0);
        assert_eq!(r.yaw, -90.0);
        assert_eq!(r.pitch, 0.0);

        // diff = (+Z): yaw = atan2(1,0)*deg - 90 = 0, pitch = -atan2(0,1) = 0
        let r = rotation_from_vec(0.0, 0.0, 1.0);
        assert_eq!(r.yaw, 0.0);
        assert_eq!(r.pitch, 0.0);

        // diff = straight up: pitch = -atan2(1, 0) = -90
        let r = rotation_from_vec(0.0, 1.0, 0.0);
        assert_eq!(r.pitch, -90.0);
    }

    #[test]
    fn looking_at_matches_from_rotation_vec() {
        let from = Vec3 { x: 0.0, y: 0.0, z: 0.0 };
        let point = Vec3 { x: 3.0, y: 1.0, z: 4.0 };
        assert_eq!(Rotation::looking_at(point, from), rotation_from_vec(3.0, 1.0, 4.0));
    }

    #[test]
    fn direction_vector_faces_plus_x_for_yaw_270() {
        // yaw = -90 (== 270) faces +X at pitch 0
        let d = Rotation { yaw: -90.0, pitch: 0.0 }.direction_vector();
        assert!((d.x - 1.0).abs() < 1e-6, "x = {}", d.x);
        assert!(d.y.abs() < 1e-6);
        assert!(d.z.abs() < 1e-6);
    }

    #[test]
    fn direction_angle_to_measures_the_yaw_gap() {
        let facing_plus_x = Rotation { yaw: -90.0, pitch: 0.0 };
        let facing_plus_z = Rotation { yaw: 0.0, pitch: 0.0 };
        let angle = facing_plus_x.direction_angle_to(facing_plus_z);
        assert!((angle - 90.0).abs() < 1e-4, "angle = {}", angle);
    }

    #[test]
    fn interpolate_to_is_fma() {
        let a = Rotation { yaw: 10.0, pitch: 0.0 };
        let b = Rotation { yaw: 20.0, pitch: -10.0 };
        let half = a.interpolate_to(b, 0.5);
        assert_eq!(half.yaw, 15.0);
        assert_eq!(half.pitch, -5.0);
    }

    // ----- KillAuraRequirements -----

    #[test]
    fn killaura_tags_roundtrip() {
        for req in [
            KillAuraRequirement::Click,
            KillAuraRequirement::MouseMovement,
            KillAuraRequirement::Weapon,
            KillAuraRequirement::EmptyHand,
            KillAuraRequirement::VanillaName,
            KillAuraRequirement::NotBreaking,
        ] {
            assert_eq!(KillAuraRequirement::from_tag(req.tag()), Some(req));
        }
        assert_eq!(KillAuraRequirement::from_tag("Nope"), None);
    }

    #[test]
    fn killaura_as_boolean_matches_upstream_predicates() {
        let inputs = req_inputs(true, true, true, true, true, false);
        assert!(killaura_requirement_as_boolean(KillAuraRequirement::Click, &inputs));
        assert!(killaura_requirement_as_boolean(KillAuraRequirement::EmptyHand, &inputs));
        // VanillaName = customName == null
        assert!(!killaura_requirement_as_boolean(KillAuraRequirement::VanillaName, &inputs));
        // NotBreaking = isDestroying == false
        assert!(!killaura_requirement_as_boolean(KillAuraRequirement::NotBreaking, &inputs));
    }

    #[test]
    fn killaura_requirements_met_needs_every_selected_tag() {
        let module = auto_rod();
        // Defaults: all tags except EmptyHand.
        let inputs = all_req_inputs();
        assert!(killaura_requirements_met(&module, &inputs));

        let inputs = req_inputs(false, true, true, false, false, true);
        assert!(!killaura_requirements_met(&module, &inputs));
    }

    // ----- value.rs items() -----

    #[test]
    fn items_setting_defaults_and_json() {
        let mut module = auto_rod();
        let v = module.setting_mut("HoldingItemsForIgnore").unwrap();
        assert!(matches!(v.kind, ValueKind::Items(_)));
        let items = v.kind.items().unwrap();
        assert_eq!(items.len(), 5);
        assert!(items.contains(&"minecraft:ender_pearl".to_string()));

        v.set_from_json(&json_items(&["minecraft:bow", "minecraft:bow", "minecraft:trident"])).unwrap();
        let items = v.kind.items().unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.contains(&"minecraft:trident".to_string()));
        assert!(v.set_from_json(&Json::Arr(vec![Json::Num(1.0), Json::Num(2.0)])).is_err());
    }

    // ----- AutoRod -----

    struct ArMock {
        targets: Vec<ArTarget>,
        distances: Vec<f64>,
        los: Vec<bool>,
        bobber: Option<ArFishingBobber>,
        rod_slot: Option<ArRodSlot>,
        point: Option<Vec3>,
        angle: Option<Rotation>,
    }

    impl ArWorld for ArMock {
        fn targets(&mut self) -> Vec<ArTarget> {
            self.targets.clone()
        }
        fn distance_to_sqr(&mut self, target: &ArTarget) -> f64 {
            self.distances[self.targets.iter().position(|t| t == target).unwrap()]
        }
        fn find_own_bobber(&mut self) -> Option<ArFishingBobber> {
            self.bobber
        }
        fn find_rod_slot(&mut self) -> Option<ArRodSlot> {
            self.rod_slot
        }
        fn has_line_of_sight(&mut self, target: &ArTarget) -> bool {
            self.los[self.targets.iter().position(|t| t == target).unwrap()]
        }
        fn find_point(&mut self, _eyes: Vec3, _target: &ArTarget) -> Option<Vec3> {
            self.point
        }
        fn projectile_angle(&mut self, _target: &ArTarget) -> Option<Rotation> {
            self.angle
        }
    }

    #[derive(Default)]
    struct ArHotbarMock {
        use_calls: Vec<(i64, String)>,
        releases: usize,
        resets: usize,
    }

    impl ArHotbar for ArHotbarMock {
        fn use_hotbar_slot_or_offhand(
            &mut self,
            _slot: &ArRodSlot,
            ticks_until_reset: i64,
            swing_mode: &str,
        ) -> bool {
            self.use_calls.push((ticks_until_reset, swing_mode.to_string()));
            true
        }
        fn release_using_item_in_tick_loop(&mut self) {
            self.releases += 1;
        }
        fn reset_slot(&mut self) {
            self.resets += 1;
        }
    }

    fn ar_setup() -> (ArMock, ArHotbarMock) {
        (
            ArMock {
                targets: vec![ArTarget { health: 8.0 }],
                distances: vec![16.0], // in 3.5..5 range? -> 4.0
                los: vec![true],
                bobber: None,
                rod_slot: Some(ArRodSlot { is_selected: false, is_fishing_rod: true, inventory_slot: 3 }),
                point: Some(Vec3 { x: 2.0, y: 0.0, z: 0.0 }),
                angle: None,
            },
            ArHotbarMock::default(),
        )
    }

    fn ar_player() -> ArPlayer {
        ArPlayer {
            eye_position: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
            health: 20.0,
            main_hand_item: "minecraft:sword".to_string(),
            is_using_item: false,
            main_hand_is_consumable: false,
            offhand_is_consumable: false,
        }
    }

    fn ar_hooks<'a>(
        mock: &'a mut ArMock,
        hotbar: &'a mut ArHotbarMock,
        targets_out: &'a mut Vec<RotationTargetCall>,
        debug: &'a mut Vec<String>,
    ) -> ArHooks<'a> {
        ArHooks {
            world: mock,
            hotbar,
            server_rotation: Rotation { yaw: 0.0, pitch: 0.0 },
            rotation_targets: targets_out,
            blink_running: false,
            scaffold_running: false,
            freeze_running: false,
            inventory_open: false,
            container_screen_open: false,
            debug,
        }
    }

    #[test]
    fn auto_rod_rotation_update_selects_first_in_range_target() {
        let module = auto_rod();
        let mut state = auto_rod_state(0.0);
        let (mut mock, mut hotbar) = ar_setup();
        let mut targets_out = Vec::new();
        let mut debug = Vec::new();
        {
            let mut hooks = ar_hooks(&mut mock, &mut hotbar, &mut targets_out, &mut debug);
            auto_rod_rotation_update(&module, &mut state, &ar_player(), &mut hooks, &all_req_inputs());
        }
        assert_eq!(state.target, Some(ArTarget { health: 8.0 }));
        assert_eq!(targets_out.len(), 1);
        assert!(!targets_out[0].consider_inventory);
        // eyes at origin, point (2,0,0): yaw -90, pitch 0
        assert_eq!(targets_out[0].rotation.yaw, -90.0);
    }

    #[test]
    fn auto_rod_requirements_gate_blocks_the_tick() {
        let module = auto_rod();
        let mut state = auto_rod_state(0.0);
        let (mut mock, mut hotbar) = ar_setup();
        let mut targets_out = Vec::new();
        let mut debug = Vec::new();
        let inputs = all_req_inputs();
        {
            let mut hooks = ar_hooks(&mut mock, &mut hotbar, &mut targets_out, &mut debug);
            hooks.blink_running = true; // requirementsMet requires !Blink.running
            auto_rod_tick(&module, &mut state, &ar_player(), &mut hooks, &inputs, 4, 0, 0.0);
        }
        assert!(hotbar.use_calls.is_empty());
        assert!(state.phase.is_none());
    }

    #[test]
    fn auto_rod_tick_full_sequence_select_pull_cooldown() {
        let module = auto_rod();
        let mut state = auto_rod_state(0.0);
        let (mut mock, mut hotbar) = ar_setup();
        let mut targets_out = Vec::new();
        let mut debug = Vec::new();

        // Tick 1: select rod (bobber == null), open the pull sequence.
        {
            let mut hooks = ar_hooks(&mut mock, &mut hotbar, &mut targets_out, &mut debug);
            // the player already looks at the target (yaw -90 from looking_at)
            hooks.server_rotation = Rotation { yaw: -90.0, pitch: 0.0 };
            auto_rod_tick(&module, &mut state, &ar_player(), &mut hooks, &all_req_inputs(), 4, 0, 0.5);
        }
        assert_eq!(hotbar.use_calls.len(), 1);
        // ticksUntilReset = slotResetDelay.last + hitTimeout = 0 + 30
        assert_eq!(hotbar.use_calls[0].0, 30);
        assert_eq!(hotbar.use_calls[0].1, "DoNotHide");
        let Some(ArPhase::WaitPull { timeout_left, .. }) = state.phase else {
            panic!("expected WaitPull phase");
        };
        assert_eq!(timeout_left, 30);

        // Tick 2..: hook the bobber → pull with slotResetDelay pick (0), then cooldown.
        mock.bobber = Some(ArFishingBobber { hooked_in: true, known_movement: Vec3::ZERO });
        {
            let mut hooks = ar_hooks(&mut mock, &mut hotbar, &mut targets_out, &mut debug);
            auto_rod_tick(&module, &mut state, &ar_player(), &mut hooks, &all_req_inputs(), 4, 0, 0.5);
        }
        // 4. pull; 5. reset slot with the random pick
        assert_eq!(hotbar.use_calls.len(), 2);
        assert_eq!(hotbar.use_calls[1].0, 0);
        let Some(ArPhase::WaitCooldown { left }) = state.phase else {
            panic!("expected WaitCooldown phase");
        };
        assert_eq!(left, 4);

        // Two more ticks: cooldown counts down, nothing else fires.
        for _ in 0..2 {
            let mut hooks = ar_hooks(&mut mock, &mut hotbar, &mut targets_out, &mut debug);
            auto_rod_tick(&module, &mut state, &ar_player(), &mut hooks, &all_req_inputs(), 4, 0, 0.5);
        }
        assert_eq!(hotbar.use_calls.len(), 2);
        let Some(ArPhase::WaitCooldown { left }) = state.phase else {
            panic!("cooldown should still be open");
        };
        assert_eq!(left, 2);

        // Two more cooldown ticks close it (4→3→2 above, then 2→1, then closed).
        for _ in 0..2 {
            let mut hooks = ar_hooks(&mut mock, &mut hotbar, &mut targets_out, &mut debug);
            auto_rod_tick(&module, &mut state, &ar_player(), &mut hooks, &all_req_inputs(), 4, 0, 0.5);
        }
        assert!(state.phase.is_none());
        // Restarted the sequence: the bobber is hooked, so no second select.
        assert_eq!(hotbar.use_calls.len(), 2);
    }

    #[test]
    fn auto_rod_on_disabled_releases_the_bobber() {
        let module = auto_rod();
        let mut state = auto_rod_state(0.0);
        let (mut mock, mut hotbar) = ar_setup();
        let mut targets_out = Vec::new();
        let mut debug = Vec::new();
        mock.bobber = Some(ArFishingBobber { hooked_in: false, known_movement: Vec3::ZERO });
        let mut hooks = ar_hooks(&mut mock, &mut hotbar, &mut targets_out, &mut debug);
        auto_rod_on_disabled(&mut state, &mut hooks);
        assert_eq!(hotbar.releases, 1);
        assert_eq!(hotbar.resets, 1);
        assert!(state.bobber.is_none());
        assert!(state.available_rod_slot.is_none());
        assert!(state.target.is_none());
    }

    // ----- AutoWeapon -----

    fn aw_stack(id: u64, tags: &[&str], damage: f64, speed: f64, kb: i64) -> AwItemStack {
        AwItemStack {
            id,
            is_empty: false,
            weapon_tags: tags.iter().map(|t| t.to_string()).collect(),
            attack_damage: damage,
            attack_speed: speed,
            knockback_level: kb,
            is_axe: tags.contains(&"Axe"),
            is_mace: tags.contains(&"Mace"),
        }
    }

    #[derive(Default)]
    struct AwInvMock {
        slots: Vec<AwSlot>,
    }

    impl AwInventory for AwInvMock {
        fn hotbar(&mut self) -> Vec<AwSlot> {
            self.slots.clone()
        }
    }

    fn aw_slots() -> Vec<AwSlot> {
        vec![
            AwSlot { inventory_slot: 0, stack: aw_stack(1, &["Sword"], 7.0, 1.6, 0), facet_rank: Some(10) },
            AwSlot { inventory_slot: 1, stack: aw_stack(2, &["Sword"], 7.0, 1.6, 2), facet_rank: Some(8) },
            AwSlot { inventory_slot: 2, stack: aw_stack(3, &["Axe"], 9.0, 1.0, 0), facet_rank: Some(20) },
            AwSlot { inventory_slot: 3, stack: aw_stack(4, &["Pickaxe"], 5.0, 1.2, 0), facet_rank: None },
        ]
    }

    fn aw_player(main_hand: AwItemStack) -> AwPlayer {
        AwPlayer {
            main_hand,
            can_smash_attack: false,
            has_cooldown: true,
            using_consumable_main_hand: false,
            slot_modified_by_auto_buff: false,
        }
    }

    fn aw_hooks<'a>(inv: &'a mut AwInvMock, selections: &'a mut Vec<(i64, i64)>) -> AwHooks<'a> {
        AwHooks {
            inventory: inv,
            mace_kill_enabled: false,
            is_older_or_equal_1_8: false,
            selections,
            ensure_sent_carried_item_calls: Box::leak(Box::new(0usize)),
            reset_slot_calls: Box::leak(Box::new(0usize)),
        }
    }

    #[test]
    fn auto_weapon_default_prefers_the_preferred_weapon() {
        let module = auto_weapon();
        let mut slots = aw_slots();
        slots[0].facet_rank = Some(5); // the best rank among swords
        let mut inv = AwInvMock { slots };
        let mut selections = Vec::new();
        let mut hooks = aw_hooks(&mut inv, &mut selections);
        let player = aw_player(aw_stack(99, &[], 1.0, 1.0, 0));
        let slot = aw_determine_weapon_slot(&module, &player, &mut hooks, None, false);
        assert_eq!(slot, Some(0));
    }

    #[test]
    fn auto_weapon_knockback_priority_picks_the_best_knockback_stack() {
        let mut module = auto_weapon();
        // Priority = Knockback
        let value = module.setting_mut("Priority").unwrap();
        value.set_from_json(&json_str("Knockback")).unwrap();

        let mut inv = AwInvMock { slots: aw_slots() };
        let mut selections = Vec::new();
        let mut hooks = aw_hooks(&mut inv, &mut selections);
        let player = aw_player(aw_stack(99, &[], 1.0, 1.0, 0));
        // Knockback 2 lives on stack id 2 (slot 1); the priority filter ignores ranks
        assert_eq!(aw_determine_weapon_slot(&module, &player, &mut hooks, None, false), Some(1));
    }

    #[test]
    fn auto_weapon_attack_handler_selects_when_on_attack() {
        let module = auto_weapon();
        let mut inv = AwInvMock { slots: aw_slots() };
        let mut selections = Vec::new();
        let mut hooks = aw_hooks(&mut inv, &mut selections);
        let player = aw_player(aw_stack(99, &[], 1.0, 1.0, 0));
        let target = AwAttackTarget { is_living_entity: true, would_block_hit: false };
        auto_weapon_attack_handler(&module, &player, &mut hooks, &target);
        // slot 1 holds the better WeaponItemFacet rank (8 < 10)
        assert_eq!(*hooks.selections, vec![(1i64, 20i64)]);
        assert_eq!(*hooks.ensure_sent_carried_item_calls, 1);

        // Busy: Auto Buff holds the slot → no new selection.
        let player = AwPlayer { slot_modified_by_auto_buff: true, ..aw_player(aw_stack(99, &[], 1.0, 1.0, 0)) };
        auto_weapon_attack_handler(&module, &player, &mut hooks, &target);
        assert_eq!(*hooks.selections, vec![(1i64, 20i64)]);
    }

    #[test]
    fn auto_weapon_shield_break_wants_an_axe() {
        let module = auto_weapon();
        let mut inv = AwInvMock { slots: aw_slots() };
        let mut selections = Vec::new();
        let mut hooks = aw_hooks(&mut inv, &mut selections);

        // On 1.8 there is no shield.
        hooks.is_older_or_equal_1_8 = true;
        let player = aw_player(aw_stack(99, &[], 1.0, 1.0, 0));
        assert!(!auto_weapon_will_shield_break(&module, &player, &mut hooks));
        hooks.is_older_or_equal_1_8 = false;

        // An axe in the main hand already breaks it.
        let player = aw_player(aw_stack(99, &["Axe"], 9.0, 1.0, 0));
        assert!(auto_weapon_will_shield_break(&module, &player, &mut hooks));

        // Otherwise the determined slot decides (AutoShieldBreak → AXE filter).
        let player = aw_player(aw_stack(99, &[], 1.0, 1.0, 0));
        assert!(auto_weapon_will_shield_break(&module, &player, &mut hooks));
    }

    #[test]
    fn auto_weapon_get_attack_speed_uses_the_determined_weapon() {
        let mut module = auto_weapon();
        module.running = true;
        let mut inv = AwInvMock { slots: aw_slots() };
        let mut selections = Vec::new();
        let mut hooks = aw_hooks(&mut inv, &mut selections);
        let player = aw_player(aw_stack(99, &[], 1.0, 1.0, 0));
        assert_eq!(auto_weapon_get_attack_speed(&module, &player, &mut hooks, 4.0), 1.6);

        // No cooldown → original.
        let player = AwPlayer { has_cooldown: false, ..aw_player(aw_stack(99, &[], 1.0, 1.0, 0)) };
        assert_eq!(auto_weapon_get_attack_speed(&module, &player, &mut hooks, 4.0), 4.0);
    }

    // ----- Aimbot -----

    #[derive(Default)]
    struct AbMock {
        targets: Vec<AbEntity>,
        crystals: Vec<AbEntity>,
        lazy_hit: Option<Vec3>,
        point: Option<AbPoint>,
        ray: Option<AbRotationWithVector>,
    }

    impl AbWorld for AbMock {
        fn targets(&mut self) -> Vec<AbEntity> {
            self.targets.clone()
        }
        fn crystals_in_range(&mut self, _eyes: Vec3, _max_range: f64) -> Vec<AbEntity> {
            self.crystals.clone()
        }
        fn look_at_hit(&mut self, _e: &AbEntity, _r: Rotation, _range: f64, _walls: f64) -> Option<Vec3> {
            self.lazy_hit
        }
        fn find_point(&mut self, _eyes: Vec3, _e: &AbEntity) -> Option<AbPoint> {
            self.point
        }
        fn raytrace_box(&mut self, _eyes: Vec3, _p: &AbPoint, _range: f64, _walls: f64) -> Option<AbRotationWithVector> {
            self.ray
        }
    }

    #[derive(Default)]
    struct AbAngleSmoothMock;
    impl AbAngleSmooth for AbAngleSmoothMock {
        fn process(&mut self, mode: &str, _player: Rotation, target: Rotation) -> Option<Rotation> {
            Some(Rotation { yaw: target.yaw, pitch: target.pitch * (if mode == "Linear" { 2.0 } else { 1.0 }) })
        }
    }

    #[derive(Default)]
    struct AbMouseMock {
        applied: usize,
    }
    impl AbMouseDelta for AbMouseMock {
        fn apply(&mut self, mut r: Rotation, _dx: f64, _dy: f64) -> Rotation {
            self.applied += 1;
            r.yaw += 10.0;
            r
        }
    }

    fn ab_entity(id: usize, cos: f64) -> AbEntity {
        AbEntity { is_end_crystal: false, is_living_entity: true, crosshair_cos: cos, ..AbEntity { is_end_crystal: false, is_living_entity: true, crosshair_cos: cos } }
    }

    #[test]
    fn aimbot_crystals_only_returns_crystal_targets() {
        let mut module = aimbot();
        module.setting_mut("Crystals").unwrap().set_from_json(&Json::Bool(true)).unwrap();
        module.setting_mut("CrystalsOnly").unwrap().set_from_json(&Json::Bool(true)).unwrap();

        let mut mock = AbMock {
            crystals: vec![ab_entity(1, 0.9), ab_entity(2, 0.5)],
            ..Default::default()
        };
        let mut smooth = AbAngleSmoothMock;
        let mut mouse = AbMouseMock::default();
        let mut set_rotations = Vec::new();
        let mut on_target = 0usize;
        let mut hooks = AbHooks {
            world: &mut mock,
            angle_smooth: &mut smooth,
            mouse_delta: &mut mouse,
            screen_open: false,
            inventory_open: false,
            container_screen_open: false,
            use_key_pressed: true,
            timer_speed: 1.0,
            player_rotation_raw: Rotation { yaw: 0.0, pitch: 0.0 },
            eye_position: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
            on_target_calls: &mut on_target,
            set_rotations: &mut set_rotations,
        };
        let targets = ab_targets(&module, &mut hooks);
        assert_eq!(targets.len(), 2);
        // sorted by descending crosshair cos
        assert!(targets[0].crosshair_cos > targets[1].crosshair_cos);
    }

    #[test]
    fn aimbot_rotation_update_requires_no_screen() {
        let module = aimbot();
        let mut state = aimbot_state();
        let mut mock = AbMock { targets: vec![ab_entity(1, 0.9)], point: Some(AbPoint { pos: Vec3 { x: 1.0, y: 0.0, z: 0.0 }, box_min: Vec3::ZERO, box_max: Vec3::ZERO }), ray: Some(AbRotationWithVector { rotation: Rotation { yaw: -90.0, pitch: 0.0 }, location: Vec3::ZERO }), ..Default::default() };
        let mut smooth = AbAngleSmoothMock;
        let mut mouse = AbMouseMock::default();
        let mut set_rotations = Vec::new();
        let mut on_target = 0usize;
        let mut hooks = AbHooks {
            world: &mut mock,
            angle_smooth: &mut smooth,
            mouse_delta: &mut mouse,
            screen_open: true,
            inventory_open: false,
            container_screen_open: false,
            use_key_pressed: true,
            timer_speed: 1.0,
            player_rotation_raw: Rotation { yaw: 0.0, pitch: 0.0 },
            eye_position: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
            on_target_calls: &mut on_target,
            set_rotations: &mut set_rotations,
        };
        let inputs = all_req_inputs();
        {
            aimbot_rotation_update(&module, &mut state, &mut hooks, &inputs);
            assert!(state.target_rotation.is_none());
            assert!(!state.target_set);
            assert_eq!(*hooks.on_target_calls, 0);

            hooks.screen_open = false;
            aimbot_rotation_update(&module, &mut state, &mut hooks, &inputs);
            assert_eq!(state.target_rotation, Some(Rotation { yaw: -90.0, pitch: 0.0 }));
            assert!(state.target_set);
            assert_eq!(*hooks.on_target_calls, 1);
        }
    }

    #[test]
    fn aimbot_look_at_interpolates_with_axis_gates() {
        let module = aimbot();
        let mut state = aimbot_state();
        state.player_rotation = Some(Rotation { yaw: 0.0, pitch: 0.0 });
        state.target_rotation = Some(Rotation { yaw: 10.0, pitch: 10.0 });
        let mut mock = AbMock::default();
        let mut smooth = AbAngleSmoothMock;
        let mut mouse = AbMouseMock::default();
        let mut set_rotations = Vec::new();
        let mut on_target = 0usize;
        let mut hooks = AbHooks {
            world: &mut mock,
            angle_smooth: &mut smooth,
            mouse_delta: &mut mouse,
            screen_open: false,
            inventory_open: false,
            container_screen_open: false,
            use_key_pressed: true,
            timer_speed: 1.0,
            player_rotation_raw: Rotation { yaw: 0.0, pitch: 0.0 },
            eye_position: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
            on_target_calls: &mut on_target,
            set_rotations: &mut set_rotations,
        };
        aimbot_look_at(&module, &state, 0.5, &mut hooks);
        // Axis defaults to Horizontal+Vertical → both interpolated at factor 0.5
        assert_eq!(*hooks.set_rotations, vec![Rotation { yaw: 5.0, pitch: 5.0 }]);
    }

    // ----- AutoShoot -----

    #[derive(Default)]
    struct AsMock {
        los_target: Option<AsTarget>,
        point: Option<Vec3>,
        angle: Option<Rotation>,
        eye: Vec3,
    }

    impl AsWorld for AsMock {
        fn select_first_with_los(&mut self) -> Option<AsTarget> {
            self.los_target
        }
        fn eye_position(&mut self) -> Vec3 {
            self.eye
        }
        fn find_point(&mut self, _eyes: Vec3, _t: &AsTarget) -> Option<Vec3> {
            self.point
        }
        fn projectile_angle(&mut self, _t: &AsTarget) -> Option<Rotation> {
            self.angle
        }
    }

    #[derive(Default)]
    struct AsInvMock {
        slot: Option<AsSlot>,
    }

    impl AsInventory for AsInvMock {
        fn find_closest_slot(&mut self, mode: &AsThrowableMode) -> Option<AsSlot> {
            let slot = self.slot.clone()?;
            mode.matches(&slot).then_some(slot)
        }
    }

    #[derive(Default)]
    struct AsHotbarMock {
        select_calls: Vec<i64>,
        still_needed: bool,
        use_calls: usize,
        click_allowed: bool,
    }

    impl AsHotbar for AsHotbarMock {
        fn select_slot_silently(&mut self, _slot: &AsSlot, ticks: i64) -> bool {
            self.select_calls.push(ticks);
            self.still_needed
        }
        fn reset_slot(&mut self) {}
        fn use_item(&mut self, _slot: &AsSlot, _swing: &str) -> bool {
            self.use_calls += 1;
            true
        }
        fn click_allowed(&mut self) -> bool {
            self.click_allowed
        }
    }

    fn as_slot(item: &str) -> AsSlot {
        AsSlot {
            hotbar_index: Some(2),
            serverside_slot: 0,
            item: item.to_string(),
            is_empty: false,
            use_hand: "Main".to_string(),
        }
    }

    #[test]
    fn auto_shoot_try_select_selects_the_slot() {
        let module = auto_shoot();
        let mut mock = AsMock::default();
        let mut inv = AsInvMock { slot: Some(as_slot("minecraft:egg")) };
        let mut hotbar = AsHotbarMock { click_allowed: true, ..Default::default() };
        let mut targets_out = Vec::new();
        let mut hooks = AsHooks {
            world: &mut mock,
            hotbar: &mut hotbar,
            inventory: &mut inv,
            server_rotation: Rotation { yaw: 0.0, pitch: 0.0 },
            rotation_targets: &mut targets_out,
            in_combat: false,
            player_is_using_item: false,
            inventory_open: false,
            kill_aura_running: false,
        };
        let slot = as_throwable_slot(&module, &mut hooks).unwrap();
        assert_eq!(slot.item, "minecraft:egg");
        assert_eq!(hotbar.select_calls, vec![1i64]); // TicksUntilSlotReset default
    }

    #[test]
    fn auto_shoot_rotation_update_requires_line_of_sight() {
        let module = auto_shoot();
        let mut state = auto_shoot_state();
        let mut mock = AsMock { angle: Some(Rotation { yaw: 5.0, pitch: 0.0 }), ..Default::default() };
        let mut inv = AsInvMock { slot: Some(as_slot("minecraft:egg")) };
        let mut hotbar = AsHotbarMock { click_allowed: true, ..Default::default() };
        let mut targets_out = Vec::new();
        let mut hooks = AsHooks {
            world: &mut mock,
            hotbar: &mut hotbar,
            inventory: &mut inv,
            server_rotation: Rotation { yaw: 0.0, pitch: 0.0 },
            rotation_targets: &mut targets_out,
            in_combat: false,
            player_is_using_item: false,
            inventory_open: false,
            kill_aura_running: false,
        };
        {
            auto_shoot_rotation_update(&module, &mut state, &mut hooks);
            assert!(state.target.is_none()); // no LOS
        }
        mock.los_target = Some(AsTarget);
        {
            let mut hooks = AsHooks {
                world: &mut mock,
                hotbar: &mut hotbar,
                inventory: &mut inv,
                server_rotation: Rotation { yaw: 0.0, pitch: 0.0 },
                rotation_targets: &mut targets_out,
                in_combat: false,
                player_is_using_item: false,
                inventory_open: false,
                kill_aura_running: false,
            };
            auto_shoot_rotation_update(&module, &mut state, &mut hooks);
        }
        assert!(state.target.is_some());
        // GravityType Auto + egg → Projectile; angle seam answered
        assert_eq!(targets_out.len(), 1);
        assert!(targets_out[0].consider_inventory);
    }

    #[test]
    fn auto_shoot_tick_clicks_when_aimed() {
        let module = auto_shoot();
        let mut state = auto_shoot_state();
        state.target = Some(AsTarget);
        let mut mock = AsMock { angle: Some(Rotation { yaw: 1.0, pitch: 0.0 }), ..Default::default() };
        let mut inv = AsInvMock { slot: Some(as_slot("minecraft:egg")) };
        let mut hotbar = AsHotbarMock { click_allowed: true, ..Default::default() };
        let mut targets_out = Vec::new();
        let mut hooks = AsHooks {
            world: &mut mock,
            hotbar: &mut hotbar,
            inventory: &mut inv,
            server_rotation: Rotation { yaw: 0.0, pitch: 0.0 },
            rotation_targets: &mut targets_out,
            in_combat: false,
            player_is_using_item: false,
            inventory_open: false,
            kill_aura_running: false,
        };
        {
            auto_shoot_tick(&module, &mut state, &mut hooks);
        }
        assert_eq!(hotbar.use_calls, 1);

        // Aim off → no click.
        hotbar.use_calls = 0;
        mock.angle = Some(Rotation { yaw: 45.0, pitch: 0.0 });
        {
            let mut hooks = AsHooks {
                world: &mut mock,
                hotbar: &mut hotbar,
                inventory: &mut inv,
                server_rotation: Rotation { yaw: 0.0, pitch: 0.0 },
                rotation_targets: &mut targets_out,
                in_combat: false,
                player_is_using_item: false,
                inventory_open: false,
                kill_aura_running: false,
            };
            auto_shoot_tick(&module, &mut state, &mut hooks);
        }
        assert_eq!(hotbar.use_calls, 0);
    }

    #[test]
    fn auto_shoot_requires_kill_aura_resets_the_target() {
        let mut module = auto_shoot();
        module.setting_mut("RequiresKillAura").unwrap().set_from_json(&Json::Bool(true)).unwrap();
        let mut state = auto_shoot_state();
        state.target = Some(AsTarget);
        let mut mock = AsMock::default();
        let mut inv = AsInvMock::default();
        let mut hotbar = AsHotbarMock::default();
        let mut targets_out = Vec::new();
        let mut hooks = AsHooks {
            world: &mut mock,
            hotbar: &mut hotbar,
            inventory: &mut inv,
            server_rotation: Rotation { yaw: 0.0, pitch: 0.0 },
            rotation_targets: &mut targets_out,
            in_combat: false,
            player_is_using_item: false,
            inventory_open: false,
            kill_aura_running: false,
        };
        assert!(as_should_pause_for_kill_aura(&module, &mut state, &hooks));
        assert!(state.target.is_none());
    }
}

// ----- ModuleTickBase.kt -----

/// Upstream `ModuleTickBase` ("TickBase", aliases "TimerRange"): saves up timer
/// balance near an enemy and spends it to reach the enemy earlier, by speeding
/// up the timer or by bursting ticks. Part 1 this slice: the settings, the
/// `Requirement` gates, `chargingSpeed`, the `GameTickEvent` speed handler and
/// the `TimerBoost` mode. `Advantage.paysOff` and the `BurstMode` tick machine
/// are carried as seams until their part lands.
///
/// Upstream `Requirement` tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TbRequirement {
    KillAura,
    LookingAt,
    Click,
    OnGround,
}

impl TbRequirement {
    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "KillAura" => Some(Self::KillAura),
            "LookingAt" => Some(Self::LookingAt),
            "Click" => Some(Self::Click),
            "OnGround" => Some(Self::OnGround),
            _ => None,
        }
    }
}

/// Upstream `SimulatedPlayerSnapshot` fields TickBase reads, from the
/// simulation seam.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TbSnapshot {
    pub pos: [f64; 3],
    pub on_ground: bool,
    pub fall_distance: f64,
}

impl TbSnapshot {
    /// Upstream `SimulatedPlayerSnapshot.eyes()`.
    pub fn eyes(&self, eye_height: f64) -> [f64; 3] {
        [self.pos[0], self.pos[1] + eye_height, self.pos[2]]
    }

    /// Upstream `SimulatedPlayerSnapshot.falling`.
    pub fn falling(&self) -> bool {
        !self.on_ground && self.fall_distance > 0.0
    }
}

/// Upstream `Hit`: a hit on `entity`, landing on `point`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TbHit {
    pub entity: u64,
    pub point: [f64; 3],
}

/// The seams TickBase reads each tick, documented against the upstream names
/// they carry.
pub struct TbHooks<'a> {
    /// Upstream `Timer.requestTimerSpeed(speed)`: called with this tick's
    /// speed, only when a mode or charging produces one.
    pub request_timer_speed: Box<dyn FnMut(f64) + 'a>,
    /// Upstream `Timer.balance`.
    pub timer_balance: f64,
    /// Upstream `Timer.spendableTicks(0..maxBalance)`: takes `MaxBalance`.
    pub timer_spendable_ticks: Box<dyn Fn(i64) -> f64 + 'a>,
    /// Upstream `snapshotsFromNow()`: the coming ticks, starting with now.
    pub snapshots_from_now: Box<dyn Fn() -> Vec<TbSnapshot> + 'a>,
    /// Upstream `hitPointFrom()` bound: where a hit from the given eyes lands,
    /// `None` when it misses. Uses where the server has the enemies.
    pub hit_at: Box<dyn Fn([f64; 3]) -> Option<TbHit> + 'a>,
    /// Upstream `world.findEnemy(0f, ChargeRange) != null`.
    pub enemy_in_charge_range: Box<dyn Fn() -> bool + 'a>,
    /// Upstream `enemyUnderCrosshair(eyes, direction, range)`.
    pub enemy_under_crosshair: Box<dyn Fn([f64; 3], [f64; 3], f64) -> Option<TbHit> + 'a>,
    /// Upstream `ModuleKillAura.running && ModuleKillAura.targetTracker.target != null`.
    pub killaura_engaged: bool,
    /// Upstream `ModuleKillAura.clicker.itemCooldown?.nextCooldown`, already
    /// carrying the `takeIf { KillAura.running && target != null }`.
    pub killaura_next_cooldown: Option<f64>,
    /// Upstream `CombatManager.attackCooldownProgress(landingDelay)`.
    pub attack_cooldown_progress: Box<dyn Fn(i64) -> f64 + 'a>,
    /// Upstream `player.hasCooldown` (the server has an attack cooldown).
    pub has_cooldown: bool,
    /// Upstream `player.isUsingItem`.
    pub using_item: bool,
    /// Upstream `player.onGround()`.
    pub on_ground: bool,
    /// Upstream `player.vehicle != null` (TickBase stays off while driving).
    pub in_vehicle: bool,
    /// Upstream `ModuleBlink.running`.
    pub blink_running: bool,
    /// Upstream `player.eyePosition`.
    pub eye_position: [f64; 3],
    /// Upstream `player.eyeHeight`.
    pub eye_height: f64,
    /// Upstream `player.lookAngle`.
    pub look_angle: [f64; 3],
    /// Upstream `mc.options.keyAttack.isPressedOnAny`.
    pub attack_pressed: bool,
    /// Upstream `mc.options.keyAttack.wasPressedRecently(250)`.
    pub attack_recently_pressed: bool,
    /// Upstream `Timer.hold(this, ticks)`: the engine holds back up to `ticks`
    /// while the hold handler re-plans.
    pub timer_hold: Box<dyn FnMut(i64) + 'a>,
    /// Upstream `Timer.burst(ticks, range)`: bursts up to the range around
    /// `ticks`, returns the ticks actually burst.
    pub timer_burst: Box<dyn FnMut(i64, i64, i64) -> i64 + 'a>,
    /// Upstream `Timer.heldTicksWithin(ticks)`.
    pub timer_held_ticks_within: Box<dyn Fn(i64) -> i64 + 'a>,
    /// Upstream `killAuraClicksAt(ticks)`, already carrying
    /// `!KillAura.running || target == null || clicker.willClickAt(ticks)`.
    pub killaura_clicks_at: Box<dyn Fn(i64) -> bool + 'a>,
    /// Upstream `ModuleCriticals.running || KillAura.running &&
    /// KillAura.criticalsSelectionMode != IGNORE`.
    pub prefers_critical: bool,
    /// Upstream `ModuleCriticals.allowsCriticalHit(ignoreOnGround = true)`.
    pub criticals_allows_hit: bool,
    /// Upstream `player.isSprinting`.
    pub is_sprinting: bool,
    /// Upstream `player.ping`, in milliseconds.
    pub player_ping_ms: f64,
    /// Upstream `PositionExtrapolation.getBestForEntity(entity).getPositionInTicks(ticks)`.
    pub extrapolated_position: Box<dyn Fn(u64, f64) -> [f64; 3] + 'a>,
    /// Upstream `enemy.hurtTime / hurtDuration / armorKnockbackResistance /
    /// eyeHeight`, and `enemy.serverPosition - enemy.position()`.
    pub enemy_info: Box<dyn Fn(u64) -> Option<TbEnemy> + 'a>,
    /// Upstream `CombatManager.attackCooldownProgress(enemy, ticks)` for enemy
    /// players, `None` for non-players.
    pub enemy_attack_cooldown_progress: Box<dyn Fn(u64, f64) -> Option<f64> + 'a>,
    /// Upstream `player.getBoundingBoxAt(pos)`.
    pub bounding_box_at: Box<dyn Fn([f64; 3]) -> TbBox + 'a>,
    /// Upstream `TickBaseVisuals.show(path, ticks, point)`.
    pub visuals_show: Box<dyn FnMut(&[[f64; 3]], Option<usize>, Option<[f64; 3]>) + 'a>,
    /// Upstream `TickBaseVisuals.clear()`.
    pub visuals_clear: Box<dyn FnMut() + 'a>,
}

fn tb_setting_f64(module: &Module, name: &str, default: f64) -> f64 {
    match module.setting(name).map(|v| &v.kind) {
        Some(ValueKind::Float(f, _, _)) => *f,
        _ => default,
    }
}

fn tb_setting_i64(module: &Module, name: &str, default: i64) -> i64 {
    match module.setting(name).map(|v| &v.kind) {
        Some(ValueKind::Int(i, _, _)) => *i,
        _ => default,
    }
}

fn tb_setting_bool(module: &Module, name: &str, default: bool) -> bool {
    match module.setting(name).map(|v| &v.kind) {
        Some(ValueKind::Bool(b)) => *b,
        _ => default,
    }
}

fn tb_mode(module: &Module) -> &str {
    match module.setting("Mode").map(|v| &v.kind) {
        Some(kind) => kind.choice().unwrap_or("Timer"),
        None => "Timer",
    }
}

fn tb_requires(module: &Module) -> Vec<TbRequirement> {
    match module.setting("Requires").map(|v| &v.kind) {
        Some(ValueKind::MultiChoice(sel, _)) => sel
            .iter()
            .filter_map(|t| TbRequirement::from_tag(t))
            .collect(),
        _ => vec![TbRequirement::KillAura],
    }
}

/// Upstream `cooldown.random()`: a uniform int of the `Cooldown` range, from a
/// roll in [0, 1).
fn tb_cooldown_random(module: &Module, roll: f64) -> i64 {
    let (from, to) = match module.setting("Cooldown").map(|v| &v.kind) {
        Some(kind) => kind.int_range().unwrap_or((0, 0)),
        None => (0, 0),
    };
    from + (roll * ((to - from + 1) as f64)).floor() as i64
}

/// Upstream `Requirement.meets(hit)`.
fn tb_requirement_meets(req: TbRequirement, hooks: &TbHooks, hit: [f64; 3]) -> bool {
    match req {
        TbRequirement::KillAura => hooks.killaura_engaged,
        TbRequirement::LookingAt => {
            // Past the hit, as it lands on the surface of the hitbox
            let eyes = hooks.eye_position;
            let dist = ((eyes[0] - hit[0]).powi(2)
                + (eyes[1] - hit[1]).powi(2)
                + (eyes[2] - hit[2]).powi(2))
            .sqrt();
            (hooks.enemy_under_crosshair)(eyes, hooks.look_angle, dist + 1.0).is_some()
        }
        TbRequirement::Click => hooks.attack_pressed || hooks.attack_recently_pressed,
        TbRequirement::OnGround => hooks.on_ground,
    }
}

/// Upstream `requirementsMet(hit)`: every selected `Requirement` met.
fn tb_requirements_met(module: &Module, hooks: &TbHooks, hit: [f64; 3]) -> bool {
    tb_requires(module)
        .iter()
        .all(|req| tb_requirement_meets(*req, hooks, hit))
}

/// Upstream `percentageChance("Chance").asBoolean`, shared with SuperKnockback.
fn tb_chance_as_boolean(module: &Module, roll: f64) -> bool {
    sk_chance_as_boolean(module, roll)
}

/// Upstream `rollChance()`: the chance is rolled once until the chance is gone.
fn tb_roll_chance(module: &Module, rolled: &mut Option<bool>, roll: f64) -> bool {
    if let Some(v) = *rolled {
        return v;
    }
    let v = tb_chance_as_boolean(module, roll);
    *rolled = Some(v);
    v
}

/// Upstream `chargingSpeed()`: charges while an enemy is near but out of
/// reach. Plain `Timer.requestTimerSpeed`, since clamping the balance would
/// drop what BurstFirst still has to pay back.
fn tick_base_charging_speed(module: &Module, hooks: &TbHooks) -> Option<f64> {
    let charge_speed = tb_setting_f64(module, "ChargeSpeed", 0.9);
    if charge_speed < 1.0
        && hooks.timer_balance < tb_setting_i64(module, "MaxBalance", 3) as f64
        && (tb_setting_bool(module, "ChargeWhileUsingItem", false) || !hooks.using_item)
        && (hooks.enemy_in_charge_range)()
        && (hooks.hit_at)(hooks.eye_position).is_none()
    {
        Some(charge_speed)
    } else {
        None
    }
}

/// Upstream `BoostMode.cooldownReady(landingDelay)`: whether the server has the
/// attack cooldown full enough for a hit landing `landingDelay` ticks of real
/// time from now. KillAura's own minimum applies on top.
fn tb_cooldown_ready(module: &Module, hooks: &TbHooks, landing_delay: i64) -> bool {
    if !hooks.has_cooldown {
        return true;
    }

    let required = tb_setting_f64(module, "AttackCooldown", 1.0)
        .max(hooks.killaura_next_cooldown.unwrap_or(0.0));
    let progress = (hooks.attack_cooldown_progress)(landing_delay);
    progress >= required
}

/// Upstream `TimerBoost` state: `boosting`, `cooldownTicks` (reset on enable).
#[derive(Default)]
pub struct TbTimerBoostState {
    pub boosting: bool,
    pub cooldown_ticks: i64,
}

/// Upstream `TimerBoost.boostTicks()`: ticks the balance lets run at
/// `boostSpeed`. Each takes 1 / speed ticks of real time and spends the rest of
/// a tick of balance.
fn tb_boost_ticks(module: &Module, hooks: &TbHooks, boost_speed: f64) -> usize {
    if boost_speed <= 1.0 {
        return 0;
    }

    let ticks =
        (hooks.timer_spendable_ticks)(tb_setting_i64(module, "MaxBalance", 3)) / (1.0 - 1.0 / boost_speed);
    // Longer boosts are not worth simulating every tick
    (ticks as i64).clamp(0, 100) as usize
}

/// Upstream `TimerBoost.planBoost()`: the snapshots of the boost the balance
/// pays for, up to the hit landing on the last, and that hit.
fn tb_plan_boost(
    module: &Module,
    hooks: &TbHooks,
    boost_speed: f64,
) -> (Vec<TbSnapshot>, Option<TbHit>) {
    let mut path: Vec<TbSnapshot> = Vec::new();

    for snapshot in (hooks.snapshots_from_now)()
        .into_iter()
        .take(tb_boost_ticks(module, hooks, boost_speed) + 1)
    {
        path.push(snapshot);
        let hit = (hooks.hit_at)(snapshot.eyes(hooks.eye_height));
        if hit.is_some() {
            return (path, hit);
        }
    }

    (path, None)
}

/// Upstream `TimerBoost.timerSpeed()`: speeds the game up once the balance
/// reaches a hit. Returns the speed the game should run at this tick, `None` to
/// leave it to charging.
pub fn tick_base_timer_boost_timer_speed(
    module: &Module,
    state: &mut TbTimerBoostState,
    hooks: &mut TbHooks,
    rolled: &mut Option<bool>,
    roll: f64,
) -> Option<f64> {
    let boost_speed = tb_setting_f64(module, "BoostSpeed", 5.0);
    if state.cooldown_ticks > 0 {
        state.cooldown_ticks -= 1;
    }

    let (path, hit) = tb_plan_boost(module, hooks, boost_speed);
    let ticks = hit.map(|_| path.len() - 1);
    // Each boosted tick takes 1 / speed ticks of real time
    let landing_delay = ((ticks.unwrap_or(0) as f64) / boost_speed).ceil() as i64;
    let target = hit.filter(|hit| {
        let after: Vec<TbSnapshot> = (hooks.snapshots_from_now)()
            .into_iter()
            .skip(path.len())
            .take(tb_setting_i64(module, "KnockbackTicks", 2) as usize)
            .collect();
        let mut path_after = path.clone();
        path_after.extend(after);
        tb_requirements_met(module, hooks, hit.point)
            && tb_cooldown_ready(module, hooks, landing_delay)
            && tb_advantage_pays_off(module, hooks, hit, &path_after, path.len() - 1, landing_delay)
    });
    (hooks.visuals_show)(
        &path.iter().map(|s| s.pos).collect::<Vec<_>>(),
        if target.is_some() { ticks } else { None },
        target.map(|h| h.point),
    );

    let boost = ticks.is_some()
        && ticks.unwrap_or(0) > 0
        && target.is_some()
        && state.cooldown_ticks <= 0
        && tb_roll_chance(module, rolled, roll);
    if state.boosting && !boost {
        state.cooldown_ticks = tb_cooldown_random(module, roll);
    }
    state.boosting = boost;

    if ticks.is_none() || ticks == Some(0) {
        *rolled = None;
    }

    if boost {
        Some(boost_speed)
    } else if ticks == Some(0) {
        // A boost ending in reach goes back to the normal speed right away
        Some(1.0)
    } else {
        None
    }
}

/// Upstream `speedHandler` (`GameTickEvent`): the active mode's speed, else
/// charging, else nothing.
pub fn tick_base_speed_handler(
    module: &Module,
    boost_state: &mut TbTimerBoostState,
    burst_state: &TbBurstState,
    hooks: &mut TbHooks,
    rolled: &mut Option<bool>,
    roll: f64,
) {
    let speed = match tb_mode(module) {
        "Timer" => tick_base_timer_boost_timer_speed(module, boost_state, hooks, rolled, roll),
        "HoldFirst" | "BurstFirst" => tick_base_burst_timer_speed(burst_state),
        _ => None,
    }
    .or_else(|| tick_base_charging_speed(module, hooks));
    if let Some(speed) = speed {
        (hooks.request_timer_speed)(speed);
    }
}

/// Upstream `onDisabled()`.
pub fn tick_base_on_disabled(hooks: &mut TbHooks, rolled: &mut Option<bool>) {
    *rolled = None;
    (hooks.visuals_clear)();
}

pub fn tick_base() -> Module {
    Module::new(
        "TickBase",
        COMBAT,
        vec![
            choice("Mode", "Timer", &["Timer", "HoldFirst", "BurstFirst"]), // tagBy(this)
            multi_choice(
                "Requires",
                &["KillAura"],
                &["KillAura", "LookingAt", "Click", "OnGround"],
            ),
            float("Chance", 100.0, 0.0, 100.0), // percentageChance
            int_range("Cooldown", 0, 0, 0, 100),
            float("AttackCooldown", 1.0, 0.0, 1.0),
            int("MaxBalance", 3, 0, 40),
            float("ChargeRange", 50.0, 0.0, 500.0),
            float("ChargeSpeed", 0.9, 0.1, 1.0),
            boolean("ChargeWhileUsingItem", false),
            // tree(Advantage): ToggleableValueGroup("Advantage", enabled) with
            // its children carried flat
            boolean("Advantage", true),
            float("EnemyRange", 3.0, 0.0, 8.0),
            int("KnockbackTicks", 2, 0, 10),
            float("KnockbackMultiplier", 1.0, 0.0, 3.0),
            boolean("SkipInvulnerable", true),
            boolean("EnemyCooldown", true),
            boolean("Criticals", true),
            // TimerBoost.boostSpeed
            float("BoostSpeed", 5.0, 1.0, 10.0),
            // BurstMode: maxTicks + delay (ported with the mode in part 2)
            int("MaxTicks", 4, 1, 20),
            int_range("Delay", 0, 0, 0, 20),
        ],
    )
}

// ----- ModuleTickBase.kt part 2: Advantage + BurstMode -----

/// The enemy-side facts `Advantage` reads, from the world seam.
#[derive(Clone, Copy, Debug)]
pub struct TbEnemy {
    pub hurt_time: i64,
    pub hurt_duration: i64,
    pub is_player: bool,
    pub armor_knockback_resistance: f64,
    pub eye_height: f64,
    /// Upstream `enemy.serverPosition.subtract(enemy.position())`.
    pub server_offset: [f64; 3],
}

/// Upstream `player.getBoundingBoxAt(pos)` reduced to what the range check
/// reads: the distance from a point to the box.
#[derive(Clone, Copy, Debug)]
pub struct TbBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl TbBox {
    /// Upstream `AABB.distanceToSqr(point)`.
    pub fn distance_to_sqr(&self, point: [f64; 3]) -> f64 {
        let dx = (self.min[0] - point[0]).max(point[0] - self.max[0]).max(0.0);
        let dy = (self.min[1] - point[1]).max(point[1] - self.max[1]).max(0.0);
        let dz = (self.min[2] - point[2]).max(point[2] - self.max[2]).max(0.0);
        dx * dx + dy * dy + dz * dz
    }
}

/// Upstream `Vec3.normalize()`: a zero vector stays zero.
fn tb_normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len == 0.0 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

/// Upstream `Advantage`: boosts only when the hit wins the exchange: the enemy
/// is not invulnerable when it lands, and cannot hit back within
/// `KnockbackTicks` ticks. That is the case when the knockback pushes it out of
/// `EnemyRange` of us, when its own attack cooldown is not full yet, or when
/// our hit is critical. The last two apply only where the server has an attack
/// cooldown.
fn tb_advantage_pays_off(
    module: &Module,
    hooks: &TbHooks,
    hit: &TbHit,
    path: &[TbSnapshot],
    landing: usize,
    landing_delay: i64,
) -> bool {
    let enemy = (hooks.enemy_info)(hit.entity);
    if !tb_setting_bool(module, "Advantage", true) || enemy.is_none() {
        return true;
    }
    let enemy = enemy.unwrap();

    // Invulnerable when the hit reaches the server, so it takes no knockback
    // and hits back. The hurt time is a round trip late, and did not count
    // down while ticks were held since it arrived
    let hurt_time = enemy.hurt_time
        - (hooks.timer_held_ticks_within)(enemy.hurt_duration - enemy.hurt_time);
    if tb_setting_bool(module, "SkipInvulnerable", true)
        && hurt_time > landing_delay + (hooks.player_ping_ms / 50.0) as i64
    {
        return false;
    }

    let modern = hooks.has_cooldown;
    let strength = if modern {
        (hooks.attack_cooldown_progress)(landing_delay)
    } else {
        1.0
    };
    let critical = modern
        && tb_setting_bool(module, "Criticals", true)
        && tb_advantage_is_critical(hooks, path[landing], strength);
    // Our hit reaches the server half a ping after we send it
    let knockback_end = landing_delay as f64
        + hooks.player_ping_ms / 2.0 / 50.0
        + tb_setting_i64(module, "KnockbackTicks", 2) as f64;
    let enemy_strength = if enemy.is_player
        && modern
        && tb_setting_bool(module, "EnemyCooldown", true)
    {
        (hooks.enemy_attack_cooldown_progress)(hit.entity, knockback_end)
    } else {
        None
    };
    // Vanilla's threshold for sprint knockback and crits
    let sprint_hit = hooks.is_sprinting && strength > 0.9;
    let range = tb_advantage_range_after_knockback(
        module, hooks, &enemy, hit.entity, path, landing, landing_delay, sprint_hit,
    );
    range > tb_setting_f64(module, "EnemyRange", 3.0)
        || critical
        || enemy_strength.map_or(false, |s| s <= 0.9)
}

/// Upstream `Advantage.rangeAfterKnockback(...)`: how far the enemy is from us
/// `KnockbackTicks` ticks after the knockback of our hit, which lands with the
/// player at `landing` of `path`, `landingDelay` ticks of real time from now.
#[allow(clippy::too_many_arguments)]
fn tb_advantage_range_after_knockback(
    module: &Module,
    hooks: &TbHooks,
    enemy: &TbEnemy,
    entity: u64,
    path: &[TbSnapshot],
    landing: usize,
    landing_delay: i64,
    sprint_hit: bool,
) -> f64 {
    // The prediction starts where the client draws the enemy, which falls
    // behind the server while ticks are held
    let predicted = (hooks.extrapolated_position)(entity, landing_delay as f64);
    let mut feet = [
        predicted[0] + enemy.server_offset[0],
        predicted[1] + enemy.server_offset[1],
        predicted[2] + enemy.server_offset[2],
    ];
    // The server halves the motion it has of the enemy. Of players it only has
    // what is left of earlier knockback, and their client takes the result as
    // its new motion
    let before = if enemy.is_player {
        [0.0, 0.0, 0.0]
    } else {
        let next = (hooks.extrapolated_position)(entity, landing_delay as f64 + 1.0);
        [
            next[0] + enemy.server_offset[0] - feet[0],
            next[1] + enemy.server_offset[1] - feet[1],
            next[2] + enemy.server_offset[2] - feet[2],
        ]
    };
    let mut motion = tb_advantage_knockback(
        module, hooks, entity, path[landing].pos, feet, before, sprint_hit,
    );

    for _ in 0..tb_setting_i64(module, "KnockbackTicks", 2) {
        feet = [feet[0] + motion[0], feet[1] + motion[1], feet[2] + motion[2]];
        // Knockback lifts the enemy off the ground, where only the air slows
        // it down
        motion = [motion[0] * 0.91, motion[1] * 0.91, motion[2] * 0.91];
    }

    let landing_pos = path[(landing + tb_setting_i64(module, "KnockbackTicks", 2) as usize)
        .min(path.len() - 1)]
    .pos;
    let point = [feet[0], feet[1] + enemy.eye_height, feet[2]];
    ((hooks.bounding_box_at)(landing_pos)).distance_to_sqr(point).sqrt()
}

/// Upstream `Advantage.knockback(enemy, from, at, motion, sprintHit)`: the
/// horizontal motion of the enemy at `at` after our hit from `from`, with the
/// server having `motion` of it before.
fn tb_advantage_knockback(
    module: &Module,
    hooks: &TbHooks,
    entity: u64,
    from: [f64; 3],
    at: [f64; 3],
    motion: [f64; 3],
    sprint_hit: bool,
) -> [f64; 3] {
    let away = tb_normalize([at[0] - from[0], 0.0, at[2] - from[2]]);
    // Armor is the only knockback resistance known of other entities, the
    // attribute is not synced
    let resistance = (hooks.enemy_info)(entity)
        .map_or(0.0, |e| e.armor_knockback_resistance)
        .clamp(0.0, 1.0);
    let strength = tb_setting_f64(module, "KnockbackMultiplier", 1.0) * (1.0 - resistance);
    let knocked = [
        motion[0] * 0.5 + away[0] * 0.4 * strength,
        away[1] * 0.4 * strength,
        motion[2] * 0.5 + away[2] * 0.4 * strength,
    ];
    if sprint_hit {
        [
            knocked[0] * 0.5 + away[0] * 0.5 * strength,
            knocked[1] * 0.5 + away[1] * 0.5 * strength,
            knocked[2] * 0.5 + away[2] * 0.5 * strength,
        ]
    } else {
        knocked
    }
}

/// Upstream `Advantage.isCritical(landing, strength)`: falling, a full enough
/// cooldown, and not sprinting, which KillAura and Criticals stop for a
/// critical hit.
fn tb_advantage_is_critical(hooks: &TbHooks, landing: TbSnapshot, strength: f64) -> bool {
    let can_crit = hooks.criticals_allows_hit
        && (!hooks.is_sprinting || hooks.prefers_critical);
    landing.falling() && strength > 0.9 && can_crit
}

/// Upstream `BurstMode.plan(snapshots, walk)`: the tick of `snapshots` to land
/// the hit on after `walk` normal ticks, shown in the visuals. `None` when no
/// hit lands and pays off, or the requirements are not met.
fn tb_burst_plan(
    module: &Module,
    hooks: &mut TbHooks,
    snapshots: Vec<TbSnapshot>,
    walk: i64,
) -> Option<usize> {
    let max_ticks = tb_setting_i64(module, "MaxTicks", 4) as usize;
    let knockback_ticks = tb_setting_i64(module, "KnockbackTicks", 2) as usize;
    let walk = walk as usize;
    let simulated: Vec<TbSnapshot> = snapshots
        .into_iter()
        .take(walk + max_ticks + 1 + knockback_ticks)
        .collect();
    let path: Vec<TbSnapshot> = simulated.iter().take(walk + max_ticks + 1).cloned().collect();
    let hits: Vec<Option<TbHit>> = path
        .iter()
        .map(|snapshot| (hooks.hit_at)(snapshot.eyes(hooks.eye_height)))
        .collect();
    let ticks = tb_burst_ticks_to_hit(module, hooks, &path, &hits, walk, &simulated);
    let hit = ticks
        .and_then(|tick| hits[tick])
        .filter(|hit| tb_requirements_met(module, hooks, hit.point));
    (hooks.visuals_show)(
        &path.iter().map(|snapshot| snapshot.pos).collect::<Vec<_>>(),
        ticks.filter(|_| hit.is_some()),
        hit.map(|hit| hit.point),
    );
    ticks.filter(|_| hit.is_some())
}

/// Upstream `BurstMode.ticksToHit(...)`: the tick of `path` to land the hit
/// on, preferring a critical one, with `walk` normal ticks before the burst.
/// `None` when no hit lands on `path` and pays off, or one lands by just
/// walking without a critical hit to wait for.
fn tb_burst_ticks_to_hit(
    module: &Module,
    hooks: &TbHooks,
    path: &[TbSnapshot],
    hits: &[Option<TbHit>],
    walk: usize,
    simulated: &[TbSnapshot],
) -> Option<usize> {
    let in_range: Vec<usize> = (0..path.len()).filter(|i| hits[*i].is_some()).collect();
    let first = *in_range.first()?;

    // The n-th game tick from now moves the player to path[n + 1], held ticks
    // are no game ticks
    let landing: Vec<usize> = in_range
        .iter()
        .copied()
        .filter(|tick| {
            let delay = tb_burst_landing_delay(module, hooks, *tick as i64, walk as i64);
            *tick > walk
                && (hooks.killaura_clicks_at)(*tick as i64 - 1)
                && tb_cooldown_ready(module, hooks, delay)
                && hits[*tick].map_or(false, |hit| {
                    tb_advantage_pays_off(module, hooks, &hit, simulated, *tick, delay)
                })
        })
        .collect();
    let critical = if hooks.prefers_critical {
        landing.iter().copied().find(|tick| path[*tick].falling())
    } else {
        None
    };

    if let Some(critical) = critical {
        Some(critical)
    } else if first <= walk {
        None
    } else {
        landing.first().copied()
    }
}

/// Upstream `BurstMode.landingDelay(tick, walk)`: ticks of real time until the
/// hit on `tick` of the path lands; the normal ticks before the burst, and
/// when holding first, the held ticks the balance does not pay for yet.
fn tb_burst_landing_delay(module: &Module, hooks: &TbHooks, tick: i64, walk: i64) -> i64 {
    let _ = module;
    if tb_mode(module) == "HoldFirst" {
        walk + (tick - walk - hooks.timer_balance as i64).max(0)
    } else {
        walk
    }
}

/// Upstream `BurstMode.delay.random()`, `Delay` `IntRange` in `0..20`.
fn tb_burst_delay_random(module: &Module, roll: f64) -> i64 {
    let (from, to) = module
        .setting("Delay")
        .map(|value| &value.kind)
        .and_then(|kind| kind.int_range())
        .unwrap_or((0, 0));
    from + (roll * ((to - from + 1) as f64)).floor() as i64
}

/// The upstream `BurstMode` coroutine translated to a per-tick state machine:
/// `waitTicks` is a phase with a countdown, resumed by
/// `tick_base_burst_tick_handler` each game tick.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TbBurstPhase {
    Idle,
    /// `Timer.hold(this, MaxTicks)`, re-planned by the hold handler each tick.
    Holding,
    /// `waitTicks(1)` right after the hold released.
    AfterHold,
    /// `waitTicks(delayTicks)` before bursting.
    PreBurst,
    /// `waitTicks(burst + ...)`; carries the ticks BurstFirst holds back after
    /// the burst.
    PostBurst { hold_back: i64 },
    /// `waitTicks(1 + cooldown.random())` after BurstFirst's hold-back.
    PostHoldBack,
    /// `waitTicks(cooldown.random())` after a failed re-plan.
    Cooldown,
}

/// Upstream `BurstMode` state: `busy`, `walkAfterHold`, `plannedAtBalance`,
/// plus the coroutine's phase, countdowns and captured locals.
#[derive(Debug)]
pub struct TbBurstState {
    /// Set while bursting and paying for it, when the game has to run at the
    /// normal speed.
    pub busy: bool,
    /// While holding first, the normal ticks that will run between the hold
    /// and the burst.
    pub walk_after_hold: Option<i64>,
    /// Upstream `plannedAtBalance` (`Float.NaN` while unset).
    pub planned_at_balance: Option<f64>,
    pub phase: TbBurstPhase,
    pub wait: i64,
    /// The `walk` captured for the pending re-plan.
    pub plan_walk: i64,
    pub delay_ticks: i64,
    /// The burst ticks from the last re-plan, paid for by `PreBurst`.
    pub burst_ticks: i64,
}

impl Default for TbBurstState {
    fn default() -> Self {
        Self {
            busy: false,
            walk_after_hold: None,
            planned_at_balance: None,
            phase: TbBurstPhase::Idle,
            wait: 0,
            plan_walk: 0,
            delay_ticks: 0,
            burst_ticks: 0,
        }
    }
}

/// Upstream `BurstMode.timerSpeed()`: the game runs at the normal speed while
/// the burst is running and paid for.
pub fn tick_base_burst_timer_speed(state: &TbBurstState) -> Option<f64> {
    Some(1.0).filter(|_| state.busy)
}

/// Upstream `BurstMode.tickHandler` with its `holdHandler`, one call per game
/// tick. `roll` feeds `rollChance()`, `cooldown_roll` and `delay_roll` feed
/// `Cooldown.random()` / `Delay.random()`.
pub fn tick_base_burst_tick_handler(
    module: &Module,
    state: &mut TbBurstState,
    hooks: &mut TbHooks,
    rolled: &mut Option<bool>,
    roll: f64,
    cooldown_roll: f64,
    delay_roll: f64,
) {
    (hooks.visuals_clear)();

    if state.phase != TbBurstPhase::Idle {
        tick_base_burst_resume(module, state, hooks, rolled, cooldown_roll);
        return;
    }

    // We do not want this module to conflict with blink
    if hooks.in_vehicle || hooks.blink_running {
        return;
    }

    let delay_ticks = tb_burst_delay_random(module, delay_roll);
    // The current tick still runs. Holding first then runs the delay and the
    // tick the burst follows, so the player covers them at the normal speed
    let walk = if tb_mode(module) == "HoldFirst" {
        delay_ticks + 2
    } else {
        1
    };
    let snapshots = (hooks.snapshots_from_now)();
    let ticks = tb_burst_plan(module, hooks, snapshots, walk);
    let Some(ticks) = ticks else {
        *rolled = None;
        return;
    };
    if !tb_roll_chance(module, rolled, roll) {
        return;
    }
    *rolled = None;

    state.busy = true;
    state.delay_ticks = delay_ticks;
    if tb_mode(module) == "HoldFirst" {
        // `holdFirst(delayTicks)`: holds back ticks until the hold handler
        // finds the balance pays for the burst, then bursts after the delay.
        state.plan_walk = delay_ticks + 1;
        state.walk_after_hold = Some(state.plan_walk);
        state.planned_at_balance = None;
        (hooks.timer_hold)(tb_setting_i64(module, "MaxTicks", 4));
        state.phase = TbBurstPhase::Holding;
    } else {
        // `burstFirst(ticks - walk)`: bursts right away and holds back the
        // ticks the charged balance does not cover after the delay.
        let charged = (hooks.timer_balance as i64).clamp(0, tb_setting_i64(module, "MaxBalance", 3));
        let burst = (hooks.timer_burst)(
            ticks as i64 - walk,
            -tb_setting_i64(module, "MaxTicks", 4),
            tb_setting_i64(module, "MaxBalance", 3),
        );
        state.phase = TbBurstPhase::PostBurst {
            hold_back: (burst - charged).max(0),
        };
        state.wait = burst + delay_ticks;
    }
}

/// `waitTicks`: countdown; `true` when the wait is over and the body runs.
fn tb_burst_wait_done(state: &mut TbBurstState) -> bool {
    if state.wait > 0 {
        state.wait -= 1;
        state.wait == 0
    } else {
        true
    }
}

/// Upstream `finally { busy = false; walkAfterHold = null }`.
fn tick_base_burst_finally(state: &mut TbBurstState) {
    state.busy = false;
    state.walk_after_hold = None;
    state.phase = TbBurstPhase::Idle;
}

/// Upstream `disable() { busy = false; walkAfterHold = null; super.disable() }`
/// (the coroutine's cancellation folded in).
pub fn tick_base_burst_disable(state: &mut TbBurstState) {
    state.busy = false;
    state.walk_after_hold = None;
    state.planned_at_balance = None;
    state.phase = TbBurstPhase::Idle;
    state.wait = 0;
}

/// The `waitTicks` resumes of the upstream `BurstMode` coroutine, plus its
/// hold handler while the hold is running.
fn tick_base_burst_resume(
    module: &Module,
    state: &mut TbBurstState,
    hooks: &mut TbHooks,
    rolled: &mut Option<bool>,
    cooldown_roll: f64,
) {
    match state.phase {
        TbBurstPhase::Idle => {}
        TbBurstPhase::Holding => {
            // Upstream `holdHandler`: plans the burst again with every tick
            // held, against where the target is now. Releases the hold once
            // the balance pays for it, or the hit is gone.
            let walk = state.walk_after_hold.unwrap_or(0);
            if Some(hooks.timer_balance) == state.planned_at_balance {
                return;
            }
            state.planned_at_balance = Some(hooks.timer_balance);

            let snapshots = (hooks.snapshots_from_now)();
            let ticks = tb_burst_plan(module, hooks, snapshots, walk);
            let release = ticks.map_or(true, |t| t as i64 - walk <= hooks.timer_balance as i64);
            if release {
                state.walk_after_hold = None;
                (hooks.timer_hold)(0);
                state.phase = TbBurstPhase::AfterHold;
                state.wait = 1;
            }
        }
        TbBurstPhase::AfterHold => {
            if !tb_burst_wait_done(state) {
                return;
            }
            // Resumed with the first tick after the hold
            state.walk_after_hold = None;
            let snapshots = (hooks.snapshots_from_now)();
            let ticks = tb_burst_plan(module, hooks, snapshots, state.plan_walk)
                .map(|t| t as i64 - state.plan_walk);
            if ticks.map_or(true, |t| t > hooks.timer_balance as i64) {
                *rolled = None;
                state.phase = TbBurstPhase::Cooldown;
                state.wait = tb_cooldown_random(module, cooldown_roll);
                return;
            }
            state.burst_ticks = ticks.unwrap();
            state.wait = state.delay_ticks;
            state.phase = TbBurstPhase::PreBurst;
        }
        TbBurstPhase::PreBurst => {
            if !tb_burst_wait_done(state) {
                return;
            }
            let burst = (hooks.timer_burst)(
                state.burst_ticks,
                0,
                tb_setting_i64(module, "MaxBalance", 3) + tb_setting_i64(module, "MaxTicks", 4),
            );
            (hooks.visuals_clear)();
            state.phase = TbBurstPhase::PostBurst { hold_back: 0 };
            state.wait = burst + tb_cooldown_random(module, cooldown_roll);
        }
        TbBurstPhase::PostBurst { hold_back } => {
            if !tb_burst_wait_done(state) {
                return;
            }
            if hold_back > 0 {
                // BurstFirst: holds back the uncovered ticks
                (hooks.timer_hold)(hold_back);
                state.phase = TbBurstPhase::PostHoldBack;
                state.wait = 1 + tb_cooldown_random(module, cooldown_roll);
            } else {
                tick_base_burst_finally(state);
            }
        }
        TbBurstPhase::PostHoldBack | TbBurstPhase::Cooldown => {
            if !tb_burst_wait_done(state) {
                return;
            }
            tick_base_burst_finally(state);
        }
    }
}
#[cfg(test)]
mod tick_base_tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    fn tb_snap(pos_x: f64) -> TbSnapshot {
        TbSnapshot { pos: [pos_x, 0.0, 0.0], on_ground: true, fall_distance: 0.0 }
    }

    fn tb_hooks<'a>(
        snapshots: &'a [TbSnapshot],
        hit: Option<TbHit>,
        hit_from: f64,
        speeds: &'a mut Vec<f64>,
        holds: &'a Rc<RefCell<Vec<i64>>>,
        bursts: &'a Rc<RefCell<Vec<(i64, i64, i64)>>>,
        visuals: &'a mut Vec<(Vec<[f64; 3]>, Option<usize>, Option<[f64; 3]>)>,
    ) -> TbHooks<'a> {
        let enemy = Some(TbEnemy {
            hurt_time: 0,
            hurt_duration: 10,
            is_player: false,
            armor_knockback_resistance: 0.0,
            eye_height: 1.62,
            server_offset: [0.0, 0.0, 0.0],
        });
        TbHooks {
            request_timer_speed: Box::new(move |s: f64| speeds.push(s)),
            timer_hold: Box::new(move |t: i64| holds.borrow_mut().push(t)),
            timer_burst: Box::new(move |ticks: i64, from: i64, to: i64| {
                bursts.borrow_mut().push((ticks, from, to));
                ticks.clamp(from, to)
            }),
            timer_held_ticks_within: Box::new(|_: i64| 0),
            killaura_clicks_at: Box::new(|_: i64| true),
            prefers_critical: false,
            criticals_allows_hit: true,
            is_sprinting: false,
            player_ping_ms: 0.0,
            extrapolated_position: Box::new(|_: u64, ticks: f64| [2.0 + 3.0 * ticks, 0.0, 0.0]),
            enemy_info: Box::new(move |_: u64| enemy),
            enemy_attack_cooldown_progress: Box::new(|_: u64, _: f64| None),
            bounding_box_at: Box::new(|pos: [f64; 3]| TbBox {
                min: [pos[0] - 0.3, pos[1], pos[2] - 0.3],
                max: [pos[0] + 0.3, pos[1] + 1.8, pos[2] + 0.3],
            }),
            timer_balance: 1.0,
            timer_spendable_ticks: Box::new(|max: i64| 10.0 * max as f64),
            snapshots_from_now: Box::new(move || snapshots.to_vec()),
            hit_at: Box::new(move |eyes: [f64; 3]| {
                if hit.is_some() && eyes[0] >= hit_from {
                    hit
                } else {
                    None
                }
            }),
            enemy_in_charge_range: Box::new(|| true),
            enemy_under_crosshair: Box::new(|_: [f64; 3], _: [f64; 3], range: f64| {
                if range > 0.0 {
                    Some(TbHit { entity: 1, point: [1.0, 1.62, 0.0] })
                } else {
                    None
                }
            }),
            killaura_engaged: true,
            killaura_next_cooldown: None,
            attack_cooldown_progress: Box::new(|_: i64| 1.0),
            has_cooldown: false,
            using_item: false,
            on_ground: true,
            in_vehicle: false,
            blink_running: false,
            eye_position: [0.0, 1.62, 0.0],
            eye_height: 1.62,
            look_angle: [1.0, 0.0, 0.0],
            attack_pressed: false,
            attack_recently_pressed: false,
            visuals_show: Box::new(move |p: &[[f64; 3]], t: Option<usize>, pt: Option<[f64; 3]>| {
                visuals.push((p.to_vec(), t, pt));
            }),
            visuals_clear: Box::new(|| {}),
        }
    }

    #[test]
    fn tick_base_timer_boost_boosts_when_the_hit_pays_off() {
        let mut module = tick_base();
        module.setting_mut("EnemyRange").unwrap().kind = ValueKind::Float(1.0, 0.0, 8.0);
        let snaps =
            vec![tb_snap(0.0), tb_snap(1.0), tb_snap(2.0), tb_snap(3.0)];
        let hit = TbHit { entity: 7, point: [2.0, 1.62, 0.0] };
        let mut state = TbTimerBoostState::default();
        let mut rolled: Option<bool> = None;

        let (speed, visuals) = {
            let mut speeds = Vec::new();
            let holds = Rc::new(RefCell::new(Vec::new()));
            let bursts = Rc::new(RefCell::new(Vec::new()));
            let mut visuals = Vec::new();
            let mut hooks = tb_hooks(&snaps, Some(hit), 2.0, &mut speeds, &holds, &bursts, &mut visuals);
            let speed =
                tick_base_timer_boost_timer_speed(&module, &mut state, &mut hooks, &mut rolled, 0.5);
            drop(hooks);
            (speed, visuals)
        };
        assert_eq!(speed, Some(5.0));
        assert!(state.boosting);
        assert_eq!(rolled, Some(true));
        // The path up to the hit (3 snapshots), 2 ticks to it, the hit point.
        assert_eq!(visuals.len(), 1);
        assert_eq!(
            visuals[0].0,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]
        );
        assert_eq!(visuals[0].1, Some(2));
        assert_eq!(visuals[0].2, Some(hit.point));
    }

    #[test]
    fn tick_base_timer_boost_ends_into_cooldown_before_boosting_again() {
        let mut module = tick_base();
        module.setting_mut("Cooldown").unwrap().kind = ValueKind::IntRange(2, 2, 0, 100);
        let snaps = vec![tb_snap(0.0), tb_snap(1.0), tb_snap(2.0)];
        let hit = TbHit { entity: 7, point: [2.0, 1.62, 0.0] };
        let mut state = TbTimerBoostState::default();
        let mut rolled: Option<bool> = None;

        // Tick 1: boosts.
        {
            let mut speeds = Vec::new();
            let holds = Rc::new(RefCell::new(Vec::new()));
            let bursts = Rc::new(RefCell::new(Vec::new()));
            let mut visuals = Vec::new();
            let mut hooks = tb_hooks(&snaps, Some(hit), 2.0, &mut speeds, &holds, &bursts, &mut visuals);
            assert_eq!(
                tick_base_timer_boost_timer_speed(&module, &mut state, &mut hooks, &mut rolled, 0.5),
                Some(5.0)
            );
        }

        // Tick 2: the hit is gone, so the boost ends and the cooldown opens.
        {
            let snaps = vec![tb_snap(0.0)];
            let mut speeds = Vec::new();
            let holds = Rc::new(RefCell::new(Vec::new()));
            let bursts = Rc::new(RefCell::new(Vec::new()));
            let mut visuals = Vec::new();
            let mut hooks = tb_hooks(&snaps, None, 0.0, &mut speeds, &holds, &bursts, &mut visuals);
            assert_eq!(
                tick_base_timer_boost_timer_speed(&module, &mut state, &mut hooks, &mut rolled, 0.5),
                None
            );
            assert_eq!(state.cooldown_ticks, 2);
            assert!(rolled.is_none());
        }

        // Tick 3: still cooling down (the falling edge opened it).
        {
            let mut speeds = Vec::new();
            let holds = Rc::new(RefCell::new(Vec::new()));
            let bursts = Rc::new(RefCell::new(Vec::new()));
            let mut visuals = Vec::new();
            let mut hooks = tb_hooks(&snaps, Some(hit), 2.0, &mut speeds, &holds, &bursts, &mut visuals);
            assert_eq!(
                tick_base_timer_boost_timer_speed(&module, &mut state, &mut hooks, &mut rolled, 0.5),
                None
            );
            assert_eq!(state.cooldown_ticks, 1);
        }

        // Tick 4: the cooldown is spent, it boosts again.
        {
            let mut speeds = Vec::new();
            let holds = Rc::new(RefCell::new(Vec::new()));
            let bursts = Rc::new(RefCell::new(Vec::new()));
            let mut visuals = Vec::new();
            let mut hooks = tb_hooks(&snaps, Some(hit), 2.0, &mut speeds, &holds, &bursts, &mut visuals);
            assert_eq!(
                tick_base_timer_boost_timer_speed(&module, &mut state, &mut hooks, &mut rolled, 0.5),
                Some(5.0)
            );
            assert_eq!(state.cooldown_ticks, 0);
        }
    }

    #[test]
    fn tick_base_charges_while_an_enemy_is_near_but_out_of_reach() {
        let mut module = tick_base();
        module.setting_mut("EnemyRange").unwrap().kind = ValueKind::Float(1.0, 0.0, 8.0);
        let snaps = vec![tb_snap(0.0)];

        // Enemy near, no reachable hit, balance below MaxBalance.
        let mut speeds = Vec::new();
        let holds = Rc::new(RefCell::new(Vec::new()));
        let bursts = Rc::new(RefCell::new(Vec::new()));
        let mut visuals = Vec::new();
        let hooks = tb_hooks(&snaps, None, 0.0, &mut speeds, &holds, &bursts, &mut visuals);
        assert_eq!(tick_base_charging_speed(&module, &hooks), Some(0.9));

        // A reachable hit means it is not just charging.
        let hit = TbHit { entity: 7, point: [2.0, 1.62, 0.0] };
        let mut speeds = Vec::new();
        let holds = Rc::new(RefCell::new(Vec::new()));
        let bursts = Rc::new(RefCell::new(Vec::new()));
        let mut visuals = Vec::new();
        let hooks = tb_hooks(&snaps, Some(hit), 0.0, &mut speeds, &holds, &bursts, &mut visuals);
        assert_eq!(tick_base_charging_speed(&module, &hooks), None);
    }

    #[test]
    fn tick_base_speed_handler_requests_boost_then_charging() {
        let burst_state = TbBurstState::default();
        let mut module = tick_base();
        module.setting_mut("EnemyRange").unwrap().kind = ValueKind::Float(1.0, 0.0, 8.0);
        let snaps = vec![tb_snap(0.0), tb_snap(1.0), tb_snap(2.0), tb_snap(3.0)];
        let hit = TbHit { entity: 7, point: [2.0, 1.62, 0.0] };
        let mut state = TbTimerBoostState::default();
        let mut rolled: Option<bool> = None;

        // Timer mode with a payable hit: the boost speed.
        let first = {
            let mut speeds = Vec::new();
            let holds = Rc::new(RefCell::new(Vec::new()));
            let bursts = Rc::new(RefCell::new(Vec::new()));
            let mut visuals = Vec::new();
            let mut hooks = tb_hooks(&snaps, Some(hit), 2.0, &mut speeds, &holds, &bursts, &mut visuals);
            tick_base_speed_handler(&module, &mut state, &burst_state, &mut hooks, &mut rolled, 0.5);
            drop(hooks);
            speeds
        };
        assert_eq!(first, vec![5.0]);

        // HoldFirst has no pin while not busy (part 2), so charging takes over
        // while the hit is out of reach.
        let mut module = tick_base();
        module.setting_mut("Mode").unwrap().kind = ValueKind::Choice("HoldFirst".into(), Vec::new());
        let second = {
            let mut speeds = Vec::new();
            let holds = Rc::new(RefCell::new(Vec::new()));
            let bursts = Rc::new(RefCell::new(Vec::new()));
            let mut visuals = Vec::new();
            let mut hooks = tb_hooks(&snaps, None, 0.0, &mut speeds, &holds, &bursts, &mut visuals);
            tick_base_speed_handler(&module, &mut state, &burst_state, &mut hooks, &mut rolled, 0.5);
            drop(hooks);
            speeds
        };
        assert_eq!(second, vec![0.9]);
    }

    #[test]
    fn tick_base_requirement_and_chance_gates() {
        let mut module = tick_base();
        let snaps = vec![tb_snap(0.0)];
        let hit = TbHit { entity: 7, point: [2.0, 1.62, 0.0] };

        // KillAura off → the default Requirement fails.
        let mut speeds = Vec::new();
        let holds = Rc::new(RefCell::new(Vec::new()));
        let bursts = Rc::new(RefCell::new(Vec::new()));
        let mut visuals = Vec::new();
        let mut hooks = tb_hooks(&snaps, Some(hit), 2.0, &mut speeds, &holds, &bursts, &mut visuals);
        hooks.killaura_engaged = false;
        assert!(!tb_requirements_met(&module, &hooks, hit.point));

        // LookingAt met through the crosshair seam; Click and OnGround met.
        module.setting_mut("Requires").unwrap().kind =
            ValueKind::MultiChoice(vec!["LookingAt".to_string()], Vec::new());
        assert!(tb_requirements_met(&module, &hooks, hit.point));
        module.setting_mut("Requires").unwrap().kind =
            ValueKind::MultiChoice(vec!["Click".to_string()], Vec::new());
        assert!(!tb_requirements_met(&module, &hooks, hit.point));
        hooks.attack_pressed = true;
        assert!(tb_requirements_met(&module, &hooks, hit.point));
        module.setting_mut("Requires").unwrap().kind =
            ValueKind::MultiChoice(vec!["OnGround".to_string()], Vec::new());
        assert!(tb_requirements_met(&module, &hooks, hit.point));
        hooks.on_ground = false;
        assert!(!tb_requirements_met(&module, &hooks, hit.point));

        // A 0% chance never rolls a boost, and the roll is carried.
        module.setting_mut("Chance").unwrap().kind = ValueKind::Float(0.0, 0.0, 100.0);
        let mut rolled: Option<bool> = None;
        assert!(!tb_roll_chance(&module, &mut rolled, 0.5));
        assert_eq!(rolled, Some(false));
        assert!(!tb_roll_chance(&module, &mut rolled, 1.0));
    }

    #[test]
    fn tick_base_on_disabled_resets_the_roll_and_visuals() {
        let mut rolled: Option<bool> = Some(true);
        {
            let mut speeds = Vec::new();
            let holds = Rc::new(RefCell::new(Vec::new()));
            let bursts = Rc::new(RefCell::new(Vec::new()));
            let mut visuals = Vec::new();
            let snaps: Vec<TbSnapshot> = Vec::new();
            let mut hooks = tb_hooks(&snaps, None, 0.0, &mut speeds, &holds, &bursts, &mut visuals);
            tick_base_on_disabled(&mut hooks, &mut rolled);
        }
        assert!(rolled.is_none());
    }
    #[test]
    fn tick_base_advantage_pays_off_flags_an_invulnerable_enemy() {
        let mut module = tick_base();
        module.setting_mut("EnemyRange").unwrap().kind =
            ValueKind::Float(1.0, 0.0, 8.0);
        let snaps = vec![tb_snap(0.0), tb_snap(1.0), tb_snap(2.0)];
        let path = snaps.clone();
        let hit = TbHit { entity: 7, point: [2.0, 1.62, 0.0] };
        let mut speeds = Vec::new();
        let holds = Rc::new(RefCell::new(Vec::new()));
        let bursts = Rc::new(RefCell::new(Vec::new()));
        let mut visuals = Vec::new();

        // The enemy is fine: the knockback pushes it out of range
        {
            let hooks = tb_hooks(&snaps, Some(hit), 2.0, &mut speeds, &holds, &bursts, &mut visuals);
            assert!(tb_advantage_pays_off(&module, &hooks, &hit, &path, 2, 1));
        }
        // Still invulnerable one round trip late: it takes no knockback and
        // hits back
        {
            let mut hooks = tb_hooks(&snaps, Some(hit), 2.0, &mut speeds, &holds, &bursts, &mut visuals);
            hooks.enemy_info = Box::new(|_: u64| {
                Some(TbEnemy {
                    hurt_time: 20,
                    hurt_duration: 10,
                    is_player: false,
                    armor_knockback_resistance: 0.0,
                    eye_height: 1.62,
                    server_offset: [0.0, 0.0, 0.0],
                })
            });
            assert!(!tb_advantage_pays_off(&module, &hooks, &hit, &path, 2, 1));
        }
    }

    #[test]
    fn tick_base_advantage_knockback_adds_the_sprint_hit() {
        let module = tick_base();
        let mut speeds = Vec::new();
        let holds = Rc::new(RefCell::new(Vec::new()));
        let bursts = Rc::new(RefCell::new(Vec::new()));
        let mut visuals = Vec::new();
        let hooks = tb_hooks(&[], None, 0.0, &mut speeds, &holds, &bursts, &mut visuals);

        // Half of the enemy's motion, plus the knockback away from the hit
        let plain = tb_advantage_knockback(
            &module, &hooks, 7, [0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [1.0, 0.0, 0.0], false,
        );
        assert_eq!(plain, [0.9, 0.0, 0.0]);
        // A sprint hit halves it again and adds the sprint knockback
        let sprint = tb_advantage_knockback(
            &module, &hooks, 7, [0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [1.0, 0.0, 0.0], true,
        );
        assert_eq!(sprint, [0.95, 0.0, 0.0]);
    }

    #[test]
    fn tick_base_hold_first_holds_until_the_balance_pays_then_bursts() {
        let mut module = tick_base();
        module.setting_mut("Mode").unwrap().kind = ValueKind::Choice(
            "HoldFirst".to_string(),
            vec![
                "Timer".to_string(),
                "HoldFirst".to_string(),
                "BurstFirst".to_string(),
            ],
        );
        let snaps: Vec<TbSnapshot> = (0..6).map(|i| tb_snap(i as f64)).collect();
        let hit = TbHit { entity: 7, point: [4.0, 1.62, 0.0] };
        let mut state = TbBurstState::default();
        let mut rolled: Option<bool> = None;
        let mut speeds = Vec::new();
        let holds = Rc::new(RefCell::new(Vec::new()));
        let bursts = Rc::new(RefCell::new(Vec::new()));
        let mut visuals = Vec::new();
        let mut hooks = tb_hooks(&snaps, Some(hit), 3.5, &mut speeds, &holds, &bursts, &mut visuals);

        // Tick 1: the burst is planned, the hold starts, the game pins at 1.0
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        assert!(state.busy);
        assert_eq!(state.phase, TbBurstPhase::Holding);
        assert_eq!(*holds.borrow(), vec![4]);
        assert_eq!(tick_base_burst_timer_speed(&state), Some(1.0));

        // The balance pays for the burst: the hold handler releases
        hooks.timer_balance = 3.0;
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        assert_eq!(state.phase, TbBurstPhase::AfterHold);
        assert_eq!(*holds.borrow(), vec![4, 0]);

        // waitTicks(1) runs, the re-plan pays, the delay (0 ticks) runs
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        assert_eq!(state.phase, TbBurstPhase::PreBurst);

        // The burst bursts the 3 ticks it held, then the cooldown wait
        // (0 ticks)
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        assert_eq!(*bursts.borrow(), vec![(3, 0, 7)]);
        assert_eq!(state.phase, TbBurstPhase::PostBurst { hold_back: 0 });

        // The burst wait runs, and the coroutine ends
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        assert!(!state.busy);
        assert_eq!(state.phase, TbBurstPhase::Idle);
    }

    #[test]
    fn tick_base_burst_first_bursts_then_holds_back_the_uncovered_ticks() {
        let mut module = tick_base();
        module.setting_mut("Mode").unwrap().kind = ValueKind::Choice(
            "BurstFirst".to_string(),
            vec![
                "Timer".to_string(),
                "HoldFirst".to_string(),
                "BurstFirst".to_string(),
            ],
        );
        let snaps: Vec<TbSnapshot> = (0..6).map(|i| tb_snap(i as f64)).collect();
        let hit = TbHit { entity: 7, point: [4.0, 1.62, 0.0] };
        let mut state = TbBurstState::default();
        let mut rolled: Option<bool> = None;
        let mut speeds = Vec::new();
        let holds = Rc::new(RefCell::new(Vec::new()));
        let bursts = Rc::new(RefCell::new(Vec::new()));
        let mut visuals = Vec::new();
        let mut hooks = tb_hooks(&snaps, Some(hit), 3.5, &mut speeds, &holds, &bursts, &mut visuals);
        hooks.timer_balance = 2.0;

        // Tick 1: plans 4 ticks, bursts 3 of them (2 are charged), and holds
        // back the uncovered 1 after the delay (0 ticks)
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        assert!(state.busy);
        assert_eq!(*bursts.borrow(), vec![(3, -4, 3)]);
        assert_eq!(state.phase, TbBurstPhase::PostBurst { hold_back: 1 });
        assert_eq!(state.wait, 3);

        // The burst wait runs (3 ticks, then the hold-back)
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        assert_eq!(state.phase, TbBurstPhase::PostBurst { hold_back: 1 });
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        assert_eq!(*holds.borrow(), vec![1]);
        assert_eq!(state.phase, TbBurstPhase::PostHoldBack);

        // waitTicks(1 + cooldown.random()) and the coroutine ends
        tick_base_burst_tick_handler(&module, &mut state, &mut hooks, &mut rolled, 0.5, 0.5, 0.5);
        assert!(!state.busy);
        assert_eq!(state.phase, TbBurstPhase::Idle);
    }

    #[test]
    fn tick_base_burst_mode_prefers_a_critical_hit() {
        let mut module = tick_base();
        module.setting_mut("Mode").unwrap().kind = ValueKind::Choice(
            "HoldFirst".to_string(),
            vec![
                "Timer".to_string(),
                "HoldFirst".to_string(),
                "BurstFirst".to_string(),
            ],
        );
        let mut snaps: Vec<TbSnapshot> = (0..6).map(|i| tb_snap(i as f64)).collect();
        snaps[5].on_ground = false;
        snaps[5].fall_distance = 1.0;
        let hit = TbHit { entity: 7, point: [5.0, 1.62, 0.0] };
        let mut speeds = Vec::new();
        let holds = Rc::new(RefCell::new(Vec::new()));
        let bursts = Rc::new(RefCell::new(Vec::new()));
        let mut visuals = Vec::new();
        let mut hooks = tb_hooks(&snaps, Some(hit), 3.5, &mut speeds, &holds, &bursts, &mut visuals);
        hooks.prefers_critical = true;

        // Tick 5 lands while falling, so the wait prefers it over tick 4
        let ticks = tb_burst_plan(&module, &mut hooks, snaps.clone(), 1);
        drop(hooks);
        assert_eq!(ticks, Some(5));
        assert_eq!(visuals[0].1, Some(5));
        assert_eq!(visuals[0].2, Some(hit.point));
    }
}

// ----- TickBaseVisuals.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// `TbHooks.visuals_show` / `visuals_clear` are the seams TickBase calls
// this through. The GPU calls go out through a `TbRenderOp` recorder; the
// geometry, fades and state machine stay computed here. `TbVisuals.world`
// carries the per-frame world reads as fields instead of live entities.

/// Upstream `Pose.STANDING`, the ghost's default pose; the world seam hands
/// the player's pose through as its own value.
pub const TB_POSE_STANDING: u8 = 0;

/// Upstream `Color4b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl TbColor {
    /// Upstream `Color4b.LIQUID_BOUNCE`.
    pub fn liquid_bounce() -> Self {
        TbColor { r: 0x46, g: 0x77, b: 0xFF, a: 0xFF }
    }

    /// Upstream `Color4b.fade(fade)`: at or above 1.0 the color is unchanged,
    /// below it the alpha becomes `(a * fade).toInt()`.
    pub fn fade(&self, fade: f32) -> Self {
        if fade >= 1.0 {
            *self
        } else {
            TbColor { r: self.r, g: self.g, b: self.b, a: (self.a as f32 * fade) as u8 }
        }
    }
}

/// Upstream `TickBaseVisuals.Afterimage`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TbAfterimage {
    pub pos: [f64; 3],
    pub y_rot: f32,
    pub pose: u8,
    pub time: u64,
}

/// Upstream `WireframePlayer`, the ghost's state the visuals drive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TbGhostPlayer {
    pub pos: [f64; 3],
    pub y_rot: f32,
    pub x_rot: f32,
    pub pose: u8,
}

impl TbGhostPlayer {
    pub fn standing() -> Self {
        TbGhostPlayer { pos: [0.0; 3], y_rot: 0.0, x_rot: 0.0, pose: TB_POSE_STANDING }
    }

    /// Upstream `WireframePlayer.setRotation(rotation)`.
    pub fn set_rotation(&mut self, y_rot: f32, x_rot: f32) {
        self.y_rot = y_rot;
        self.x_rot = x_rot;
    }
}

/// One upstream render call, recorded instead of drawn.
#[derive(Clone, Debug, PartialEq)]
pub enum TbRenderOp {
    /// Upstream `drawCircleOutline(radius, color)`, drawn at the anchor.
    CircleOutline { radius: f32, color: TbColor },
    /// Upstream `drawCustomMesh(ClientRenderPipelines.triangleStrip(noDepthTest
    /// = true))`: `(x, z)` vertex, color pairs, two vertices per iteration.
    TriangleStrip { pairs: Vec<((f32, f32), TbColor)> },
    /// Upstream `drawLinesWithWidth(color.argb, 3f, lineStripAsLines())`,
    /// vertex pairs in anchor-relative space.
    LinesWithWidth { width: f32, segments: Vec<([f64; 3], [f64; 3])>, color: TbColor },
    /// Upstream the two-vertex `drawCustomMesh(ClientRenderPipelines.lines(
    /// noDepthTest = true))` strike from the eyes to the hit.
    EyeLine { to_offset: [f64; 3], from_color: TbColor, to_color: TbColor },
    /// Upstream `WireframePlayer.render(color, outlineColor, noDepthTest =
    /// true)`, drawn at the ghost's position.
    GhostRender { fill: TbColor, outline: TbColor },
}

/// Upstream `TickBaseVisuals`.
pub struct TbVisuals {
    /// Upstream the `Color` setting.
    pub color: TbColor,
    /// Upstream the `BalanceRing` setting.
    pub balance_ring: bool,
    /// Upstream `path`: the coming positions of the player from now up to
    /// where the hit on `hit` lands. Empty without a hit.
    pub path: Vec<[f64; 3]>,
    pub hit: Option<[f64; 3]>,
    /// Upstream `afterimage`: where the player was when the ticks started to
    /// run ahead of real time.
    pub afterimage: Option<TbAfterimage>,
    pub ahead: bool,
    pub ghost: TbGhostPlayer,
}

impl Default for TbVisuals {
    fn default() -> Self {
        Self::new()
    }
}

impl TbVisuals {
    pub fn new() -> Self {
        TbVisuals {
            color: TbColor::liquid_bounce(),
            balance_ring: true,
            path: Vec::new(),
            hit: None,
            afterimage: None,
            ahead: false,
            ghost: TbGhostPlayer::standing(),
        }
    }

    /// Upstream `show(path, landing, hit)`: shows the coming positions of the
    /// player from now, with the hit landing after `landing` ticks.
    pub fn show(&mut self, path: &[[f64; 3]], landing: Option<usize>, hit: Option<[f64; 3]>) {
        self.path = match (landing, hit) {
            (Some(landing), Some(_)) if landing > 0 => {
                path.iter().take(landing + 1).copied().collect()
            }
            _ => Vec::new(),
        };
        self.hit = hit;
    }

    /// Upstream `clear()`.
    pub fn clear(&mut self) {
        self.show(&[], None, None)
    }

    /// Upstream `TickBaseVisuals.tickHandler` on `PlayerPostTickEvent`:
    /// remembers where the player was when the ticks started to run ahead of
    /// real time.
    pub fn tick_handler(
        &mut self,
        bursting: bool,
        timer_speed: f64,
        old_position: [f64; 3],
        y_rot: f32,
        pose: u8,
        time_ms: u64,
    ) {
        let was_ahead = self.ahead;
        self.ahead = bursting || timer_speed > 1.0;
        if self.ahead && !was_ahead {
            self.afterimage = Some(TbAfterimage {
                pos: old_position,
                y_rot,
                pose,
                time: time_ms,
            });
        }
    }
}

/// The per-frame world reads of `WorldRenderEvent` and the player, handed
/// through as fields.
pub struct TbVisualsWorld {
    /// Upstream `clientStartDurationMs`.
    pub time_ms: u64,
    /// Upstream `player.interpolateCurrentPosition(event.partialTicks)`.
    pub origin: [f64; 3],
    /// Upstream `player.getViewYRot(event.partialTicks)`.
    pub view_y_rot: f32,
    /// Upstream `player.eyeHeight`.
    pub eye_height: f64,
    /// Upstream `player.pose`.
    pub player_pose: u8,
    /// Upstream `camera.position()`.
    pub camera_position: [f64; 3],
    /// Upstream `Timer.balance`.
    pub timer_balance: f64,
    /// Upstream `ModuleTickBase.balance` as `(first, last)`.
    pub balance_range: (i64, i64),
}

/// Upstream `Mth.wrapDegrees(float)`.
fn tb_wrap_degrees(value: f32) -> f32 {
    let mut f = value % 360.0;
    if f >= 180.0 {
        f -= 360.0;
    }
    if f < -180.0 {
        f += 360.0;
    }
    f
}

/// Upstream `Rotation.lookingAt(point, from)` = `fromRotationVec(point - from)`:
/// yaw `wrapDegrees(atan2(dz, dx).toDegrees() - 90)`, pitch
/// `wrapDegrees(-atan2(dy, length(dx, dz)).toDegrees())`.
fn tb_looking_at(point: [f64; 3], from: [f64; 3]) -> (f32, f32) {
    let dx = point[0] - from[0];
    let dy = point[1] - from[1];
    let dz = point[2] - from[2];
    let yaw = tb_wrap_degrees((dz.atan2(dx).to_degrees() as f32) - 90.0);
    let pitch = tb_wrap_degrees(-(dy.atan2((dx * dx + dz * dz).sqrt()).to_degrees() as f32));
    (yaw, pitch)
}

impl TbVisuals {
    /// Upstream `renderHandler` on `WorldRenderEvent`: the balance ring, the
    /// coming path with its landing, and the afterimage, recorded in order.
    pub fn render_handler(&mut self, world: &TbVisualsWorld) -> Vec<TbRenderOp> {
        let mut ops = Vec::new();
        if self.balance_ring {
            self.render_balance(world, &mut ops);
        }
        if let Some(hit) = self.hit.filter(|_| !self.path.is_empty()) {
            self.render_path(world, &mut ops);
            self.render_landing(world, hit, &mut ops);
        }
        self.render_afterimage(world, &mut ops);
        ops
    }

    /// Upstream `renderBalance(origin, yRot)`: a ring around the feet filled
    /// by the balance, centred on where the player looks.
    fn render_balance(&self, world: &TbVisualsWorld, ops: &mut Vec<TbRenderOp>) {
        let (first, last) = world.balance_range;
        let fraction = if last > first {
            (((world.timer_balance - first as f64) / (last - first) as f64) as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        ops.push(TbRenderOp::CircleOutline { radius: 0.6, color: self.color.fade(0.2) });
        if fraction <= 0.0 {
            return;
        }

        let sweep = fraction * std::f32::consts::TAU;
        let start = (world.view_y_rot + 90.0).to_radians() - sweep / 2.0;
        let segments = std::cmp::max(2, (48.0 * fraction) as i32);
        let inner = 0.56;
        let outer = 0.64;

        let mut pairs = Vec::new();
        for i in 0..=segments {
            let angle = start + sweep * i as f32 / segments as f32;
            let x = angle.cos();
            let z = angle.sin();
            pairs.push(((x * inner, z * inner), self.color.fade(0.35)));
            pairs.push(((x * outer, z * outer), self.color));
        }
        ops.push(TbRenderOp::TriangleStrip { pairs });
    }

    /// Upstream `renderPath(origin)`: the coming ticks up to the landing as a
    /// line on the ground.
    fn render_path(&self, world: &TbVisualsWorld, ops: &mut Vec<TbRenderOp>) {
        // The line starts where the player is drawn between ticks; the
        // vertex list is relative to the anchor, so the first vertex is
        // `Vec3.ZERO` and every following one is `path[i] - origin`
        let mut vertices = Vec::with_capacity(self.path.len());
        vertices.push([0.0; 3]);
        for point in self.path.iter().skip(1) {
            vertices.push([
                point[0] - world.origin[0],
                point[1] - world.origin[1],
                point[2] - world.origin[2],
            ]);
        }
        let segments = vertices.windows(2).map(|w| (w[0], w[1])).collect();
        ops.push(TbRenderOp::LinesWithWidth { width: 3.0, segments, color: self.color });
    }

    /// Upstream `renderLanding(hit)`: the player where the hit lands, and the
    /// strike from its eyes to the hit.
    fn render_landing(&mut self, world: &TbVisualsWorld, hit: [f64; 3], ops: &mut Vec<TbRenderOp>) {
        let pos = *self.path.last().expect("renderLanding runs with a non-empty path");
        let eyes = [pos[0], pos[1] + world.eye_height, pos[2]];
        let fade = tb_ghost_fade(world.camera_position, eyes);

        if fade > 0.0 {
            let (y_rot, x_rot) = tb_looking_at(hit, eyes);
            self.ghost.pos = pos;
            self.ghost.pose = world.player_pose;
            self.ghost.set_rotation(y_rot, x_rot);
            ops.push(TbRenderOp::GhostRender {
                fill: self.color.fade(0.12 * fade),
                outline: self.color.fade(0.9 * fade),
            });
        }

        ops.push(TbRenderOp::EyeLine {
            to_offset: [hit[0] - eyes[0], hit[1] - eyes[1], hit[2] - eyes[2]],
            from_color: self.color.fade(0.05),
            to_color: self.color,
        });
    }

    /// Upstream `renderAfterimage(time)`: the player where the ticks started
    /// to run ahead of real time, fading out.
    fn render_afterimage(&mut self, world: &TbVisualsWorld, ops: &mut Vec<TbRenderOp>) {
        let image = match self.afterimage {
            Some(image) => image,
            None => return,
        };
        let eyes = [image.pos[0], image.pos[1] + world.eye_height, image.pos[2]];
        let fade = (1.0 - (world.time_ms - image.time) as f32 / 450.0)
            * tb_ghost_fade(world.camera_position, eyes);
        if fade <= 0.0 {
            return;
        }

        self.ghost.pos = image.pos;
        self.ghost.pose = image.pose;
        self.ghost.set_rotation(image.y_rot, 0.0);
        ops.push(TbRenderOp::GhostRender {
            fill: self.color.fade(0.08 * fade),
            outline: self.color.fade(0.6 * fade),
        });
    }
}

/// Upstream `WorldRenderEnvironment.ghostFade(eyes)`: ghosts close to the
/// camera would fill the screen in first person.
fn tb_ghost_fade(camera: [f64; 3], eyes: [f64; 3]) -> f32 {
    let dx = camera[0] - eyes[0];
    let dy = camera[1] - eyes[1];
    let dz = camera[2] - eyes[2];
    let distance = (dx * dx + dy * dy + dz * dz).sqrt();
    (((distance - 0.8) / 1.2) as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tick_base_visuals_tests {
    use super::*;

    fn visuals_world() -> TbVisualsWorld {
        TbVisualsWorld {
            time_ms: 112,
            origin: [0.0, 64.0, 0.0],
            view_y_rot: 0.0,
            eye_height: 1.8,
            player_pose: TB_POSE_STANDING,
            camera_position: [5.0, 70.0, 0.0],
            timer_balance: 1.0,
            balance_range: (0, 4),
        }
    }

    #[test]
    fn visuals_show_cuts_the_path_to_the_landing_tick() {
        let mut v = TbVisuals::new();
        let path = vec![
            [0.0, 64.0, 0.0],
            [1.0, 64.0, 0.0],
            [2.0, 64.0, 0.0],
            [3.0, 64.0, 0.0],
        ];

        v.show(&path, Some(2), Some([5.0, 64.0, 0.0]));
        assert_eq!(v.path.len(), 3);
        assert_eq!(v.hit, Some([5.0, 64.0, 0.0]));

        // Without a landing the path is empty
        v.show(&path, None, Some([5.0, 64.0, 0.0]));
        assert!(v.path.is_empty());

        v.show(&path, Some(2), None);
        assert!(v.path.is_empty());
        assert_eq!(v.hit, None);

        v.show(&path, Some(2), Some([5.0, 64.0, 0.0]));
        v.clear();
        assert!(v.path.is_empty());
        assert_eq!(v.hit, None);
    }

    #[test]
    fn visuals_tick_handler_captures_the_afterimage_when_ahead_starts() {
        let mut v = TbVisuals::new();

        v.tick_handler(false, 1.0, [0.0, 64.0, 0.0], 12.0, TB_POSE_STANDING, 100);
        assert!(!v.ahead);
        assert!(v.afterimage.is_none());

        // Bursting starts the ahead state and takes the snapshot
        v.tick_handler(true, 1.0, [1.0, 64.0, 0.0], 13.0, TB_POSE_STANDING, 150);
        let image = v.afterimage.expect("afterimage captured");
        assert_eq!(image.pos, [1.0, 64.0, 0.0]);
        assert_eq!(image.y_rot, 13.0);
        assert_eq!(image.time, 150);

        // Already ahead: no new snapshot
        v.tick_handler(true, 1.0, [2.0, 64.0, 0.0], 0.0, TB_POSE_STANDING, 200);
        assert_eq!(v.afterimage.unwrap().time, 150);

        // Ahead stays on while bursting, then turns off below speed 1
        v.tick_handler(false, 1.0, [0.0, 64.0, 0.0], 0.0, TB_POSE_STANDING, 250);
        assert!(!v.ahead);
        assert_eq!(v.afterimage.unwrap().time, 150);

        // A timer speed over 1 counts as ahead and takes a new snapshot
        v.tick_handler(false, 1.5, [3.0, 64.0, 0.0], 0.0, TB_POSE_STANDING, 300);
        assert_eq!(v.afterimage.unwrap().pos, [3.0, 64.0, 0.0]);
    }

    #[test]
    fn visuals_render_balance_fills_the_ring_from_the_balance_range() {
        let mut v = TbVisuals::new();
        let world = visuals_world();

        let ops = v.render_handler(&world);

        // No hit and no afterimage: only the balance ring is drawn
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0], TbRenderOp::CircleOutline { radius: 0.6, color: TbColor::liquid_bounce().fade(0.2) });

        // Balance 1 of 0..4: fraction 0.25, segments max(2, 48 * 0.25) = 12,
        // two vertices per segment plus the closing pair
        match &ops[1] {
            TbRenderOp::TriangleStrip { pairs } => {
                assert_eq!(pairs.len(), 26);
                let ((ix, iz), inner) = pairs[0];
                let ((ox, oz), outer) = pairs[1];
                // yRot 0: start = (0 + 90).toRadians() - sweep / 2
                let sweep = 0.25 * std::f32::consts::TAU;
                let start = (90.0f32).to_radians() - sweep / 2.0;
                assert!((ix - inner_x(start, 0.56)).abs() < 1e-5);
                assert!((iz - inner_z(start, 0.56)).abs() < 1e-5);
                assert!((ox - outer_x(start, 0.64)).abs() < 1e-5);
                assert_eq!(inner, TbColor::liquid_bounce().fade(0.35));
                assert_eq!(outer, TbColor::liquid_bounce());
            }
            other => panic!("expected a triangle strip, got {:?}", other),
        }
    }

    fn inner_x(start: f32, radius: f32) -> f32 {
        start.cos() * radius
    }
    fn inner_z(start: f32, radius: f32) -> f32 {
        start.sin() * radius
    }
    fn outer_x(start: f32, radius: f32) -> f32 {
        start.cos() * radius
    }
    fn outer_z(start: f32, radius: f32) -> f32 {
        start.sin() * radius
    }

    #[test]
    fn visuals_render_handler_draws_path_landing_and_afterimage_in_order() {
        let mut v = TbVisuals::new();
        v.show(
            &[[0.0, 64.0, 0.0], [1.0, 64.5, 0.0], [2.0, 64.0, 0.0]],
            Some(1),
            Some([2.0, 64.0, 0.0]),
        );
        v.afterimage = Some(TbAfterimage {
            pos: [0.0, 64.0, 0.0],
            y_rot: 45.0,
            pose: TB_POSE_STANDING,
            time: 0,
        });
        let world = visuals_world();

        let ops = v.render_handler(&world);
        assert_eq!(ops.len(), 6);

        // The path line: the player's drawn position, then the landing point
        match &ops[2] {
            TbRenderOp::LinesWithWidth { width, segments, color } => {
                assert_eq!(*width, 3.0);
                assert_eq!(*color, TbColor::liquid_bounce());
                assert_eq!(segments.len(), 1);
                assert_eq!(segments[0].0, [0.0, 0.0, 0.0]);
                assert_eq!(segments[0].1, [1.0, 0.5, 0.0]);
            }
            other => panic!("expected the path line, got {:?}", other),
        }

        // The landing ghost looks at the hit
        match &ops[3] {
            TbRenderOp::GhostRender { fill, outline } => {
                assert_eq!(fill.a, (255.0f32 * 0.12) as u8);
                assert_eq!(outline.a, (255.0f32 * 0.9) as u8);
            }
            other => panic!("expected the landing ghost, got {:?}", other),
        }

        // The strike from the landing player's eyes to the hit
        match &ops[4] {
            TbRenderOp::EyeLine { to_offset, from_color, to_color } => {
                assert!((to_offset[0] - 1.0).abs() < 1e-9);
                assert!((to_offset[1] + 2.3).abs() < 1e-9);
                assert_eq!(to_offset[2], 0.0);
                assert_eq!(*from_color, TbColor::liquid_bounce().fade(0.05));
                assert_eq!(*to_color, TbColor::liquid_bounce());
            }
            other => panic!("expected the eye line, got {:?}", other),
        }

        // The afterimage ghost fades out over 450 ms
        match &ops[5] {
            TbRenderOp::GhostRender { fill, outline } => {
                // fade = (1 - 112/450) * ghostFade = 0.7511
                assert_eq!(fill.a, (255.0f32 * 0.08 * 0.751111) as u8);
                assert_eq!(outline.a, (255.0f32 * 0.6 * 0.751111) as u8);
            }
            other => panic!("expected the afterimage ghost, got {:?}", other),
        }

        // The ghost ends on the afterimage's pose and rotation
        assert_eq!(v.ghost.pos, [0.0, 64.0, 0.0]);
        assert_eq!(v.ghost.y_rot, 45.0);
        assert_eq!(v.ghost.x_rot, 0.0);
    }

    #[test]
    fn visuals_ghost_fade_clamps_around_the_camera() {
        // A ghost under the camera fades out completely
        assert_eq!(tb_ghost_fade([0.0, 66.0, 0.0], [0.0, 65.8, 0.0]), 0.0);
        // At 0.8 blocks it starts to appear, at 2.0 it is fully visible
        assert_eq!(tb_ghost_fade([0.8, 65.8, 0.0], [0.0, 65.8, 0.0]), 0.0);
        assert_eq!(tb_ghost_fade([3.0, 70.0, 0.0], [0.0, 65.8, 0.0]), 1.0);
    }

    #[test]
    fn visuals_looking_at_rotates_the_landing_ghost_toward_the_hit() {
        // Facing +z is yaw 0
        assert!((tb_looking_at([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]).0).abs() < 1e-4);
        // Facing +x is yaw -90
        assert!((tb_looking_at([1.0, 0.0, 0.0], [0.0, 0.0, 0.0]).0 + 90.0).abs() < 1e-4);
        // Straight up is pitch -90
        assert!((tb_looking_at([0.0, 1.0, 0.0], [0.0, 0.0, 0.0]).1 + 90.0).abs() < 1e-4);
        // Level on the ground keeps pitch 0
        assert!((tb_looking_at([0.0, 0.0, 2.0], [0.0, 0.0, 0.0]).1).abs() < 1e-4);
    }
}

// ----- ModuleVelocity.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Upstream alias: "AntiKnockBack". The 14 `VelocityMode` choices stay a
// dispatch seam until the `velocity/mode/` files port; the module itself is
// the packet dispatcher. Upstream `isLocalPlayerVelocity()`: a
// `ClientboundSetEntityMotionPacket` aimed at the local player, or a
// `ClientboundExplodePacket` with player knockback.

/// The upstream `Mode` choice tags, verbatim.
pub const VELOCITY_MODE_TAGS: [&str; 14] = [
    "Modify",
    "Reversal",
    "Strafe",
    "JumpReset",
    "Lag",
    "Reduce",
    "Hypixel",
    "Dexland",
    "Hylex",
    "BlocksMC",
    "Grim-2.3.71",
    "Grim-2.3.44-MC1.17",
    "AAC-4.4.2",
    "Intave-2026-10-02",
];

/// Upstream `Velocity` world seams for one `PacketEvent`.
pub struct VelocityHooks<'a> {
    /// Upstream `event.original`.
    pub original: bool,
    /// Upstream `packet.isLocalPlayerVelocity()`.
    pub local_player_velocity: bool,
    /// Upstream `packet is ClientboundPlayerPositionPacket`.
    pub player_position_packet: bool,
    /// Upstream the selected mode's run on the re-dispatched packet event;
    /// returns whether that event stayed uncancelled.
    pub mode_run: Box<dyn FnMut() -> bool + 'a>,
}

/// Upstream the module's mutable state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityState {
    /// Upstream `internal var pause: Int`, decremented once per game tick.
    pub pause: i64,
}

/// What `ModuleVelocity.packetHandler` did to one incoming packet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityPacketOutcome {
    /// The packet flows on untouched.
    PassThrough,
    /// Upstream the original velocity packet was cancelled, optionally
    /// delayed, then re-dispatched: `handled` says whether the packet was
    /// handled or dropped, `waited_ms` the wall-clock wait before it.
    Cancelled { waited_ms: u64, handled: bool },
    /// Upstream `pause = PauseOnFlag`.
    Paused,
}

pub fn velocity() -> Module {
    Module::new(
        "Velocity",
        COMBAT,
        vec![
            // choices("Mode", VelocityModify, arrayOf(...)) with tagBy; the
            // default is VelocityModify ("Modify")
            choice("Mode", "Modify", &VELOCITY_MODE_TAGS),
            int_range("Delay", 0, 0, 0, 40), // "ticks" suffix
            int("PauseOnFlag", 0, 0, 20),    // "ticks" suffix
        ],
    )
}

/// Upstream `Delay.random()` on the `Delay` `IntRange` in `0..40`.
fn velocity_delay_random(module: &Module, roll: f64) -> i64 {
    let (from, to) = module
        .setting("Delay")
        .map(|value| &value.kind)
        .and_then(|kind| kind.int_range())
        .unwrap_or((0, 0));
    from + (roll * ((to - from + 1) as f64)).floor() as i64
}

/// Upstream `ModuleVelocity.pauseHandler` on `GameTickEvent`.
pub fn velocity_pause_handler(state: &mut VelocityState) {
    if state.pause > 0 {
        state.pause -= 1;
    }
}

/// Upstream `ModuleVelocity.packetHandler`, a `sequenceHandler` on
/// `PacketEvent` with priority 1. `delay_roll` is the `Delay.random()` roll.
pub fn velocity_packet_handler(
    module: &Module,
    state: &mut VelocityState,
    hooks: &mut VelocityHooks,
    delay_roll: f64,
) -> VelocityPacketOutcome {
    if !hooks.original || state.pause > 0 {
        return VelocityPacketOutcome::PassThrough;
    }

    if hooks.local_player_velocity {
        // When delay is above 0, we will delay the velocity update
        let (_, to) = module
            .setting("Delay")
            .map(|value| &value.kind)
            .and_then(|kind| kind.int_range())
            .unwrap_or((0, 0));
        if to > 0 {
            // cancelEvent()
            let ticks = velocity_delay_random(module, delay_roll);
            let waited_ms = if ticks > 0 {
                // tickUntil { System.currentTimeMillis() >= timeToWait }
                (ticks as u64) * 50
            } else {
                0
            };

            // A fresh, non-original PacketEvent re-dispatches the packet
            let handled = (hooks.mode_run)();

            return VelocityPacketOutcome::Cancelled { waited_ms, handled };
        }
    } else if hooks.player_position_packet {
        state.pause = tb_setting_i64(module, "PauseOnFlag", 0);
        return VelocityPacketOutcome::Paused;
    }

    VelocityPacketOutcome::PassThrough
}

#[cfg(test)]
mod velocity_tests {
    use super::*;

    fn vel_hooks<'a>(
        original: bool,
        local_velocity: bool,
        position_packet: bool,
        handled: &'a mut bool,
    ) -> VelocityHooks<'a> {
        VelocityHooks {
            original,
            local_player_velocity: local_velocity,
            player_position_packet: position_packet,
            mode_run: Box::new(move || *handled),
        }
    }

    #[test]
    fn velocity_passes_packets_through_when_not_original_or_paused() {
        let module = velocity();
        let mut state = VelocityState::default();
        let mut handled = false;

        // A non-original event never runs
        {
            let mut hooks = vel_hooks(false, true, false, &mut handled);
            assert_eq!(
                velocity_packet_handler(&module, &mut state, &mut hooks, 0.5),
                VelocityPacketOutcome::PassThrough
            );
        }

        // A paused module passes everything through
        state.pause = 3;
        {
            let mut hooks = vel_hooks(true, true, true, &mut handled);
            assert_eq!(
                velocity_packet_handler(&module, &mut state, &mut hooks, 0.5),
                VelocityPacketOutcome::PassThrough
            );
        }
        assert_eq!(state.pause, 3);
        assert!(!handled);
    }

    #[test]
    fn velocity_with_a_zero_delay_keeps_the_velocity_packet() {
        let module = velocity();
        let mut state = VelocityState::default();
        let mut handled = false;

        // Default Delay 0..0: delay.last is 0, so the packet is untouched
        {
            let mut hooks = vel_hooks(true, true, false, &mut handled);
            assert_eq!(
                velocity_packet_handler(&module, &mut state, &mut hooks, 0.5),
                VelocityPacketOutcome::PassThrough
            );
        }
        assert!(!handled);
    }

    #[test]
    fn velocity_cancels_the_packet_delay_and_redispatches() {
        let mut module = velocity();
        // IntRange(from, to, lo, hi): Delay 2..10 inside 0..40
        module.setting_mut("Delay").unwrap().kind = ValueKind::IntRange(2, 10, 0, 40);
        let mut state = VelocityState::default();

        // delay.random() with roll 0.5 over 2..10: 2 + floor(0.5 * 9) = 6
        {
            let mut handled = true;
            let mut hooks = vel_hooks(true, true, false, &mut handled);
            assert_eq!(
                velocity_packet_handler(&module, &mut state, &mut hooks, 0.5),
                VelocityPacketOutcome::Cancelled { waited_ms: 6 * 50, handled: true }
            );
        }

        // The mode cancelling the re-dispatched event drops the packet
        {
            let mut handled = false;
            let mut hooks = vel_hooks(true, true, false, &mut handled);
            assert_eq!(
                velocity_packet_handler(&module, &mut state, &mut hooks, 0.5),
                VelocityPacketOutcome::Cancelled { waited_ms: 6 * 50, handled: false }
            );
        }

        // delay.random() = 0 skips the wait but still re-dispatches
        module.setting_mut("Delay").unwrap().kind = ValueKind::IntRange(0, 2, 0, 40);
        {
            let mut handled = true;
            let mut hooks = vel_hooks(true, true, false, &mut handled);
            assert_eq!(
                velocity_packet_handler(&module, &mut state, &mut hooks, 0.0),
                VelocityPacketOutcome::Cancelled { waited_ms: 0, handled: true }
            );
        }
    }

    #[test]
    fn velocity_pauses_on_a_player_position_packet() {
        let mut module = velocity();
        let mut state = VelocityState::default();
        let mut handled = false;

        {
            let mut hooks = vel_hooks(true, false, true, &mut handled);
            assert_eq!(
                velocity_packet_handler(&module, &mut state, &mut hooks, 0.5),
                VelocityPacketOutcome::Paused
            );
        }
        assert_eq!(state.pause, 0);

        // PauseOnFlag 12: the position packet arms the pause
        module.setting_mut("PauseOnFlag").unwrap().kind = ValueKind::Int(12, 0, 20);
        {
            let mut hooks = vel_hooks(true, false, true, &mut handled);
            assert_eq!(
                velocity_packet_handler(&module, &mut state, &mut hooks, 0.5),
                VelocityPacketOutcome::Paused
            );
        }
        assert_eq!(state.pause, 12);

        // Each game tick decrements the pause
        velocity_pause_handler(&mut state);
        velocity_pause_handler(&mut state);
        assert_eq!(state.pause, 10);

        // Once the pause hits zero, packets run again
        state.pause = 1;
        velocity_pause_handler(&mut state);
        {
            let mut hooks = vel_hooks(true, true, false, &mut handled);
            assert_eq!(
                velocity_packet_handler(&module, &mut state, &mut hooks, 0.5),
                VelocityPacketOutcome::PassThrough
            );
        }
    }

    #[test]
    fn velocity_mode_tags_match_the_upstream_choices() {
        let module = velocity();
        let mode = module.setting("Mode").unwrap();
        assert_eq!(mode.kind.choice(), Some("Modify"));
        assert_eq!(
            VELOCITY_MODE_TAGS,
            [
                "Modify",
                "Reversal",
                "Strafe",
                "JumpReset",
                "Lag",
                "Reduce",
                "Hypixel",
                "Dexland",
                "Hylex",
                "BlocksMC",
                "Grim-2.3.71",
                "Grim-2.3.44-MC1.17",
                "AAC-4.4.2",
                "Intave-2026-10-02",
            ]
        );
    }
}

// ----- velocity/mode/VelocityMode.kt + VelocityModify.kt + VelocityStrafe.kt
// (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Upstream `VelocityMode.running` is `super.running && pause == 0`: every
// mode gate here takes the caller's `mode_running` input, which is that
// conjunction. The modes share one world seam.

/// Upstream world/player surface the velocity modes read.
pub trait VelocityWorld {
    /// `player.id`
    fn player_id(&self) -> i64;
    /// `player.onGround()`
    fn player_on_ground(&self) -> bool;
    /// `player.input.keyPresses.anyHorizontal`
    fn any_horizontal_input(&self) -> bool;
    /// `player.deltaMovement`
    fn delta_movement(&self) -> Vec3;
    /// `player.horizontalSpeed` (`deltaMovement.horizontalDistance()`)
    fn horizontal_speed(&self) -> f64;
    /// `player.input` `isMoving`
    fn input_is_moving(&self) -> bool;
    /// `player.getMovementDirectionOfInput(input)` yaw for `withStrafe`
    fn movement_yaw(&self) -> f32;
    /// `world.findEnemy(0f, range)`: an attackable enemy in range
    fn find_enemy(&self, max_range: f64) -> Option<Vec3>;
    /// `isLookingAtEntity(target, range, rotation)`: Some when it hits
    fn is_looking_at_entity(&self, target: Vec3, range: f64, rotation: (f32, f32)) -> Option<()>;
    /// `RotationManager.currentRotation`
    fn current_rotation(&self) -> Option<(f32, f32)>;
    /// `player.rotation`
    fn player_rotation(&self) -> (f32, f32);
}

/// Upstream `Vec3.withStrafe(speed, strength = 1.0, input, yaw)`.
pub fn with_strafe(vec: Vec3, speed: f64, strength: f64, input_moving: bool, yaw_deg: f32) -> Vec3 {
    if !input_moving {
        return Vec3 { x: 0.0, y: vec.y, z: 0.0 };
    }

    let one_minus_strength = 1.0 - strength;
    let prev_x = vec.x * one_minus_strength;
    let prev_z = vec.z * one_minus_strength;
    let used_speed = speed * strength;

    let angle = (yaw_deg as f64).to_radians();
    let new_x = prev_x - angle.sin() * used_speed;
    let new_z = prev_z + angle.cos() * used_speed;

    Vec3 { x: new_x, y: vec.y, z: new_z }
}

// ----- VelocityModify.kt -----
//
// Basic velocity which should bypass most servers with common anti-cheats
// like NCP.

/// The upstream `VelocityTriggerFilter` tags, verbatim.
pub const VELOCITY_FILTER_TAGS: [&str; 3] = ["Always", "OnGround", "InAir"];

pub fn velocity_modify() -> Module {
    Module::new(
        "Modify",
        COMBAT,
        vec![
            float("Horizontal", 0.0, -1.0, 1.0),
            float("Vertical", 0.0, -1.0, 1.0),
            float("MotionHorizontal", 0.0, 0.0, 1.0),
            float("MotionVertical", 0.0, 0.0, 1.0),
            // percentageChance("Chance", 100), a "%" int in 0..100
            int("Chance", 100, 0, 100),
            choice("Filter", "Always", &["Always", "OnGround", "InAir"]),
            boolean("OnlyMove", false),
            int("TransactionBuffer", 0, 0, 3),
            boolean("ConsiderExplosion", true),
        ],
    )
}

/// Upstream `VelocityModify.transactionBuffer`, reset on disable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityModifyState {
    pub transaction_buffer: i64,
}

/// Upstream `VelocityModify.disable()`.
pub fn velocity_modify_disable(state: &mut VelocityModifyState) {
    state.transaction_buffer = 0;
}

/// The packets the modify handler distinguishes, flattened to their
/// distinguishing fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityModifyPacket {
    /// `ClientboundSetEntityMotionPacket` with `id == player.id`
    Motion { movement: (f64, f64, f64) },
    /// `ClientboundExplodePacket` with `playerKnockback.isPresent`
    Explode { knockback: (f64, f64, f64) },
    /// `ServerboundPongPacket`
    Pong,
}

/// What the modify handler did to the packet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityModifyOutcome {
    /// The packet flows on untouched.
    PassThrough,
    /// Upstream `event.cancelEvent()`.
    Cancelled,
    /// The packet's motion was rewritten in place: (x, y, z).
    Modified((f64, f64, f64)),
}

/// Upstream `PercentageChance.getAsBoolean`: 0% is always false, 100% always
/// true, otherwise a uniform roll in 0..100 is under the percentage.
fn velocity_chance_as_boolean(module: &Module, roll: f64) -> bool {
    // Int percentage (VelocityModify) or Float percentage (VelocityJumpReset)
    let percentage = module
        .setting("Chance")
        .map(|value| &value.kind)
        .and_then(|kind| match kind {
            ValueKind::Int(v, _, _) => Some(*v as f64),
            ValueKind::Float(v, _, _) => Some(*v),
            _ => None,
        })
        .unwrap_or(100.0);
    if percentage <= 0.0 {
        false
    } else if percentage >= 100.0 {
        true
    } else {
        roll * 100.0 < percentage
    }
}

/// Upstream `VelocityTriggerFilter.condition`: Always, OnGround, InAir.
fn velocity_modify_filter_passes(module: &Module, world: &dyn VelocityWorld) -> bool {
    match module
        .setting("Filter")
        .and_then(|value| value.kind.choice())
        .unwrap_or("Always")
    {
        "OnGround" => world.player_on_ground(),
        "InAir" => !world.player_on_ground(),
        _ => true, // Always
    }
}

/// Upstream the shared gate block: chance, filter, `OnlyMove` + input.
fn velocity_modify_gates_pass(
    module: &Module,
    world: &dyn VelocityWorld,
    chance_roll: f64,
) -> bool {
    if !velocity_chance_as_boolean(module, chance_roll) {
        return false;
    }
    if !velocity_modify_filter_passes(module, world) {
        return false;
    }
    if tb_setting_bool(module, "OnlyMove", false) && !world.any_horizontal_input() {
        return false;
    }
    true
}

/// Upstream `VelocityModify.packetHandler`, a `handler` on `PacketEvent`.
/// `wait_until_ground` is `NoFallBlink.waitUntilGround`.
pub fn velocity_modify_packet_handler(
    module: &Module,
    state: &mut VelocityModifyState,
    world: &dyn VelocityWorld,
    chance_roll: f64,
    packet: VelocityModifyPacket,
    wait_until_ground: &mut bool,
) -> VelocityModifyOutcome {
    match packet {
        VelocityModifyPacket::Motion { movement: (x, y, z) } => {
            // Check if this is a regular velocity update
            if !velocity_modify_gates_pass(module, world, chance_roll) {
                return VelocityModifyOutcome::PassThrough;
            }

            let horizontal = tb_setting_f64(module, "Horizontal", 0.0);
            let vertical = tb_setting_f64(module, "Vertical", 0.0);

            // It should just block the packet
            if horizontal == 0.0 && vertical == 0.0 {
                *wait_until_ground = true;
                return VelocityModifyOutcome::Cancelled;
            }

            let current_velocity = world.delta_movement();
            let (mut x, mut y, mut z) = (x, y, z);

            // Modify packet according to the specified values
            if horizontal != 0.0 {
                x *= horizontal;
                z *= horizontal;
            } else {
                // set the horizontal velocity to the player velocity to
                // prevent horizontal slowdown
                x = current_velocity.x * tb_setting_f64(module, "MotionHorizontal", 0.0);
                z = current_velocity.z * tb_setting_f64(module, "MotionHorizontal", 0.0);
            }

            if vertical != 0.0 {
                y *= vertical;
            } else {
                // set the vertical velocity to the player velocity to
                // prevent vertical slowdown
                y = current_velocity.y * tb_setting_f64(module, "MotionVertical", 0.0);
            }

            *wait_until_ground = true;
            state.transaction_buffer += tb_setting_i64(module, "TransactionBuffer", 0);
            VelocityModifyOutcome::Modified((x, y, z))
        }

        VelocityModifyPacket::Explode { knockback: (x, y, z) } => {
            // Check if velocity is affected by explosion
            if !tb_setting_bool(module, "ConsiderExplosion", true) {
                return VelocityModifyOutcome::PassThrough;
            }
            if !velocity_modify_gates_pass(module, world, chance_roll) {
                return VelocityModifyOutcome::PassThrough;
            }

            // note: explosion packets are being used by hypixel to trick
            // poorly made cheats.

            // Modify packet according to the specified values
            let horizontal = tb_setting_f64(module, "Horizontal", 0.0);
            let vertical = tb_setting_f64(module, "Vertical", 0.0);
            let (mut x, mut y, mut z) = (x, y, z);
            x *= horizontal;
            y *= vertical;
            z *= horizontal;

            *wait_until_ground = true;
            state.transaction_buffer += tb_setting_i64(module, "TransactionBuffer", 0);
            VelocityModifyOutcome::Modified((x, y, z))
        }

        VelocityModifyPacket::Pong => {
            if state.transaction_buffer > 0 {
                state.transaction_buffer -= 1;
                VelocityModifyOutcome::Cancelled
            } else {
                VelocityModifyOutcome::PassThrough
            }
        }
    }
}

// ----- VelocityStrafe.kt -----

pub fn velocity_strafe() -> Module {
    Module::new(
        "Strafe",
        COMBAT,
        vec![
            int("Delay", 2, 0, 10), // "ticks" suffix
            float("Strength", 1.0, 0.1, 2.0),
            // OnlyFacing: ToggleableValueGroup with a Range child
            boolean("OnlyFacing", false),
            float("Range", 3.5, 0.1, 6.0),
            boolean("UntilGround", false),
        ],
    )
}

/// Upstream `VelocityStrafe.applyStrafe` + `shouldStrafe`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityStrafeState {
    pub apply_strafe: bool,
    pub should_strafe: bool,
}

/// Upstream `VelocityStrafe.tickHandler`, a `handler` on `GameTickEvent`.
pub fn velocity_strafe_tick_handler(
    module: &Module,
    state: &mut VelocityStrafeState,
    world: &dyn VelocityWorld,
) {
    if !tb_setting_bool(module, "OnlyFacing", false) {
        return;
    }
    let range = tb_setting_f64(module, "Range", 3.5);
    let Some(target) = world.find_enemy(range) else {
        return;
    };

    let rotation = world
        .current_rotation()
        .unwrap_or_else(|| world.player_rotation());
    state.should_strafe =
        world.is_looking_at_entity(target, range, rotation).is_some();
}

/// What the strafe packet handler did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityStrafePacketOutcome {
    /// The packet is untouched.
    Untouched,
    /// Upstream `waitTicks(delay)` then `player.deltaMovement =
    /// player.deltaMovement.withStrafe(speed = player.horizontalSpeed *
    /// strength)`; `until_ground_armed` says `UntilGround` set
    /// `applyStrafe`.
    Applied { new_movement: Vec3, until_ground_armed: bool },
}

/// Upstream `VelocityStrafe.packetHandler`, a `sequenceHandler` on
/// `PacketEvent`.
pub fn velocity_strafe_packet_handler(
    module: &Module,
    state: &mut VelocityStrafeState,
    world: &dyn VelocityWorld,
    local_player_velocity: bool,
) -> VelocityStrafePacketOutcome {
    // Check if this is a regular velocity update
    if !local_player_velocity {
        return VelocityStrafePacketOutcome::Untouched;
    }

    let only_facing = tb_setting_bool(module, "OnlyFacing", false);
    if only_facing && !state.should_strafe {
        return VelocityStrafePacketOutcome::Untouched;
    }

    // A few anti-cheats can be easily tricked by applying the velocity a few
    // ticks after being damaged: waitTicks(delay)
    let _delay = tb_setting_i64(module, "Delay", 2);

    // Apply strafe
    let strength = tb_setting_f64(module, "Strength", 1.0);
    let new_movement = with_strafe(
        world.delta_movement(),
        world.horizontal_speed() * strength,
        1.0,
        world.input_is_moving(),
        world.movement_yaw(),
    );

    let mut until_ground_armed = false;
    if tb_setting_bool(module, "UntilGround", false) {
        state.apply_strafe = true;
        until_ground_armed = true;
    }

    VelocityStrafePacketOutcome::Applied { new_movement, until_ground_armed }
}

/// What the strafe move handler did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityStrafeMoveOutcome {
    /// The movement is untouched.
    Untouched,
    /// Upstream `event.movement = event.movement.withStrafe(speed =
    /// player.horizontalSpeed * strength)`; carries the new movement.
    Applied(Vec3),
}

/// Upstream `VelocityStrafe.moveHandler`, a `handler` on `PlayerMoveEvent`.
pub fn velocity_strafe_move_handler(
    module: &Module,
    state: &mut VelocityStrafeState,
    world: &dyn VelocityWorld,
    movement: Vec3,
) -> VelocityStrafeMoveOutcome {
    if world.player_on_ground() {
        state.apply_strafe = false;
        return VelocityStrafeMoveOutcome::Untouched;
    }

    if state.apply_strafe {
        let strength = tb_setting_f64(module, "Strength", 1.0);
        let new_movement = with_strafe(
            movement,
            world.horizontal_speed() * strength,
            1.0,
            world.input_is_moving(),
            world.movement_yaw(),
        );
        return VelocityStrafeMoveOutcome::Applied(new_movement);
    }

    VelocityStrafeMoveOutcome::Untouched
}

#[cfg(test)]
mod velocity_mode_tests {
    use super::*;

    struct MockVelWorld {
        on_ground: bool,
        moving_input: bool,
        delta: Vec3,
        yaw: f32,
        enemy: Option<Vec3>,
        looking: bool,
    }

    impl Default for MockVelWorld {
        fn default() -> Self {
            MockVelWorld {
                on_ground: true,
                moving_input: true,
                delta: Vec3 { x: 0.4, y: -0.078, z: 0.3 },
                yaw: 90.0,
                enemy: None,
                looking: false,
            }
        }
    }

    impl VelocityWorld for MockVelWorld {
        fn player_id(&self) -> i64 {
            42
        }
        fn player_on_ground(&self) -> bool {
            self.on_ground
        }
        fn any_horizontal_input(&self) -> bool {
            self.moving_input
        }
        fn delta_movement(&self) -> Vec3 {
            self.delta
        }
        fn horizontal_speed(&self) -> f64 {
            (self.delta.x * self.delta.x + self.delta.z * self.delta.z).sqrt()
        }
        fn input_is_moving(&self) -> bool {
            self.moving_input
        }
        fn movement_yaw(&self) -> f32 {
            self.yaw
        }
        fn find_enemy(&self, _max_range: f64) -> Option<Vec3> {
            self.enemy
        }
        fn is_looking_at_entity(
            &self,
            _target: Vec3,
            _range: f64,
            _rotation: (f32, f32),
        ) -> Option<()> {
            if self.looking { Some(()) } else { None }
        }
        fn current_rotation(&self) -> Option<(f32, f32)> {
            None
        }
        fn player_rotation(&self) -> (f32, f32) {
            (10.0, 0.0)
        }
    }

    #[test]
    fn velocity_modify_scales_the_motion_packet() {
        let mut module = velocity_modify();
        module.setting_mut("Horizontal").unwrap().kind = ValueKind::Float(0.5, -1.0, 1.0);
        module.setting_mut("Vertical").unwrap().kind = ValueKind::Float(0.5, -1.0, 1.0);
        let world = MockVelWorld::default();
        let mut state = VelocityModifyState::default();
        let mut wait_until_ground = false;

        let outcome = velocity_modify_packet_handler(
            &module,
            &mut state,
            &world,
            0.5,
            VelocityModifyPacket::Motion { movement: (0.8, 0.4, 0.6) },
            &mut wait_until_ground,
        );
        // x and z scale by horizontal, y by vertical
        assert_eq!(outcome, VelocityModifyOutcome::Modified((0.4, 0.2, 0.3)));
        assert!(wait_until_ground);
        assert_eq!(state.transaction_buffer, 0);
    }

    #[test]
    fn velocity_modify_motion_fallback_uses_player_velocity() {
        let mut module = velocity_modify();
        // Vertical scales, Horizontal falls back to MotionHorizontal
        module.setting_mut("Vertical").unwrap().kind = ValueKind::Float(0.5, -1.0, 1.0);
        module.setting_mut("MotionHorizontal").unwrap().kind = ValueKind::Float(0.25, 0.0, 1.0);
        let world = MockVelWorld::default();
        let mut state = VelocityModifyState::default();
        let mut wait_until_ground = false;

        let outcome = velocity_modify_packet_handler(
            &module,
            &mut state,
            &world,
            0.5,
            VelocityModifyPacket::Motion { movement: (0.8, 0.4, 0.6) },
            &mut wait_until_ground,
        );
        // x = delta.x * motionHorizontal, y = movement.y * vertical
        assert_eq!(
            outcome,
            VelocityModifyOutcome::Modified((0.4 * 0.25, 0.2, 0.3 * 0.25))
        );
        assert!(wait_until_ground);
    }

    #[test]
    fn velocity_modify_blocks_the_packet_when_horizontal_and_vertical_are_zero() {
        let module = velocity_modify();
        let world = MockVelWorld::default();
        let mut state = VelocityModifyState::default();
        let mut wait_until_ground = false;

        let outcome = velocity_modify_packet_handler(
            &module,
            &mut state,
            &world,
            0.5,
            VelocityModifyPacket::Motion { movement: (0.8, 0.4, 0.6) },
            &mut wait_until_ground,
        );
        assert_eq!(outcome, VelocityModifyOutcome::Cancelled);
        assert!(wait_until_ground);
    }

    #[test]
    fn velocity_modify_gates_drop_the_packet() {
        let world = MockVelWorld::default();
        let mut state = VelocityModifyState::default();
        let mut wait_until_ground = false;

        // Chance 0% never fires
        let mut module = velocity_modify();
        module.setting_mut("Chance").unwrap().kind = ValueKind::Int(0, 0, 100);
        let outcome = velocity_modify_packet_handler(
            &module,
            &mut state,
            &world,
            0.5,
            VelocityModifyPacket::Motion { movement: (0.8, 0.4, 0.6) },
            &mut wait_until_ground,
        );
        assert_eq!(outcome, VelocityModifyOutcome::PassThrough);
        assert!(!wait_until_ground);

        // OnGround filter while in air drops it
        let mut module = velocity_modify();
        module.setting_mut("Filter").unwrap().kind =
            ValueKind::Choice("OnGround".into(), VELOCITY_FILTER_TAGS.iter().map(|s| s.to_string()).collect());
        let mut world = MockVelWorld::default();
        world.on_ground = false;
        let outcome = velocity_modify_packet_handler(
            &module,
            &mut state,
            &world,
            0.5,
            VelocityModifyPacket::Motion { movement: (0.8, 0.4, 0.6) },
            &mut wait_until_ground,
        );
        assert_eq!(outcome, VelocityModifyOutcome::PassThrough);

        // OnlyMove without horizontal input drops it
        let mut module = velocity_modify();
        module.setting_mut("OnlyMove").unwrap().kind = ValueKind::Bool(true);
        let mut world = MockVelWorld::default();
        world.moving_input = false;
        let outcome = velocity_modify_packet_handler(
            &module,
            &mut state,
            &world,
            0.5,
            VelocityModifyPacket::Motion { movement: (0.8, 0.4, 0.6) },
            &mut wait_until_ground,
        );
        assert_eq!(outcome, VelocityModifyOutcome::PassThrough);
    }

    #[test]
    fn velocity_modify_scales_the_explosion_knockback() {
        let mut module = velocity_modify();
        module.setting_mut("Horizontal").unwrap().kind = ValueKind::Float(0.5, -1.0, 1.0);
        module.setting_mut("Vertical").unwrap().kind = ValueKind::Float(0.5, -1.0, 1.0);
        module.setting_mut("TransactionBuffer").unwrap().kind = ValueKind::Int(2, 0, 3);
        let world = MockVelWorld::default();
        let mut state = VelocityModifyState::default();
        let mut wait_until_ground = false;

        let outcome = velocity_modify_packet_handler(
            &module,
            &mut state,
            &world,
            0.5,
            VelocityModifyPacket::Explode { knockback: (0.8, 0.4, 0.6) },
            &mut wait_until_ground,
        );
        // knockback.x *= horizontal, knockback.y *= vertical, knockback.z *= horizontal
        assert_eq!(outcome, VelocityModifyOutcome::Modified((0.4, 0.2, 0.3)));
        assert!(wait_until_ground);
        assert_eq!(state.transaction_buffer, 2);

        // ConsiderExplosion false leaves the packet untouched
        module.setting_mut("ConsiderExplosion").unwrap().kind = ValueKind::Bool(false);
        let outcome = velocity_modify_packet_handler(
            &module,
            &mut state,
            &world,
            0.5,
            VelocityModifyPacket::Explode { knockback: (0.8, 0.4, 0.6) },
            &mut wait_until_ground,
        );
        assert_eq!(outcome, VelocityModifyOutcome::PassThrough);
        assert_eq!(state.transaction_buffer, 2);
    }

    #[test]
    fn velocity_modify_cancels_pong_packets_while_the_buffer_is_up() {
        let mut module = velocity_modify();
        // Horizontal 0.5 so the motion packet scales and fills the buffer
        // (with both modifiers 0 the packet is blocked outright instead)
        module.setting_mut("Horizontal").unwrap().kind = ValueKind::Float(0.5, -1.0, 1.0);
        module.setting_mut("TransactionBuffer").unwrap().kind = ValueKind::Int(2, 0, 3);
        let world = MockVelWorld::default();
        let mut state = VelocityModifyState::default();
        let mut wait_until_ground = false;

        // Fill the buffer through a motion packet
        velocity_modify_packet_handler(
            &module,
            &mut state,
            &world,
            0.5,
            VelocityModifyPacket::Motion { movement: (0.8, 0.4, 0.6) },
            &mut wait_until_ground,
        );
        assert_eq!(state.transaction_buffer, 2);

        // Each pong cancels and drains one
        assert_eq!(
            velocity_modify_packet_handler(
                &module,
                &mut state,
                &world,
                0.5,
                VelocityModifyPacket::Pong,
                &mut wait_until_ground
            ),
            VelocityModifyOutcome::Cancelled
        );
        assert_eq!(
            velocity_modify_packet_handler(
                &module,
                &mut state,
                &world,
                0.5,
                VelocityModifyPacket::Pong,
                &mut wait_until_ground
            ),
            VelocityModifyOutcome::Cancelled
        );
        assert_eq!(state.transaction_buffer, 0);

        // Once empty, pongs flow on
        assert_eq!(
            velocity_modify_packet_handler(
                &module,
                &mut state,
                &world,
                0.5,
                VelocityModifyPacket::Pong,
                &mut wait_until_ground
            ),
            VelocityModifyOutcome::PassThrough
        );

        // disable() resets the buffer
        state.transaction_buffer = 3;
        velocity_modify_disable(&mut state);
        assert_eq!(state.transaction_buffer, 0);
    }

    #[test]
    fn with_strafe_blends_the_previous_velocity_toward_the_yaw() {
        let vec = Vec3 { x: 1.0, y: 0.5, z: 2.0 };

        // Not moving zeroes the horizontal components
        assert_eq!(
            with_strafe(vec, 4.0, 1.0, false, 90.0),
            Vec3 { x: 0.0, y: 0.5, z: 0.0 }
        );

        // yaw 90°: sin = 1, cos = 0, so x - speed, z + 0
        let out = with_strafe(vec, 3.0, 0.5, true, 90.0);
        assert_eq!(out.y, 0.5);
        assert!((out.x - (1.0 * 0.5 - 1.5)).abs() < 1e-12);
        assert!((out.z - 2.0 * 0.5).abs() < 1e-12);
    }

    #[test]
    fn velocity_strafe_packet_handler_applies_the_strafe() {
        let mut module = velocity_strafe();
        let world = MockVelWorld::default();
        let mut state = VelocityStrafeState::default();

        // Not a local velocity packet: untouched
        assert_eq!(
            velocity_strafe_packet_handler(&module, &mut state, &world, false),
            VelocityStrafePacketOutcome::Untouched
        );

        // Local velocity: strafe applied, UntilGround arms applyStrafe
        let outcome = velocity_strafe_packet_handler(&module, &mut state, &world, true);
        assert!(matches!(outcome, VelocityStrafePacketOutcome::Applied { until_ground_armed: false, .. }));
        assert!(!state.apply_strafe);

        module.setting_mut("UntilGround").unwrap().kind = ValueKind::Bool(true);
        let outcome = velocity_strafe_packet_handler(&module, &mut state, &world, true);
        assert!(matches!(outcome, VelocityStrafePacketOutcome::Applied { until_ground_armed: true, .. }));
        assert!(state.apply_strafe);
    }

    #[test]
    fn velocity_strafe_packet_handler_respects_only_facing() {
        let mut module = velocity_strafe();
        let mut world = MockVelWorld::default();
        world.enemy = Some(Vec3::ZERO);
        world.looking = false;
        let mut state = VelocityStrafeState::default();

        // OnlyFacing with no enemy seen: the tick handler never sets
        // shouldStrafe, so the packet handler passes
        module.setting_mut("OnlyFacing").unwrap().kind = ValueKind::Bool(true);
        velocity_strafe_tick_handler(&module, &mut state, &world);
        assert!(!state.should_strafe);
        assert_eq!(
            velocity_strafe_packet_handler(&module, &mut state, &world, true),
            VelocityStrafePacketOutcome::Untouched
        );

        // Looking at the enemy sets shouldStrafe and lets it through
        world.looking = true;
        velocity_strafe_tick_handler(&module, &mut state, &world);
        assert!(state.should_strafe);
        assert!(matches!(
            velocity_strafe_packet_handler(&module, &mut state, &world, true),
            VelocityStrafePacketOutcome::Applied { .. }
        ));

        // OnlyFacing off ignores shouldStrafe entirely
        module.setting_mut("OnlyFacing").unwrap().kind = ValueKind::Bool(false);
        state.should_strafe = false;
        assert!(matches!(
            velocity_strafe_packet_handler(&module, &mut state, &world, true),
            VelocityStrafePacketOutcome::Applied { .. }
        ));
    }

    #[test]
    fn velocity_strafe_move_handler_applies_until_the_player_lands() {
        let module = velocity_strafe();
        let mut world = MockVelWorld::default();
        world.on_ground = false;
        let mut state = VelocityStrafeState::default();
        state.apply_strafe = true;

        // In air with applyStrafe: the movement is rewritten
        let outcome =
            velocity_strafe_move_handler(&module, &mut state, &world, Vec3 { x: 1.0, y: 0.0, z: 0.0 });
        assert!(matches!(outcome, VelocityStrafeMoveOutcome::Applied(_)));

        // On ground: cleared and untouched
        world.on_ground = true;
        let outcome =
            velocity_strafe_move_handler(&module, &mut state, &world, Vec3 { x: 1.0, y: 0.0, z: 0.0 });
        assert_eq!(outcome, VelocityStrafeMoveOutcome::Untouched);
        assert!(!state.apply_strafe);

        // Back in air without applyStrafe: untouched
        world.on_ground = false;
        let outcome =
            velocity_strafe_move_handler(&module, &mut state, &world, Vec3 { x: 1.0, y: 0.0, z: 0.0 });
        assert_eq!(outcome, VelocityStrafeMoveOutcome::Untouched);
    }
}

// ----- velocity/mode/VelocityReversal.kt + VelocityJumpReset.kt
// (GPL-3.0, (c) 2015-2026 CCBlueX) -----

/// Upstream `VelocityWorld` gains the hurt/sprint surface the two modes read.
pub trait VelocityWorldExtended: VelocityWorld {
    /// `player.hurtTime` (9 = the tick the hit landed)
    fn hurt_time(&self) -> i64;
    /// `player.isSprinting`
    fn is_sprinting(&self) -> bool;
    /// `LocalPlayerFallDamageTracker.isCurrentFallDamage`
    fn is_current_fall_damage(&self) -> bool;
    /// Upstream `player.isSprinting = false` (Reduce attack burst): a write
    /// seam, default no-op.
    fn set_sprinting(&mut self, sprinting: bool) {
        let _ = sprinting;
    }

    /// Upstream `player.deltaMovement = ...` (Reduce attack burst): a write
    /// seam, default no-op.
    fn set_delta_movement(&mut self, delta: Vec3) {
        let _ = delta;
    }

}

/// Upstream `Vec3.multiply(factorX, factorY = 1.0, factorZ)`.
fn vec3_multiply(vec: Vec3, factor_x: f64, factor_y: f64, factor_z: f64) -> Vec3 {
    Vec3 {
        x: vec.x * factor_x,
        y: vec.y * factor_y,
        z: vec.z * factor_z,
    }
}

/// Upstream `Vec3.lengthSqr()`.
fn vec3_length_sqr(vec: Vec3) -> f64 {
    vec.x * vec.x + vec.y * vec.y + vec.z * vec.z
}

// ----- VelocityReversal.kt -----
//
// A velocity mode that reverses your velocity after a set amount of ticks.
// Default values bypass Vulcan (3/9/25) ~ anticheat-test.com

pub fn velocity_reversal() -> Module {
    Module::new(
        "Reversal",
        COMBAT,
        vec![
            int("Delay", 2, 1, 5), // "ticks" suffix
            float("XModifier", 0.5, 0.0, 1.0),
            float("ZModifier", 0.5, 0.0, 1.0),
            boolean("OnlyMoving", false),
        ],
    )
}

/// Upstream `VelocityReversal.handlingVelocity` + `velocityTicks`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityReversalState {
    pub handling_velocity: bool,
    pub velocity_ticks: i64,
}

fn velocity_reversal_reset(state: &mut VelocityReversalState) {
    state.velocity_ticks = 0;
    state.handling_velocity = false;
}

/// Upstream `VelocityReversal.packetEventHandler`, a `handler` on
/// `PacketEvent`.
pub fn velocity_reversal_packet_handler(
    module: &Module,
    state: &mut VelocityReversalState,
    world: &dyn VelocityWorldExtended,
    local_player_velocity: bool,
) {
    if !local_player_velocity {
        return;
    }

    // `player.moving`
    if tb_setting_bool(module, "OnlyMoving", false) && !world.input_is_moving() {
        return;
    }

    velocity_reversal_reset(state);
    state.handling_velocity = true;
}

/// What the reversal tick handler did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityReversalTickOutcome {
    /// Nothing ran or the state only reset.
    Untouched,
    /// Upstream `player.deltaMovement = player.deltaMovement.multiply(
    /// factorX = -xModifier, factorZ = -zModifier)`; carries the new
    /// movement.
    Applied(Vec3),
}

/// Upstream `VelocityReversal.playerTickHandler`, a `handler` on
/// `PlayerTickEvent`.
pub fn velocity_reversal_tick_handler(
    module: &Module,
    state: &mut VelocityReversalState,
    world: &dyn VelocityWorldExtended,
) -> VelocityReversalTickOutcome {
    if !state.handling_velocity {
        return VelocityReversalTickOutcome::Untouched;
    }

    if vec3_length_sqr(world.delta_movement()) == 0.0 {
        velocity_reversal_reset(state);
        return VelocityReversalTickOutcome::Untouched;
    }

    state.velocity_ticks += 1;
    if state.velocity_ticks >= tb_setting_i64(module, "Delay", 2) {
        let new_movement = vec3_multiply(
            world.delta_movement(),
            -tb_setting_f64(module, "XModifier", 0.5),
            1.0,
            -tb_setting_f64(module, "ZModifier", 0.5),
        );
        velocity_reversal_reset(state);
        return VelocityReversalTickOutcome::Applied(new_movement);
    }

    VelocityReversalTickOutcome::Untouched
}

// ----- VelocityJumpReset.kt -----
//
// Jump Reset mode. A technique most players use to minimize the amount of
// knockback they get.

/// The upstream `JumpByReceivedHits` tags, verbatim.
pub const VELOCITY_JUMP_HITS_TAGS: [&str; 1] = ["HitsUntilJump"];
/// The upstream `JumpByDelay` tags, verbatim.
pub const VELOCITY_JUMP_DELAY_TAGS: [&str; 1] = ["UntilJump"];

pub fn velocity_jump_reset() -> Module {
    Module::new(
        "JumpReset",
        COMBAT,
        vec![
            // percentageChance("Chance", 100f), a "%" float in 0..100
            float("Chance", 100.0, 0.0, 100.0),
            // JumpByReceivedHits: ToggleableValueGroup("JumpByReceivedHits", false)
            boolean("JumpByReceivedHits", false),
            // hitsUntilJump = intRange("HitsUntilJump", 2..2, 0..10)
            int_range("HitsUntilJump", 2, 2, 0, 10),
            // JumpByDelay: ToggleableValueGroup("JumpByDelay", true)
            boolean("JumpByDelay", true),
            // ticksUntilJump = intRange("UntilJump", 2..2, 0..20, "ticks")
            int_range("UntilJump", 2, 2, 0, 20),
        ],
    )
}

/// Upstream `VelocityJumpReset.limitUntilJump`, `hitsUntilJump` and
/// `ticksUntilJump` (the last two re-rolled from their ranges each jump).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VelocityJumpResetState {
    pub limit_until_jump: i64,
    pub hits_until_jump: i64,
    pub ticks_until_jump: i64,
}

impl Default for VelocityJumpResetState {
    fn default() -> Self {
        VelocityJumpResetState {
            limit_until_jump: 0,
            hits_until_jump: 2, // JumpByReceivedHits.hitsUntilJump.random() over 2..2
            ticks_until_jump: 2, // JumpByDelay.ticksUntilJump.random() over 2..2
        }
    }
}

/// Upstream `IntRange.random()` on a `ValueKind::IntRange` setting.
fn velocity_jump_range_random(module: &Module, name: &str, default_from: i64, default_to: i64, roll: f64) -> i64 {
    let (from, to) = module
        .setting(name)
        .map(|value| &value.kind)
        .and_then(|kind| kind.int_range())
        .unwrap_or((default_from, default_to));
    from + (roll * ((to - from + 1) as f64)).floor() as i64
}

/// Upstream `VelocityJumpReset.isCooldownOver()`. The `debugParameter`
/// calls are render-debug-only and not ported.
fn velocity_jump_is_cooldown_over(module: &Module, state: &VelocityJumpResetState) -> bool {
    let hits_enabled = tb_setting_bool(module, "JumpByReceivedHits", false);
    let delay_enabled = tb_setting_bool(module, "JumpByDelay", true);

    if hits_enabled {
        state.limit_until_jump >= state.hits_until_jump
    } else if delay_enabled {
        state.limit_until_jump >= state.ticks_until_jump
    } else {
        // If none of the options are enabled, it will go automatic
        true
    }
}

/// Upstream `VelocityJumpReset.updateLimit()`.
fn velocity_jump_update_limit(
    module: &Module,
    state: &mut VelocityJumpResetState,
    world: &dyn VelocityWorldExtended,
) {
    if tb_setting_bool(module, "JumpByReceivedHits", false) {
        if world.hurt_time() == 9 {
            state.limit_until_jump += 1;
        }
        return;
    }

    state.limit_until_jump += 1;
}

/// Upstream `VelocityJumpReset.movementInputHandler`, a `handler` on
/// `MovementInputEvent`. Returns whether `event.jump` was set true.
/// `chance_roll` is the Chance roll, `hits_roll` and `ticks_roll` the
/// re-rolls of `HitsUntilJump` / `UntilJump` after a jump.
pub fn velocity_jump_reset_input_handler(
    module: &Module,
    state: &mut VelocityJumpResetState,
    world: &dyn VelocityWorldExtended,
    chance_roll: f64,
    hits_roll: f64,
    ticks_roll: f64,
) -> bool {
    // To be able to alter velocity when receiving knockback, player must be
    // sprinting.
    if world.hurt_time() != 9
        || !world.player_on_ground()
        || !world.is_sprinting()
        || world.is_current_fall_damage()
        || !velocity_jump_is_cooldown_over(module, state)
        || !velocity_chance_as_boolean(module, chance_roll)
    {
        velocity_jump_update_limit(module, state, world);
        return false;
    }

    // event.jump = true
    state.limit_until_jump = 0;

    state.hits_until_jump = velocity_jump_range_random(module, "HitsUntilJump", 2, 2, hits_roll);
    state.ticks_until_jump = velocity_jump_range_random(module, "UntilJump", 2, 2, ticks_roll);
    true
}

#[cfg(test)]
mod velocity_reversal_jump_tests {
    use super::*;

    struct MockExtWorld {
        on_ground: bool,
        moving_input: bool,
        delta: Vec3,
        hurt_time: i64,
        sprinting: bool,
        fall_damage: bool,
    }

    impl Default for MockExtWorld {
        fn default() -> Self {
            MockExtWorld {
                on_ground: true,
                moving_input: true,
                delta: Vec3 { x: 0.4, y: -0.078, z: 0.3 },
                hurt_time: 9,
                sprinting: true,
                fall_damage: false,
            }
        }
    }

    impl VelocityWorld for MockExtWorld {
        fn player_id(&self) -> i64 {
            42
        }
        fn player_on_ground(&self) -> bool {
            self.on_ground
        }
        fn any_horizontal_input(&self) -> bool {
            self.moving_input
        }
        fn delta_movement(&self) -> Vec3 {
            self.delta
        }
        fn horizontal_speed(&self) -> f64 {
            (self.delta.x * self.delta.x + self.delta.z * self.delta.z).sqrt()
        }
        fn input_is_moving(&self) -> bool {
            self.moving_input
        }
        fn movement_yaw(&self) -> f32 {
            90.0
        }
        fn find_enemy(&self, _max_range: f64) -> Option<Vec3> {
            None
        }
        fn is_looking_at_entity(
            &self,
            _target: Vec3,
            _range: f64,
            _rotation: (f32, f32),
        ) -> Option<()> {
            None
        }
        fn current_rotation(&self) -> Option<(f32, f32)> {
            None
        }
        fn player_rotation(&self) -> (f32, f32) {
            (10.0, 0.0)
        }
    }

    impl VelocityWorldExtended for MockExtWorld {
        fn hurt_time(&self) -> i64 {
            self.hurt_time
        }
        fn is_sprinting(&self) -> bool {
            self.sprinting
        }
        fn is_current_fall_damage(&self) -> bool {
            self.fall_damage
        }
    }

    #[test]
    fn velocity_reversal_arms_on_a_local_velocity_packet() {
        let module = velocity_reversal();
        let world = MockExtWorld::default();
        let mut state = VelocityReversalState::default();

        // A non-velocity packet never arms it
        velocity_reversal_packet_handler(&module, &mut state, &world, false);
        assert!(!state.handling_velocity);

        // A local velocity packet resets and arms
        velocity_reversal_packet_handler(&module, &mut state, &world, true);
        assert!(state.handling_velocity);
        assert_eq!(state.velocity_ticks, 0);

        // OnlyMoving without input leaves the state alone
        let mut module = velocity_reversal();
        module.setting_mut("OnlyMoving").unwrap().kind = ValueKind::Bool(true);
        let mut world = MockExtWorld::default();
        world.moving_input = false;
        state.handling_velocity = false;
        velocity_reversal_packet_handler(&module, &mut state, &world, true);
        assert!(!state.handling_velocity);
    }

    #[test]
    fn velocity_reversal_reverses_the_movement_after_the_delay() {
        let module = velocity_reversal();
        let mut world = MockExtWorld::default();
        world.delta = Vec3 { x: 0.4, y: -0.078, z: 0.3 };
        let mut state = VelocityReversalState {
            handling_velocity: true,
            velocity_ticks: 0,
        };

        // Ticks 1..delay-1 count without touching the movement
        assert_eq!(
            velocity_reversal_tick_handler(&module, &mut state, &world),
            VelocityReversalTickOutcome::Untouched
        );
        assert_eq!(state.velocity_ticks, 1);

        // On the delay tick the movement flips (x *= -0.5, z *= -0.5)
        let outcome = velocity_reversal_tick_handler(&module, &mut state, &world);
        assert_eq!(
            outcome,
            VelocityReversalTickOutcome::Applied(Vec3 { x: -0.2, y: -0.078, z: -0.15 })
        );
        // reset() ran
        assert!(!state.handling_velocity);
        assert_eq!(state.velocity_ticks, 0);

        // A zero-length movement resets without applying
        state.handling_velocity = true;
        world.delta = Vec3::ZERO;
        assert_eq!(
            velocity_reversal_tick_handler(&module, &mut state, &world),
            VelocityReversalTickOutcome::Untouched
        );
        assert!(!state.handling_velocity);
    }

    #[test]
    fn velocity_jump_reset_jumps_on_the_ninth_hurt_tick() {
        let module = velocity_jump_reset();
        let world = MockExtWorld::default();
        let mut state = VelocityJumpResetState::default();

        // hurtTime 9, on ground, sprinting, no fall damage, JumpByDelay 2..2
        // with limit 0: no jump yet, the limit counts up
        assert!(!velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 1);

        // Second hurt tick: limit 1 is still under the cooldown (2)
        assert!(!velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 2);

        // Third hurt tick: the UntilJump cooldown (2) is over, so it jumps
        assert!(velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 0);
        // the ranges re-rolled over 2..2
        assert_eq!(state.hits_until_jump, 2);
        assert_eq!(state.ticks_until_jump, 2);
    }

    #[test]
    fn velocity_jump_reset_gates_skip_the_jump_but_count_the_limit() {
        let module = velocity_jump_reset();
        let mut world = MockExtWorld::default();
        let mut state = VelocityJumpResetState::default();

        // hurtTime 8: no jump, limit counts
        world.hurt_time = 8;
        assert!(!velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 1);

        // Not sprinting: no jump, limit counts
        world.hurt_time = 9;
        world.sprinting = false;
        assert!(!velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 2);

        // Current fall damage: no jump
        world.sprinting = true;
        world.fall_damage = true;
        assert!(!velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 3);

        // Chance 0%: no jump
        world.fall_damage = false;
        let mut module = velocity_jump_reset();
        module.setting_mut("Chance").unwrap().kind = ValueKind::Float(0.0, 0.0, 100.0);
        assert!(!velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 4);
    }

    #[test]
    fn velocity_jump_reset_hits_mode_counts_only_on_hurt_ticks() {
        let mut module = velocity_jump_reset();
        module.setting_mut("JumpByDelay").unwrap().kind = ValueKind::Bool(false);
        module.setting_mut("JumpByReceivedHits").unwrap().kind = ValueKind::Bool(true);
        let mut world = MockExtWorld::default();
        let mut state = VelocityJumpResetState::default();

        // hurtTime 9 on hurt ticks, 0 between: only hurt ticks count toward
        // HitsUntilJump (2)
        world.hurt_time = 9;
        assert!(!velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 1);

        world.hurt_time = 0;
        assert!(!velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 1);

        world.hurt_time = 9;
        assert!(!velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 2);

        // Fourth hurt tick: HitsUntilJump (2) is reached, so it jumps
        world.hurt_time = 9;
        assert!(velocity_jump_reset_input_handler(&module, &mut state, &world, 0.5, 0.5, 0.5));
        assert_eq!(state.limit_until_jump, 0);
    }
}

// ----- velocity/mode/VelocityLag.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Lag mode. It delays some ticks of knockback. It is equals to "delay
// velocity".

pub fn velocity_lag() -> Module {
    Module::new(
        "Lag",
        COMBAT,
        vec![
            // "ticks" suffix
            int_range("LagTime", 5, 5, 1, 20),
            boolean("JumpReset", false),
            boolean("ConsiderExplosion", true),
        ],
    )
}

/// Upstream `VelocityLag.shouldLag`, `lagTicks` and `shouldJump`, plus the
/// `waitTicks(2)` countdown that clears `shouldJump`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityLagState {
    pub should_lag: bool,
    pub lag_ticks: i64,
    pub should_jump: bool,
    /// `waitTicks(2)` bookkeeping: ticks left before `shouldJump` clears.
    pub jump_wait_ticks: i64,
}

/// Upstream `VelocityLag.disable()`.
pub fn velocity_lag_disable(state: &mut VelocityLagState) {
    state.should_lag = false;
    state.lag_ticks = 0;
}

/// Upstream `VelocityLag.packetHandler`, a `handler` on `PacketEvent`. The
/// two booleans stand in for `packet.isLocalPlayerVelocity(considerExplosion)`
/// (a `ClientboundSetEntityMotionPacket` on the local player, resp. a
/// `ClientboundExplodePacket` with player knockback).
pub fn velocity_lag_packet_handler(
    module: &Module,
    state: &mut VelocityLagState,
    is_set_entity_motion_velocity: bool,
    is_explode_player_knockback: bool,
    lag_roll: f64,
) {
    let consider_explosion = tb_setting_bool(module, "ConsiderExplosion", true);
    let is_local_velocity =
        is_set_entity_motion_velocity || (is_explode_player_knockback && consider_explosion);
    if !is_local_velocity {
        return;
    }

    state.should_lag = true;
    state.lag_ticks = velocity_jump_range_random(module, "LagTime", 5, 5, lag_roll);
}

/// What the blink queue handler did to a `BlinkPacketEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityLagBlinkOutcome {
    /// The event is untouched.
    Untouched,
    /// Upstream `event.action = Action.QUEUE`.
    Queue,
}

/// Upstream `VelocityLag.queuePacketHandler`, a `handler` on
/// `BlinkPacketEvent`.
pub fn velocity_lag_blink_packet_handler(
    state: &VelocityLagState,
    origin_incoming: bool,
    is_keep_alive: bool,
) -> VelocityLagBlinkOutcome {
    if !state.should_lag || !origin_incoming || is_keep_alive {
        return VelocityLagBlinkOutcome::Untouched;
    }

    VelocityLagBlinkOutcome::Queue
}

/// Upstream `VelocityLag.tickHandler`, a `handler` on `GameTickEvent`.
pub fn velocity_lag_tick_handler(state: &mut VelocityLagState) {
    if state.should_lag {
        state.lag_ticks -= 1;
    }
}

/// What the tick-packet-process handler did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityLagProcessOutcome {
    /// The event is untouched.
    Untouched,
    /// Upstream the state reset, `BlinkManager.flush(TransferOrigin.INCOMING)`
    /// ran, `shouldJump` armed and `waitTicks(2)` started.
    Flushed,
}

/// Upstream `VelocityLag.tickPacketProcessHandler`, a `sequenceHandler` on
/// `TickPacketProcessEvent`.
pub fn velocity_lag_process_handler(state: &mut VelocityLagState) -> VelocityLagProcessOutcome {
    if state.should_lag && state.lag_ticks == 0 {
        state.should_lag = false;
        state.lag_ticks = 0;
        // BlinkManager.flush(TransferOrigin.INCOMING)
        state.should_jump = true;
        // waitTicks(2)
        state.jump_wait_ticks = 2;
        return VelocityLagProcessOutcome::Flushed;
    }

    VelocityLagProcessOutcome::Untouched
}

/// The tail of upstream `waitTicks(2)` in the tick-packet-process handler:
/// one step per waited tick; on the last one `shouldJump = false`.
pub fn velocity_lag_wait_ticks_step(state: &mut VelocityLagState) {
    if state.jump_wait_ticks > 0 {
        state.jump_wait_ticks -= 1;
        if state.jump_wait_ticks == 0 {
            state.should_jump = false;
        }
    }
}

/// Upstream `VelocityLag.movementInputHandler`, a `handler` on
/// `MovementInputEvent`. Returns whether `event.jump` was set true.
pub fn velocity_lag_input_handler(
    module: &Module,
    state: &mut VelocityLagState,
    world: &dyn VelocityWorldExtended,
) -> bool {
    // To be able to alter velocity when receiving knockback, player must be
    // sprinting.
    if tb_setting_bool(module, "JumpReset", false)
        && state.should_jump
        && world.player_on_ground()
        && world.is_sprinting()
    {
        if world.is_current_fall_damage() {
            state.should_jump = false;
            return false;
        }

        // event.jump = true
        state.should_jump = false;
        return true;
    }

    false
}

#[cfg(test)]
mod velocity_lag_tests {
    use super::*;

    struct MockLagWorld {
        on_ground: bool,
        sprinting: bool,
        fall_damage: bool,
    }

    impl Default for MockLagWorld {
        fn default() -> Self {
            MockLagWorld {
                on_ground: true,
                sprinting: true,
                fall_damage: false,
            }
        }
    }

    impl VelocityWorld for MockLagWorld {
        fn player_id(&self) -> i64 {
            42
        }
        fn player_on_ground(&self) -> bool {
            self.on_ground
        }
        fn any_horizontal_input(&self) -> bool {
            true
        }
        fn delta_movement(&self) -> Vec3 {
            Vec3 { x: 0.4, y: -0.078, z: 0.3 }
        }
        fn horizontal_speed(&self) -> f64 {
            0.5
        }
        fn input_is_moving(&self) -> bool {
            true
        }
        fn movement_yaw(&self) -> f32 {
            90.0
        }
        fn find_enemy(&self, _max_range: f64) -> Option<Vec3> {
            None
        }
        fn is_looking_at_entity(
            &self,
            _target: Vec3,
            _range: f64,
            _rotation: (f32, f32),
        ) -> Option<()> {
            None
        }
        fn current_rotation(&self) -> Option<(f32, f32)> {
            None
        }
        fn player_rotation(&self) -> (f32, f32) {
            (10.0, 0.0)
        }
    }

    impl VelocityWorldExtended for MockLagWorld {
        fn hurt_time(&self) -> i64 {
            0
        }
        fn is_sprinting(&self) -> bool {
            self.sprinting
        }
        fn is_current_fall_damage(&self) -> bool {
            self.fall_damage
        }
    }

    #[test]
    fn velocity_lag_arms_on_a_local_velocity_packet() {
        let module = velocity_lag();
        let mut state = VelocityLagState::default();

        // Non-velocity packets never arm it
        velocity_lag_packet_handler(&module, &mut state, false, false, 0.5);
        assert!(!state.should_lag);

        // SetEntityMotion arms it regardless of ConsiderExplosion
        velocity_lag_packet_handler(&module, &mut state, true, false, 0.5);
        assert!(state.should_lag);
        assert_eq!(state.lag_ticks, 5); // LagTime 5..5

        // An explode packet arms it only with ConsiderExplosion
        state.should_lag = false;
        velocity_lag_packet_handler(&module, &mut state, false, true, 0.5);
        assert!(state.should_lag);
        state.should_lag = false;
        let mut module = velocity_lag();
        module.setting_mut("ConsiderExplosion").unwrap().kind = ValueKind::Bool(false);
        velocity_lag_packet_handler(&module, &mut state, false, true, 0.5);
        assert!(!state.should_lag);
    }

    #[test]
    fn velocity_lag_queues_incoming_packets_but_not_keepalives() {
        let module = velocity_lag();
        let mut state = VelocityLagState::default();

        // Not lagging: untouched
        assert_eq!(
            velocity_lag_blink_packet_handler(&state, true, false),
            VelocityLagBlinkOutcome::Untouched
        );

        // Lagging incoming non-keepalive: queue
        state.should_lag = true;
        assert_eq!(
            velocity_lag_blink_packet_handler(&state, true, false),
            VelocityLagBlinkOutcome::Queue
        );

        // Outgoing or keepalive: untouched
        assert_eq!(
            velocity_lag_blink_packet_handler(&state, false, false),
            VelocityLagBlinkOutcome::Untouched
        );
        assert_eq!(
            velocity_lag_blink_packet_handler(&state, true, true),
            VelocityLagBlinkOutcome::Untouched
        );
    }

    #[test]
    fn velocity_lag_flushes_when_the_lag_ticks_run_out() {
        let module = velocity_lag();
        let mut state = VelocityLagState {
            should_lag: true,
            lag_ticks: 2,
            should_jump: false,
            jump_wait_ticks: 0,
        };

        // Each game tick counts down while lagging
        velocity_lag_tick_handler(&mut state);
        velocity_lag_tick_handler(&mut state);
        assert_eq!(state.lag_ticks, 0);

        // At zero: state reset, flush, shouldJump armed, waitTicks(2) starts
        assert_eq!(
            velocity_lag_process_handler(&mut state),
            VelocityLagProcessOutcome::Flushed
        );
        assert!(!state.should_lag);
        assert_eq!(state.lag_ticks, 0);
        assert!(state.should_jump);
        assert_eq!(state.jump_wait_ticks, 2);

        // Two waited ticks later shouldJump clears
        velocity_lag_wait_ticks_step(&mut state);
        assert!(state.should_jump);
        velocity_lag_wait_ticks_step(&mut state);
        assert!(!state.should_jump);

        // Already-processed state stays untouched
        assert_eq!(
            velocity_lag_process_handler(&mut state),
            VelocityLagProcessOutcome::Untouched
        );

        // disable() resets the lag state
        state.should_lag = true;
        state.lag_ticks = 7;
        velocity_lag_disable(&mut state);
        assert!(!state.should_lag);
        assert_eq!(state.lag_ticks, 0);
    }

    #[test]
    fn velocity_lag_input_handler_jumps_on_the_ground_while_sprinting() {
        let mut module = velocity_lag();
        module.setting_mut("JumpReset").unwrap().kind = ValueKind::Bool(true);
        let mut world = MockLagWorld::default();
        let mut state = VelocityLagState::default();
        state.should_jump = true;

        // On ground, sprinting: jump set and shouldJump cleared
        assert!(velocity_lag_input_handler(&module, &mut state, &world));
        assert!(!state.should_jump);

        // JumpReset off: no jump
        state.should_jump = true;
        module.setting_mut("JumpReset").unwrap().kind = ValueKind::Bool(false);
        assert!(!velocity_lag_input_handler(&module, &mut state, &world));
        assert!(state.should_jump);

        // In the air: no jump
        module.setting_mut("JumpReset").unwrap().kind = ValueKind::Bool(true);
        world.on_ground = false;
        assert!(!velocity_lag_input_handler(&module, &mut state, &world));
        assert!(state.should_jump);

        // Not sprinting: no jump
        world.on_ground = true;
        world.sprinting = false;
        assert!(!velocity_lag_input_handler(&module, &mut state, &world));
        assert!(state.should_jump);

        // Current fall damage: no jump and shouldJump cleared
        world.sprinting = true;
        world.fall_damage = true;
        assert!(!velocity_lag_input_handler(&module, &mut state, &world));
        assert!(!state.should_jump);
    }
}

// ----- velocity/mode/VelocityReduce.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Attack Reduce. Holds incoming packets in the blink queue while the player
// closes on a target, then either bursts the queued attacks into it or
// releases the lag after a reason (flag, spectator, out of range, max delay).

/// Upstream `VelocityReduce` settings.
pub fn velocity_reduce() -> Module {
    Module::new(
        "Reduce",
        COMBAT,
        vec![
            int_range("AttackCount", 3, 3, 0, 20),
            float_range("LagTargetRange", 2.0, 6.0, 0.0, 20.0),
            // "ticks" suffix
            int_range("LagMaxDelay", 10, 10, 1, 1000),
            boolean("LagRequireKillAura", false),
            float("Horizontal", 0.6, 0.0, 1.0),
            float("Vertical", 1.0, 0.0, 1.0),
            // Upstream Debug ToggleableValueGroup (doNotIncludeAlways),
            // flattened: `Debug.notify` posts via notification()/chat().
            boolean("Debug", false),
            boolean("DebugChatMessage", false),
            boolean("DebugNotification", false),
            // Upstream BlinkEsp modes (doNotIncludeAlways); the visual modes
            // stay in the host renderer, only the selection is carried.
            choice("BlinkEsp", "Box", &["Box", "Model", "Wireframe", "None"]),
        ],
    )
}

/// An entity as the Reduce mode sees it: `Entity` fields the mode reads
/// (`id`, position, `rotation`, `isRemoved`, `shouldBeAttacked()`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VelocityReduceTarget {
    pub entity_id: i64,
    pub pos: Vec3,
    pub rotation: (f32, f32),
    pub removed: bool,
    pub attackable: bool,
}

/// Upstream `TrackedEntityPosition`: the base position and rotation a
/// rendered (blink-lagged) entity is tracked at. `handlePacket` advances it
/// with each incoming packet; the packet decode (entity position deltas)
/// stays in the host net layer and arrives here as an update tuple.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VelocityTrackedEntityPosition {
    pub base: Vec3,
    pub rotation: (f32, f32),
}

impl VelocityTrackedEntityPosition {
    pub fn new(target: VelocityReduceTarget) -> Self {
        VelocityTrackedEntityPosition {
            base: target.pos,
            rotation: target.rotation,
        }
    }
}

/// Upstream `TrackedEntityPosition.handlePacket(packet, world, trackedTarget)`.
pub fn velocity_tracked_entity_position_handle_packet(
    pos: &mut VelocityTrackedEntityPosition,
    update: Option<(Vec3, f32, f32)>,
) {
    if let Some((base, yaw, pitch)) = update {
        pos.base = base;
        pos.rotation = (yaw, pitch);
    }
}

/// Upstream `ReleaseReason` with its `debugSuffix`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityReduceReleaseReason {
    TargetReached,
    Flag,
    Spectator,
    OutOfRange,
    MaxDelay,
}

impl VelocityReduceReleaseReason {
    pub fn debug_suffix(&self) -> Option<&'static str> {
        match self {
            VelocityReduceReleaseReason::TargetReached => None,
            VelocityReduceReleaseReason::Flag => Some("flag"),
            VelocityReduceReleaseReason::Spectator => Some("spectator"),
            VelocityReduceReleaseReason::OutOfRange => Some("out of range"),
            VelocityReduceReleaseReason::MaxDelay => Some("max delay"),
        }
    }
}

/// Upstream `VelocityReduce` state: `target`, `renderTarget`,
/// `renderTargetPos`, `remainingAttackCount`, `currentGameTick`,
/// `forwardInputAttackGameTick`, `receiveDamage`, `lagTicks` and
/// `releaseReason`. `lagTicks` is `-1` when not lagging.
#[derive(Clone, Debug, PartialEq)]
pub struct VelocityReduceState {
    pub target: Option<VelocityReduceTarget>,
    pub render_target: Option<VelocityReduceTarget>,
    pub render_target_pos: Option<VelocityTrackedEntityPosition>,
    pub remaining_attack_count: i64,
    pub current_game_tick: i64,
    pub forward_input_attack_game_tick: i64,
    pub receive_damage: bool,
    pub lag_ticks: i64,
    pub release_reason: Option<VelocityReduceReleaseReason>,
}

impl Default for VelocityReduceState {
    fn default() -> Self {
        VelocityReduceState {
            target: None,
            render_target: None,
            render_target_pos: None,
            remaining_attack_count: 0,
            current_game_tick: 0,
            forward_input_attack_game_tick: -1,
            receive_damage: false,
            lag_ticks: -1,
            release_reason: None,
        }
    }
}

/// Upstream `backtrackBlocked`.
pub fn velocity_reduce_backtrack_blocked(state: &VelocityReduceState) -> bool {
    state.lag_ticks >= 0 || state.remaining_attack_count > 0
}

/// Upstream `ownsIncomingBlinkQueue`.
pub fn velocity_reduce_owns_incoming_blink_queue(state: &VelocityReduceState) -> bool {
    state.lag_ticks >= 0
}

/// Upstream `resetRenderState()`.
pub fn velocity_reduce_reset_render_state(state: &mut VelocityReduceState) {
    state.render_target = None;
    state.render_target_pos = None;
}

/// Upstream `enable()`.
pub fn velocity_reduce_enable(state: &mut VelocityReduceState) {
    *state = VelocityReduceState::default();
}

/// What upstream `disable()` did with the blink queue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityReduceDisableOutcome {
    /// Not lagging, nothing flushed.
    Untouched,
    /// `BlinkManager.flush(TransferOrigin.INCOMING)` ran.
    FlushedIncoming,
}

/// Upstream `disable()`.
pub fn velocity_reduce_disable(state: &mut VelocityReduceState) -> VelocityReduceDisableOutcome {
    let flushed = state.lag_ticks >= 0;
    *state = VelocityReduceState::default();
    if flushed {
        VelocityReduceDisableOutcome::FlushedIncoming
    } else {
        VelocityReduceDisableOutcome::Untouched
    }
}

/// Upstream `canLag`: `!lagRequireKillAura || ModuleKillAura.running`.
pub fn velocity_reduce_can_lag(module: &Module, kill_aura_running: bool) -> bool {
    !tb_setting_bool(module, "LagRequireKillAura", false) || kill_aura_running
}

/// Upstream `LagTargetRange` as `(start, endInclusive)`.
fn velocity_reduce_target_range(module: &Module) -> (f64, f64) {
    module
        .setting("LagTargetRange")
        .map(|value| &value.kind)
        .and_then(|kind| kind.float_range())
        .unwrap_or((2.0, 6.0))
}

/// Upstream `squaredBoxedDistanceTo(player)` as squared distance between the
/// player position and the entity position; the host seam carries the box
/// distance when its AABBs differ from the centre distance.
fn velocity_reduce_squared_distance(a: Vec3, b: Vec3) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = a.z - b.z;
    dx * dx + dy * dy + dz * dz
}

/// Upstream `weightedMinByOrNullAtMost(bound) { key }`: the element with the
/// smallest key, but only when that key is at most `bound`.
fn velocity_reduce_weighted_min_by_or_none_at_most<'a, T: Copy + 'a>(
    items: impl Iterator<Item = &'a T>,
    bound: f64,
    key: impl Fn(&T) -> f64,
) -> Option<T> {
    let mut best: Option<(&T, f64)> = None;
    for item in items {
        let k = key(item);
        if best.map_or(true, |(_, best_k)| k < best_k) {
            best = Some((item, k));
        }
    }
    let (item, k) = best?;
    if k <= bound {
        Some(*item)
    } else {
        None
    }
}

/// Everything `findTarget()` needs from the outside: the KillAura context,
/// the player rotation, the crosshair raytrace and the renderable entity
/// list. `crosshair_hit` is `findEntityInCrosshair(range, rotation)` resolved
/// by the host raytracer with the upstream filter
/// `{ !it.isRemoved && it.shouldBeAttacked() }` applied.
pub struct VelocityReduceFindTargetContext<'a> {
    pub kill_aura_running: bool,
    /// `ModuleKillAura.targetTracker.target`
    pub kill_aura_target: Option<VelocityReduceTarget>,
    /// `ModuleKillAura.range.interactionRange`
    pub kill_aura_interaction_range: f64,
    /// `RotationManager.currentRotation ?: player.rotation`
    pub current_rotation: Option<(f32, f32)>,
    pub player_rotation: (f32, f32),
    pub player_pos: Vec3,
    /// `world.entitiesForRendering()`, each carrying `isRemoved` and
    /// `shouldBeAttacked()` in `removed`/`attackable`.
    pub candidates: &'a [VelocityReduceTarget],
    pub crosshair_hit: Option<VelocityReduceTarget>,
}

/// Upstream `findTarget()`.
pub fn velocity_reduce_find_target(
    module: &Module,
    state: &mut VelocityReduceState,
    ctx: &VelocityReduceFindTargetContext,
) {
    let can_lag = velocity_reduce_can_lag(module, ctx.kill_aura_running);
    let (range_start, range_end) = velocity_reduce_target_range(module);

    if !can_lag && state.lag_ticks >= 0 {
        return;
    }

    if ctx.kill_aura_running && ctx.kill_aura_target.is_some() {
        let kill_aura_target = ctx.kill_aura_target.unwrap();
        if state.lag_ticks == -1 {
            state.render_target = Some(kill_aura_target);
        }
        if !can_lag
            || velocity_reduce_squared_distance(kill_aura_target.pos, ctx.player_pos)
                <= range_start * range_start
        {
            state.target = Some(kill_aura_target);
        }
        return;
    }

    let crosshair_range = if can_lag {
        range_start
    } else {
        ctx.kill_aura_interaction_range
    };
    let rotation = ctx.current_rotation.unwrap_or(ctx.player_rotation);
    let _ = crosshair_range;
    let _ = rotation;
    // findEntityInCrosshair(range, rotation) { !it.isRemoved && it.shouldBeAttacked() }
    let hit = ctx
        .crosshair_hit
        .filter(|target| !target.removed && target.attackable);
    state.target = hit;

    if state.lag_ticks == -1 {
        state.render_target = state.target;
    }

    if state.target.is_some() || state.lag_ticks >= 0 {
        return;
    }

    state.render_target = velocity_reduce_weighted_min_by_or_none_at_most(
        ctx.candidates
            .iter()
            .filter(|entity| !entity.removed && entity.attackable),
        range_end * range_end,
        |entity| velocity_reduce_squared_distance(entity.pos, ctx.player_pos),
    );
}

/// Upstream `hasLostReduceTarget()`.
pub fn velocity_reduce_has_lost_reduce_target(
    state: &VelocityReduceState,
    kill_aura_running: bool,
    kill_aura_target_id: Option<i64>,
) -> bool {
    let reduce_target = match state.target {
        Some(target) => target,
        None => return true,
    };

    if !kill_aura_running {
        return false;
    }

    let kill_aura_target_id = match kill_aura_target_id {
        Some(id) => id,
        None => return true,
    };
    kill_aura_target_id != reduce_target.entity_id
}

/// What upstream `getEspData()` feeds the render-only `BlinkEsp` modes
/// (Box/Model/Wireframe/None, kept in the host renderer): the render target
/// id, its tracked base position and rotation, while lagging.
pub fn velocity_reduce_esp_data(
    state: &VelocityReduceState,
) -> Option<(i64, Vec3, (f32, f32))> {
    if state.lag_ticks == -1 {
        return None;
    }

    let render_target = state.render_target?;
    let render_target_pos = state.render_target_pos?;
    Some((
        render_target.entity_id,
        render_target_pos.base,
        render_target_pos.rotation,
    ))
}

/// What the upstream `Debug.notify(message)` would deliver; the host posts
/// it through its notification / chat backends.
#[derive(Clone, Debug, PartialEq)]
pub struct VelocityReduceDebugNotice {
    pub message: String,
    pub notification: bool,
    pub chat: bool,
}

/// Upstream `Debug.notify(message)`.
pub fn velocity_reduce_debug_notify(module: &Module, message: &str) -> Option<VelocityReduceDebugNotice> {
    if !tb_setting_bool(module, "Debug", false) {
        return None;
    }

    let notification = tb_setting_bool(module, "DebugNotification", false);
    let chat = tb_setting_bool(module, "DebugChatMessage", false);
    if !notification && !chat {
        return None;
    }

    Some(VelocityReduceDebugNotice {
        message: message.to_string(),
        notification,
        chat,
    })
}

/// What the packet handler did with a `PacketEvent` on `TransferOrigin.INCOMING`.
#[derive(Clone, Debug, PartialEq)]
pub enum VelocityReducePacketOutcome {
    /// The event is untouched.
    Untouched,
    /// Lagging: a `ClientboundPlayerPositionPacket` set `ReleaseReason.FLAG`
    /// and the tracked render position advanced with the packet.
    LagHandling { flagged: bool, tracked: bool },
    /// `ModuleVelocity.pause > 0`, nothing happened.
    Paused,
    /// A local-player damage packet was recorded.
    ReceivedDamage,
    /// A local-player velocity packet arrived but nothing could act on it.
    NoAction,
    /// Blocked by `player.isUsingItem` or `ModuleScaffold.running`.
    BlockedByUseOrScaffold,
    /// `findTarget()` left no render target.
    NoTarget,
    /// Lag armed for `delay` ticks (`lagTicks = lagMaxDelay`).
    ArmedLag {
        delay: i64,
        debug: Option<VelocityReduceDebugNotice>,
    },
    /// Attack burst armed with `count` attacks
    /// (`remainingAttackCount = attackCount.random()`).
    ArmedAttacks { count: i64 },
}

/// Upstream `packetEventHandler`, a `handler` on `PacketEvent` (only the
/// INCOMING origin is handled). `isLocalPlayerVelocity()` takes its default
/// `considerExplosion = true`.
#[allow(clippy::too_many_arguments)]
pub fn velocity_reduce_packet_handler(
    module: &Module,
    state: &mut VelocityReduceState,
    ctx: &VelocityReduceFindTargetContext,
    origin_incoming: bool,
    is_player_position_packet: bool,
    tracked_update: Option<(Vec3, f32, f32)>,
    velocity_pause: i64,
    is_local_player_damage: bool,
    is_local_player_velocity: bool,
    is_using_item: bool,
    scaffold_running: bool,
    is_sprinting: bool,
    attack_roll: f64,
) -> VelocityReducePacketOutcome {
    if !origin_incoming {
        return VelocityReducePacketOutcome::Untouched;
    }

    if state.lag_ticks >= 0 {
        let flagged = is_player_position_packet;
        if flagged {
            state.release_reason = Some(VelocityReduceReleaseReason::Flag);
        }

        let mut tracked = false;
        if let (Some(render_target_pos), Some(render_target)) =
            (state.render_target_pos.as_mut(), state.render_target)
        {
            velocity_tracked_entity_position_handle_packet(render_target_pos, tracked_update);
            let _ = render_target;
            tracked = true;
        }

        return VelocityReducePacketOutcome::LagHandling { flagged, tracked };
    }

    if velocity_pause > 0 {
        return VelocityReducePacketOutcome::Paused;
    }

    if is_local_player_damage {
        state.receive_damage = true;
        return VelocityReducePacketOutcome::ReceivedDamage;
    } else if is_local_player_velocity && state.receive_damage {
        state.receive_damage = false;
        if is_using_item || scaffold_running {
            return VelocityReducePacketOutcome::BlockedByUseOrScaffold;
        }

        velocity_reduce_find_target(module, state, ctx);

        if state.render_target.is_none() {
            return VelocityReducePacketOutcome::NoTarget;
        }

        let can_lag = velocity_reduce_can_lag(module, ctx.kill_aura_running);
        if (state.target.is_none() && can_lag) || (state.target.is_some() && !is_sprinting) {
            let debug = if state.target.is_some() {
                velocity_reduce_debug_notify(module, "Lag... (not sprinting)")
            } else {
                velocity_reduce_debug_notify(module, "Lag...")
            };

            if state.target.is_none() {
                state.render_target_pos = state.render_target.map(VelocityTrackedEntityPosition::new);
            }
            let delay = tb_setting_i64(module, "LagMaxDelay", 10);
            state.lag_ticks = delay;
            return VelocityReducePacketOutcome::ArmedLag { delay, debug };
        } else if state.target.is_some() {
            let count = velocity_jump_range_random(module, "AttackCount", 3, 3, attack_roll);
            state.remaining_attack_count = count;
            return VelocityReducePacketOutcome::ArmedAttacks { count };
        }

        return VelocityReducePacketOutcome::NoAction;
    }

    VelocityReducePacketOutcome::NoAction
}

/// What the blink queue handler did with a `BlinkPacketEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityReduceBlinkOutcome {
    /// The event is untouched.
    Untouched,
    /// Upstream `event.action = BlinkManager.Action.QUEUE`.
    Queue,
}

/// Upstream `queuePacketHandler`, a `handler` on `BlinkPacketEvent`.
pub fn velocity_reduce_blink_packet_handler(
    state: &VelocityReduceState,
    origin_incoming: bool,
) -> VelocityReduceBlinkOutcome {
    if state.lag_ticks >= 0 && origin_incoming {
        return VelocityReduceBlinkOutcome::Queue;
    }

    VelocityReduceBlinkOutcome::Untouched
}

/// What the tick handler did on a `GameTickEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityReduceTickOutcome {
    /// No attack burst running.
    Untouched,
    /// `hasLostReduceTarget()` ended the burst early.
    LostTarget,
    /// One `attackEntity(target!!, SwingMode.DO_NOT_HIDE)` burst step ran,
    /// with the remaining count after the decrement.
    Attacked { count_after: i64 },
}

/// Upstream `tickHandler`, a `handler` on `GameTickEvent`. The world seam
/// needs the `player.isSprinting = false` and `deltaMovement` writes; both
/// are default no-ops on `VelocityWorldExtended`.
pub fn velocity_reduce_tick_handler(
    module: &Module,
    state: &mut VelocityReduceState,
    world: &mut dyn VelocityWorldExtended,
    kill_aura_running: bool,
    kill_aura_target_id: Option<i64>,
) -> VelocityReduceTickOutcome {
    state.current_game_tick += 1;

    if state.remaining_attack_count > 0 {
        if velocity_reduce_has_lost_reduce_target(state, kill_aura_running, kill_aura_target_id) {
            state.remaining_attack_count = 0;
            state.target = None;
            return VelocityReduceTickOutcome::LostTarget;
        }

        // player.isSprinting = false
        world.set_sprinting(false);
        // attackEntity(target!!, SwingMode.DO_NOT_HIDE)
        state.forward_input_attack_game_tick = state.current_game_tick;
        // player.deltaMovement = player.deltaMovement.multiply(horizontal, vertical, horizontal)
        let horizontal = tb_setting_f64(module, "Horizontal", 0.6);
        let vertical = tb_setting_f64(module, "Vertical", 1.0);
        let delta = world.delta_movement();
        world.set_delta_movement(Vec3 {
            x: delta.x * horizontal,
            y: delta.y * vertical,
            z: delta.z * horizontal,
        });
        state.remaining_attack_count -= 1;
        if state.remaining_attack_count == 0 {
            state.target = None;
        }
        return VelocityReduceTickOutcome::Attacked {
            count_after: state.remaining_attack_count,
        };
    }

    VelocityReduceTickOutcome::Untouched
}

/// What the tick-packet-process handler did on a `TickPacketProcessEvent`.
#[derive(Clone, Debug, PartialEq)]
pub enum VelocityReduceProcessOutcome {
    /// No release reason pending, the event is untouched.
    Untouched,
    /// `BlinkManager.flush(TransferOrigin.INCOMING)` ran, `lagTicks` reset to
    /// `-1` and the render state cleared. `TARGET_REACHED` also re-arms the
    /// attack count with `attackCount.random()`.
    Released {
        target_reached: bool,
        debug: Option<VelocityReduceDebugNotice>,
    },
}

/// Upstream `tickPacketProcessEventHandler`, a `handler` on
/// `TickPacketProcessEvent`.
pub fn velocity_reduce_process_handler(
    module: &Module,
    state: &mut VelocityReduceState,
    attack_roll: f64,
) -> VelocityReduceProcessOutcome {
    match state.release_reason {
        None => VelocityReduceProcessOutcome::Untouched,
        Some(release_reason) => {
            // BlinkManager.flush(TransferOrigin.INCOMING)
            state.lag_ticks = -1;
            velocity_reduce_reset_render_state(state);
            let target_reached = release_reason == VelocityReduceReleaseReason::TargetReached;
            let debug = if target_reached {
                velocity_reduce_debug_notify(module, "Finish lag")
            } else {
                velocity_reduce_debug_notify(
                    module,
                    &format!("Finish lag ({})", release_reason.debug_suffix().unwrap_or("")),
                )
            };
            if target_reached {
                state.remaining_attack_count =
                    velocity_jump_range_random(module, "AttackCount", 3, 3, attack_roll);
            }
            state.release_reason = None;
            VelocityReduceProcessOutcome::Released {
                target_reached,
                debug,
            }
        }
    }
}

/// The `MovementInputEvent` slice Reduce touches: upstream
/// `DirectionalInput.FORWARDS` and `.copy(forwards = true, backwards = false)`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityReduceInput {
    pub forwards: bool,
    pub backwards: bool,
    pub left: bool,
    pub right: bool,
}

/// What the movement input handler did on a `MovementInputEvent`.
#[derive(Clone, Debug, PartialEq)]
pub enum VelocityReduceMovementOutcome {
    /// Not lagging, the event is untouched.
    Untouched,
    /// A lag tick counted down and `findTarget()` ran; `release` is the
    /// release reason it ended with, if any.
    LagTick {
        release: Option<VelocityReduceReleaseReason>,
    },
}

/// Upstream `movementInputEventHandler`, a `handler` on `MovementInputEvent`.
pub fn velocity_reduce_movement_input_handler(
    module: &Module,
    state: &mut VelocityReduceState,
    ctx: &VelocityReduceFindTargetContext,
    player_pos: Vec3,
    flying: bool,
    input: &mut VelocityReduceInput,
) -> VelocityReduceMovementOutcome {
    let mut outcome = VelocityReduceMovementOutcome::Untouched;
    if state.lag_ticks > 0 && state.release_reason.is_none() {
        state.lag_ticks -= 1;
        velocity_reduce_find_target(module, state, ctx);

        let (_, range_end) = velocity_reduce_target_range(module);
        let release = if flying {
            Some(VelocityReduceReleaseReason::Spectator)
        } else if state.target.is_some() {
            // event.directionalInput = DirectionalInput.FORWARDS
            *input = VelocityReduceInput {
                forwards: true,
                backwards: false,
                left: false,
                right: false,
            };
            Some(VelocityReduceReleaseReason::TargetReached)
        } else {
            let base = state
                .render_target_pos
                .map(|pos| pos.base)
                .unwrap_or(Vec3 { x: 0.0, y: 0.0, z: 0.0 });
            if velocity_reduce_squared_distance(player_pos, base) > range_end * range_end {
                Some(VelocityReduceReleaseReason::OutOfRange)
            } else if state.lag_ticks == 0 {
                Some(VelocityReduceReleaseReason::MaxDelay)
            } else {
                None
            }
        };
        state.release_reason = release;
        outcome = VelocityReduceMovementOutcome::LagTick { release };
    }

    if state.current_game_tick == state.forward_input_attack_game_tick {
        // event.directionalInput = event.directionalInput.copy(
        //     forwards = true, backwards = false
        // )
        input.forwards = true;
        input.backwards = false;
    }

    outcome
}

#[cfg(test)]
mod velocity_reduce_tests {
    use super::*;

    struct MockReduceWorld {
        on_ground: bool,
        sprinting: bool,
        fall_damage: bool,
        delta: Vec3,
    }

    impl Default for MockReduceWorld {
        fn default() -> Self {
            MockReduceWorld {
                on_ground: true,
                sprinting: true,
                fall_damage: false,
                delta: Vec3 { x: 0.4, y: -0.078, z: 0.3 },
            }
        }
    }

    impl VelocityWorld for MockReduceWorld {
        fn player_id(&self) -> i64 {
            42
        }
        fn player_on_ground(&self) -> bool {
            self.on_ground
        }
        fn any_horizontal_input(&self) -> bool {
            true
        }
        fn delta_movement(&self) -> Vec3 {
            self.delta
        }
        fn horizontal_speed(&self) -> f64 {
            0.5
        }
        fn input_is_moving(&self) -> bool {
            true
        }
        fn movement_yaw(&self) -> f32 {
            90.0
        }
        fn find_enemy(&self, _max_range: f64) -> Option<Vec3> {
            None
        }
        fn is_looking_at_entity(
            &self,
            _target: Vec3,
            _range: f64,
            _rotation: (f32, f32),
        ) -> Option<()> {
            None
        }
        fn current_rotation(&self) -> Option<(f32, f32)> {
            None
        }
        fn player_rotation(&self) -> (f32, f32) {
            (10.0, 0.0)
        }
    }

    impl VelocityWorldExtended for MockReduceWorld {
        fn hurt_time(&self) -> i64 {
            0
        }
        fn is_sprinting(&self) -> bool {
            self.sprinting
        }
        fn is_current_fall_damage(&self) -> bool {
            self.fall_damage
        }
        fn set_sprinting(&mut self, sprinting: bool) {
            self.sprinting = sprinting;
        }
        fn set_delta_movement(&mut self, delta: Vec3) {
            self.delta = delta;
        }
    }

    fn target(id: i64, pos: Vec3) -> VelocityReduceTarget {
        VelocityReduceTarget {
            entity_id: id,
            pos,
            rotation: (10.0, 0.0),
            removed: false,
            attackable: true,
        }
    }

    fn ctx<'a>(
        kill_aura_running: bool,
        kill_aura_target: Option<VelocityReduceTarget>,
        candidates: &'a [VelocityReduceTarget],
        crosshair_hit: Option<VelocityReduceTarget>,
    ) -> VelocityReduceFindTargetContext<'a> {
        VelocityReduceFindTargetContext {
            kill_aura_running,
            kill_aura_target,
            kill_aura_interaction_range: 4.5,
            current_rotation: None,
            player_rotation: (10.0, 0.0),
            player_pos: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
            candidates,
            crosshair_hit,
        }
    }

    #[test]
    fn velocity_reduce_enable_disable_reset_the_state() {
        let mut state = VelocityReduceState::default();
        state.lag_ticks = 5;
        state.remaining_attack_count = 2;
        velocity_reduce_enable(&mut state);
        assert_eq!(state.lag_ticks, -1);
        assert_eq!(state.remaining_attack_count, 0);
        assert_eq!(state.forward_input_attack_game_tick, -1);

        // disable() flushes only while lagging
        state.lag_ticks = 3;
        assert_eq!(
            velocity_reduce_disable(&mut state),
            VelocityReduceDisableOutcome::FlushedIncoming
        );
        assert_eq!(state.lag_ticks, -1);
        assert_eq!(
            velocity_reduce_disable(&mut state),
            VelocityReduceDisableOutcome::Untouched
        );

        // backtrackBlocked / ownsIncomingBlinkQueue
        let mut state = VelocityReduceState::default();
        assert!(!velocity_reduce_backtrack_blocked(&state));
        assert!(!velocity_reduce_owns_incoming_blink_queue(&state));
        state.lag_ticks = 2;
        assert!(velocity_reduce_backtrack_blocked(&state));
        assert!(velocity_reduce_owns_incoming_blink_queue(&state));
        state.lag_ticks = -1;
        state.remaining_attack_count = 1;
        assert!(velocity_reduce_backtrack_blocked(&state));
        assert!(!velocity_reduce_owns_incoming_blink_queue(&state));
    }

    #[test]
    fn velocity_reduce_can_lag_follows_the_killaura_requirement() {
        let module = velocity_reduce();

        // No KillAura requirement: always can lag
        assert!(velocity_reduce_can_lag(&module, false));
        assert!(velocity_reduce_can_lag(&module, true));

        // LagRequireKillAura on: only while KillAura runs
        let mut module = velocity_reduce();
        module.setting_mut("LagRequireKillAura").unwrap().kind = ValueKind::Bool(true);
        assert!(!velocity_reduce_can_lag(&module, false));
        assert!(velocity_reduce_can_lag(&module, true));
    }

    #[test]
    fn velocity_reduce_find_target_takes_the_killaura_target_in_range() {
        let module = velocity_reduce();
        let mut state = VelocityReduceState::default();

        // KillAura target within LagTargetRange.start (2.0): target set
        let kt = target(1, Vec3 { x: 1.0, y: 0.0, z: 0.0 });
        let context = ctx(true, Some(kt), &[], None);
        velocity_reduce_find_target(&module, &mut state, &context);
        assert_eq!(state.target, Some(kt));
        assert_eq!(state.render_target, Some(kt));

        // KillAura target beyond the range: render target only, no target
        let mut state = VelocityReduceState::default();
        let kt = target(2, Vec3 { x: 5.0, y: 0.0, z: 0.0 });
        let context = ctx(true, Some(kt), &[], None);
        velocity_reduce_find_target(&module, &mut state, &context);
        assert_eq!(state.target, None);
        assert_eq!(state.render_target, Some(kt));

        // LagRequireKillAura without a running KillAura blocks the swap
        let mut module = velocity_reduce();
        module.setting_mut("LagRequireKillAura").unwrap().kind = ValueKind::Bool(true);
        let mut state = VelocityReduceState::default();
        let kt = target(3, Vec3 { x: 1.0, y: 0.0, z: 0.0 });
        let context = ctx(false, Some(kt), &[], None);
        velocity_reduce_find_target(&module, &mut state, &context);
        assert_eq!(state.target, None);
    }

    #[test]
    fn velocity_reduce_find_target_falls_back_to_the_closest_candidate() {
        let module = velocity_reduce();
        let mut state = VelocityReduceState::default();

        // Crosshair hit becomes the target
        let hit = target(7, Vec3 { x: 3.0, y: 0.0, z: 0.0 });
        let context = ctx(false, None, &[], Some(hit));
        velocity_reduce_find_target(&module, &mut state, &context);
        assert_eq!(state.target, Some(hit));
        assert_eq!(state.render_target, Some(hit));

        // No target and not lagging: closest attackable candidate within
        // LagTargetRange.end (6.0) becomes the render target
        let mut state = VelocityReduceState::default();
        let near = target(8, Vec3 { x: 3.0, y: 0.0, z: 0.0 });
        let far = target(9, Vec3 { x: 10.0, y: 0.0, z: 0.0 });
        let candidates = vec![far, near];
        let context = ctx(false, None, &candidates, None);
        velocity_reduce_find_target(&module, &mut state, &context);
        assert_eq!(state.target, None);
        assert_eq!(state.render_target, Some(near));

        // Every candidate beyond the bound: no render target
        let mut state = VelocityReduceState::default();
        let candidates = vec![far];
        let context = ctx(false, None, &candidates, None);
        velocity_reduce_find_target(&module, &mut state, &context);
        assert_eq!(state.render_target, None);

        // Removed or unattackable candidates never render
        let mut state = VelocityReduceState::default();
        let mut dead = near;
        dead.removed = true;
        let candidates = vec![dead];
        let context = ctx(false, None, &candidates, None);
        velocity_reduce_find_target(&module, &mut state, &context);
        assert_eq!(state.render_target, None);
    }

    #[test]
    fn velocity_reduce_packet_handler_arms_the_lag_after_damage() {
        let module = velocity_reduce();
        let mut state = VelocityReduceState::default();

        // Outgoing origin: untouched
        let context = ctx(false, None, &[], None);
        assert_eq!(
            velocity_reduce_packet_handler(
                &module, &mut state, &context, false, false, None, 0, false, false, false, false,
                false, 0.5
            ),
            VelocityReducePacketOutcome::Untouched
        );

        // Pause active: paused
        assert_eq!(
            velocity_reduce_packet_handler(
                &module, &mut state, &context, true, false, None, 2, false, false, false, false,
                false, 0.5
            ),
            VelocityReducePacketOutcome::Paused
        );

        // Damage recorded first
        assert_eq!(
            velocity_reduce_packet_handler(
                &module, &mut state, &context, true, false, None, 0, true, false, false, false,
                false, 0.5
            ),
            VelocityReducePacketOutcome::ReceivedDamage
        );

        // Velocity after damage with no target and no render target: NoTarget
        assert_eq!(
            velocity_reduce_packet_handler(
                &module, &mut state, &context, true, false, None, 0, false, true, false, false,
                false, 0.5
            ),
            VelocityReducePacketOutcome::NoTarget
        );
        assert!(state.receive_damage == false);

        // With a render target: lag armed for LagMaxDelay (10) with the
        // tracked position built from it
        let near = target(8, Vec3 { x: 3.0, y: 0.0, z: 0.0 });
        let candidates = vec![near];
        let context = ctx(false, None, &candidates, None);
        state.receive_damage = true;
        let outcome = velocity_reduce_packet_handler(
            &module, &mut state, &context, true, false, None, 0, false, true, false, false, false,
            0.5,
        );
        match outcome {
            VelocityReducePacketOutcome::ArmedLag { delay, debug } => {
                assert_eq!(delay, 10);
                assert_eq!(debug, None);
            }
            other => panic!("expected ArmedLag, got {:?}", other),
        }
        assert_eq!(state.lag_ticks, 10);
        assert_eq!(
            state.render_target_pos.map(|pos| pos.base),
            Some(near.pos)
        );

        // Using the item blocks the arm
        let mut state = VelocityReduceState::default();
        state.receive_damage = true;
        assert_eq!(
            velocity_reduce_packet_handler(
                &module, &mut state, &context, true, false, None, 0, false, true, true, false,
                false, 0.5
            ),
            VelocityReducePacketOutcome::BlockedByUseOrScaffold
        );
        // Scaffold running blocks too
        let mut state = VelocityReduceState::default();
        state.receive_damage = true;
        assert_eq!(
            velocity_reduce_packet_handler(
                &module, &mut state, &context, true, false, None, 0, false, true, false, true,
                false, 0.5
            ),
            VelocityReducePacketOutcome::BlockedByUseOrScaffold
        );
    }

    #[test]
    fn velocity_reduce_packet_handler_arms_attacks_with_a_target_and_sprint() {
        let module = velocity_reduce();
        let kt = target(1, Vec3 { x: 1.0, y: 0.0, z: 0.0 });
        let context = ctx(true, Some(kt), &[], None);
        let mut state = VelocityReduceState::default();

        // Damage then velocity with a target while sprinting: attack burst
        assert_eq!(
            velocity_reduce_packet_handler(
                &module, &mut state, &context, true, false, None, 0, true, false, false, false,
                false, 0.5
            ),
            VelocityReducePacketOutcome::ReceivedDamage
        );
        assert_eq!(
            velocity_reduce_packet_handler(
                &module, &mut state, &context, true, false, None, 0, false, true, false, false,
                true, 0.5
            ),
            VelocityReducePacketOutcome::ArmedAttacks { count: 3 }
        );
        assert_eq!(state.remaining_attack_count, 3);
        // no tracked position on the attack path
        assert_eq!(state.render_target_pos, None);
    }

    #[test]
    fn velocity_reduce_lag_branch_flags_and_tracks() {
        let module = velocity_reduce();
        let near = target(8, Vec3 { x: 3.0, y: 0.0, z: 0.0 });
        let candidates = [near];
        let context = ctx(false, None, &candidates, None);
        let mut state = VelocityReduceState::default();
        state.lag_ticks = 5;
        state.render_target = Some(near);
        state.render_target_pos = Some(VelocityTrackedEntityPosition::new(near));

        // A player-position packet flags the release and tracks the position
        let moved = Vec3 { x: 4.0, y: 1.0, z: 2.0 };
        assert_eq!(
            velocity_reduce_packet_handler(
                &module,
                &mut state,
                &context,
                true,
                true,
                Some((moved, 20.0, 0.0)),
                0,
                false,
                false,
                false,
                false,
                false,
                0.5
            ),
            VelocityReducePacketOutcome::LagHandling {
                flagged: true,
                tracked: true
            }
        );
        assert_eq!(
            state.release_reason,
            Some(VelocityReduceReleaseReason::Flag)
        );
        assert_eq!(
            state.render_target_pos,
            Some(VelocityTrackedEntityPosition {
                base: moved,
                rotation: (20.0, 0.0)
            })
        );

        // Lag armed earlier, then queue handler: incoming packets queue
        assert_eq!(
            velocity_reduce_blink_packet_handler(&state, true),
            VelocityReduceBlinkOutcome::Queue
        );
        // Outgoing does not
        assert_eq!(
            velocity_reduce_blink_packet_handler(&state, false),
            VelocityReduceBlinkOutcome::Untouched
        );
        // And once the lag releases, neither does
        state.lag_ticks = -1;
        assert_eq!(
            velocity_reduce_blink_packet_handler(&state, true),
            VelocityReduceBlinkOutcome::Untouched
        );
    }

    #[test]
    fn velocity_reduce_tick_handler_runs_the_attack_burst() {
        let mut module = velocity_reduce();
        module.setting_mut("Horizontal").unwrap().kind = ValueKind::Float(0.5, 0.0, 1.0);
        module.setting_mut("Vertical").unwrap().kind = ValueKind::Float(1.0, 0.0, 1.0);
        let mut world = MockReduceWorld::default();
        world.delta = Vec3 { x: 0.4, y: -0.078, z: 0.3 };
        let kt = target(1, Vec3 { x: 1.0, y: 0.0, z: 0.0 });
        let mut state = VelocityReduceState::default();
        state.target = Some(kt);
        state.remaining_attack_count = 2;

        // First burst step: sprint off, delta scaled, forward tick stamped
        assert_eq!(
            velocity_reduce_tick_handler(&module, &mut state, &mut world, true, Some(1)),
            VelocityReduceTickOutcome::Attacked { count_after: 1 }
        );
        assert_eq!(state.forward_input_attack_game_tick, state.current_game_tick);
        assert_eq!(world.delta.x, 0.2); // 0.4 * Horizontal 0.5
        assert_eq!(world.delta.z, 0.15);
        assert_eq!(world.delta.y, -0.078); // Vertical 1.0
        assert!(!world.sprinting);

        // Losing the KillAura target ends the burst early
        assert_eq!(
            velocity_reduce_tick_handler(&module, &mut state, &mut world, true, Some(2)),
            VelocityReduceTickOutcome::LostTarget
        );
        assert_eq!(state.remaining_attack_count, 0);
        assert_eq!(state.target, None);

        // Second burst step clears the target at zero
        let mut state = VelocityReduceState::default();
        state.target = Some(kt);
        state.remaining_attack_count = 1;
        assert_eq!(
            velocity_reduce_tick_handler(&module, &mut state, &mut world, true, Some(1)),
            VelocityReduceTickOutcome::Attacked { count_after: 0 }
        );
        assert_eq!(state.target, None);

        // No burst: untouched
        assert_eq!(
            velocity_reduce_tick_handler(&module, &mut state, &mut world, true, Some(1)),
            VelocityReduceTickOutcome::Untouched
        );
    }

    #[test]
    fn velocity_reduce_process_handler_releases_and_rearms() {
        let module = velocity_reduce();
        let mut state = VelocityReduceState::default();

        // No release reason: untouched
        assert_eq!(
            velocity_reduce_process_handler(&module, &mut state, 0.5),
            VelocityReduceProcessOutcome::Untouched
        );

        // TARGET_REACHED: flush, reset lag, re-arm attacks
        state.lag_ticks = 4;
        state.release_reason = Some(VelocityReduceReleaseReason::TargetReached);
        state.render_target = Some(target(1, Vec3 { x: 1.0, y: 0.0, z: 0.0 }));
        let outcome = velocity_reduce_process_handler(&module, &mut state, 0.5);
        match outcome {
            VelocityReduceProcessOutcome::Released { target_reached, debug } => {
                assert!(target_reached);
                assert_eq!(debug, None);
            }
            other => panic!("expected Released, got {:?}", other),
        }
        assert_eq!(state.lag_ticks, -1);
        assert_eq!(state.render_target, None);
        assert_eq!(state.render_target_pos, None);
        assert_eq!(state.remaining_attack_count, 3); // AttackCount 3..3
        assert_eq!(state.release_reason, None);

        // Another reason: no re-arm, debug suffix in the message
        let mut module = velocity_reduce();
        module.setting_mut("Debug").unwrap().kind = ValueKind::Bool(true);
        module.setting_mut("DebugChatMessage").unwrap().kind = ValueKind::Bool(true);
        state.release_reason = Some(VelocityReduceReleaseReason::OutOfRange);
        let outcome = velocity_reduce_process_handler(&module, &mut state, 0.5);
        match outcome {
            VelocityReduceProcessOutcome::Released { target_reached, debug } => {
                assert!(!target_reached);
                assert_eq!(
                    debug.map(|notice| notice.message),
                    Some("Finish lag (out of range)".to_string())
                );
            }
            other => panic!("expected Released, got {:?}", other),
        }
        assert_eq!(state.remaining_attack_count, 3);
    }

    #[test]
    fn velocity_reduce_movement_input_handler_releases_by_reason() {
        let module = velocity_reduce();
        let near = target(8, Vec3 { x: 3.0, y: 0.0, z: 0.0 });
        let far = target(9, Vec3 { x: 10.0, y: 0.0, z: 0.0 });

        // Flying releases as SPECTATOR before the target is checked
        let context = ctx(true, Some(near), &[], None);
        let mut state = VelocityReduceState::default();
        state.lag_ticks = 2;
        let mut input = VelocityReduceInput::default();
        assert_eq!(
            velocity_reduce_movement_input_handler(&module, &mut state, &context, Vec3 { x: 0.0, y: 0.0, z: 0.0 }, true, &mut input),
            VelocityReduceMovementOutcome::LagTick {
                release: Some(VelocityReduceReleaseReason::Spectator)
            }
        );
        assert_eq!(state.lag_ticks, 1);
        assert_eq!(input, VelocityReduceInput::default());

        // A target reached: FORWARDS input and TARGET_REACHED (KillAura
        // target within LagTargetRange.start, unlike `near` at 3.0)
        let kt = target(1, Vec3 { x: 1.0, y: 0.0, z: 0.0 });
        let context = ctx(true, Some(kt), &[], None);
        let mut state = VelocityReduceState::default();
        state.lag_ticks = 2;
        let mut input = VelocityReduceInput::default();
        input.backwards = true;
        assert_eq!(
            velocity_reduce_movement_input_handler(&module, &mut state, &context, Vec3 { x: 0.0, y: 0.0, z: 0.0 }, false, &mut input),
            VelocityReduceMovementOutcome::LagTick {
                release: Some(VelocityReduceReleaseReason::TargetReached)
            }
        );
        assert_eq!(
            input,
            VelocityReduceInput {
                forwards: true,
                backwards: false,
                left: false,
                right: false
            }
        );

        // Out of range (render target tracked at far, player at origin,
        // LagTargetRange.end 6.0)
        let mut state = VelocityReduceState::default();
        state.lag_ticks = 5;
        state.render_target_pos = Some(VelocityTrackedEntityPosition::new(far));
        let context = ctx(false, None, &[], None);
        let mut input = VelocityReduceInput::default();
        assert_eq!(
            velocity_reduce_movement_input_handler(&module, &mut state, &context, Vec3 { x: 0.0, y: 0.0, z: 0.0 }, false, &mut input),
            VelocityReduceMovementOutcome::LagTick {
                release: Some(VelocityReduceReleaseReason::OutOfRange)
            }
        );

        // Max delay when the lag ticks run out in range
        let mut state = VelocityReduceState::default();
        state.lag_ticks = 1;
        state.render_target_pos = Some(VelocityTrackedEntityPosition::new(near));
        let context = ctx(false, None, &[], None);
        let mut input = VelocityReduceInput::default();
        assert_eq!(
            velocity_reduce_movement_input_handler(&module, &mut state, &context, Vec3 { x: 0.0, y: 0.0, z: 0.0 }, false, &mut input),
            VelocityReduceMovementOutcome::LagTick {
                release: Some(VelocityReduceReleaseReason::MaxDelay)
            }
        );
        assert_eq!(state.lag_ticks, 0);

        // After an attack burst tick, forwards is forced on that game tick
        let mut world = MockReduceWorld::default();
        let kt = target(1, Vec3 { x: 1.0, y: 0.0, z: 0.0 });
        let mut state = VelocityReduceState::default();
        state.target = Some(kt);
        state.remaining_attack_count = 1;
        let mut module = velocity_reduce();
        velocity_reduce_tick_handler(&mut module, &mut state, &mut world, true, Some(1));
        let context = ctx(false, None, &[], None);
        let mut input = VelocityReduceInput::default();
        input.backwards = true;
        assert_eq!(
            velocity_reduce_movement_input_handler(&mut module, &mut state, &context, Vec3 { x: 0.0, y: 0.0, z: 0.0 }, false, &mut input),
            VelocityReduceMovementOutcome::Untouched
        );
        assert!(input.forwards);
        assert!(!input.backwards);
    }

    #[test]
    fn velocity_reduce_esp_data_needs_lag_and_a_tracked_position() {
        let near = target(8, Vec3 { x: 3.0, y: 0.0, z: 0.0 });
        let mut state = VelocityReduceState::default();

        // Not lagging: none
        assert_eq!(velocity_reduce_esp_data(&state), None);

        // Lagging without a tracked position: none
        state.lag_ticks = 3;
        state.render_target = Some(near);
        assert_eq!(velocity_reduce_esp_data(&state), None);

        // Lagging with a tracked position: id, base, rotation
        state.render_target_pos = Some(VelocityTrackedEntityPosition::new(near));
        assert_eq!(
            velocity_reduce_esp_data(&state),
            Some((8, near.pos, (10.0, 0.0)))
        );
    }
}

// ----- velocity/mode/VelocityHypixel20250127.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Deprecated upstream: will be removed with the release of 0.43.0 and
// ported to LiquidBounce Legacy Add-on.

/// Upstream `VelocityHypixel20250127.absorbedVelocity`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityHypixelState {
    pub absorbed_velocity: bool,
}

/// What the packet handler did with a `PacketEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityHypixelPacketOutcome {
    /// `ModuleFly.enabled` short-circuit, or not a local SetEntityMotion.
    Untouched,
    /// First in-air velocity update cancelled and absorbed.
    CancelledAndAbsorbed,
    /// The packet movement was rewritten to the player's own delta x/z.
    MovementReplaced { x: f64, z: f64 },
}

/// Upstream `packetHandler`, a `handler` on `PacketEvent`. `ModuleFly.enabled`
/// arrives as `fly_enabled`.
pub fn velocity_hypixel_packet_handler(
    state: &mut VelocityHypixelState,
    fly_enabled: bool,
    is_set_entity_motion: bool,
    packet_id_matches_player: bool,
    on_ground: bool,
    delta_x: f64,
    delta_z: f64,
) -> VelocityHypixelPacketOutcome {
    if fly_enabled {
        return VelocityHypixelPacketOutcome::Untouched;
    }

    // Check if this is a regular velocity update
    if is_set_entity_motion && packet_id_matches_player {
        if !on_ground && !state.absorbed_velocity {
            // event.cancelEvent()
            state.absorbed_velocity = true;
            return VelocityHypixelPacketOutcome::CancelledAndAbsorbed;
        }

        // packet.movement.x = player.deltaMovement.x
        // packet.movement.z = player.deltaMovement.z
        return VelocityHypixelPacketOutcome::MovementReplaced {
            x: delta_x,
            z: delta_z,
        };
    }

    VelocityHypixelPacketOutcome::Untouched
}

/// Upstream `gameHandler`, a `handler` on `GameTickEvent`.
pub fn velocity_hypixel_game_tick_handler(state: &mut VelocityHypixelState, on_ground: bool) {
    if on_ground {
        state.absorbed_velocity = false;
    }
}

/// Upstream `disable()` (`super.disable()` included).
pub fn velocity_hypixel_disable(state: &mut VelocityHypixelState) {
    state.absorbed_velocity = false;
}

// ----- velocity/mode/VelocityGrim2371.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Grim anticheat (2.3.71) bypass: cancels the damage velocity, queues
// incoming packets, then clicks the block under the player with a 90-degree
// pitch rotation to confirm the position.

/// Upstream `VelocityMode("Grim-2.3.71", aliases = listOf("Grim2371"))`.
pub const VELOCITY_GRIM_NAME: &str = "Grim-2.3.71";
pub const VELOCITY_GRIM_ALIASES: [&str; 1] = ["Grim2371"];

/// Upstream `MAX_FREEZE_TICKS = 20` (to prevent freezing).
pub const VELOCITY_GRIM_MAX_FREEZE_TICKS: i64 = 20;

/// A `BlockHitResult`: the block position hit and the face direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VelocityGrimBlockHit {
    pub block_pos: (i64, i64, i64),
    pub direction: VelocityGrimDirection,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityGrimDirection {
    Up,
    Down,
    North,
    South,
    West,
    East,
}

/// Upstream `BlockPos.relative(direction)`.
pub fn velocity_grim_hit_relative(hit: VelocityGrimBlockHit) -> (i64, i64, i64) {
    let (x, y, z) = hit.block_pos;
    match hit.direction {
        VelocityGrimDirection::Up => (x, y + 1, z),
        VelocityGrimDirection::Down => (x, y - 1, z),
        VelocityGrimDirection::North => (x, y, z - 1),
        VelocityGrimDirection::South => (x, y, z + 1),
        VelocityGrimDirection::West => (x - 1, y, z),
        VelocityGrimDirection::East => (x + 1, y, z),
    }
}

/// Upstream state: `cancelNextVelocity`, `delay`, `needClick`,
/// `waitForPing`, `waitForUpdate`, `hitResult`, `shouldSkip`, `freezeTicks`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityGrimState {
    pub cancel_next_velocity: bool,
    pub delay: bool,
    pub need_click: bool,
    pub wait_for_ping: bool,
    pub wait_for_update: bool,
    pub hit_result: Option<VelocityGrimBlockHit>,
    pub should_skip: bool,
    pub freeze_ticks: i64,
}

/// Upstream `enable()` (`waitForPing` is deliberately not reset upstream).
pub fn velocity_grim_enable(state: &mut VelocityGrimState) {
    state.cancel_next_velocity = false;
    state.delay = false;
    state.need_click = false;
    state.wait_for_update = false;
    state.hit_result = None;
    state.should_skip = false;
    state.freeze_ticks = 0;
}

/// Upstream `disable()`: `BlinkManager.flush(TransferOrigin.INCOMING)` runs
/// unconditionally.
pub fn velocity_grim_disable(_state: &mut VelocityGrimState) -> VelocityReduceDisableOutcome {
    VelocityReduceDisableOutcome::FlushedIncoming
}

/// What the sequence packet handler did with a `PacketEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityGrimPacketOutcome {
    /// The event is untouched.
    Untouched,
    /// An interact/attack/spectator-action/use-item-on packet flagged the
    /// next tick to skip the click.
    SkipFlagged,
    /// A `ServerboundMovePlayerPacket` with a position was cancelled while
    /// waiting for the block update.
    CancelledMovePlayer,
    /// A `ServerboundPongPacket` while waiting for the ping: `waitTicks(1)`
    /// ran and both waits cleared.
    PongReceived,
    /// A `ClientboundBlockUpdatePacket` at the player's block position:
    /// `waitTicks(1)` ran, `waitForPing` armed, `needClick` cleared.
    BlockUpdateAtPlayer,
    /// Waiting (`waitForUpdate || delay`), the event is untouched.
    Waiting,
    /// A local-player damage packet armed `cancelNextVelocity`.
    DamageSeen,
    /// The damage velocity was cancelled: `delay` armed for the click.
    VelocityCancelled,
}

/// Upstream `packetHandler`, a `sequenceHandler` on `PacketEvent`.
/// `isLocalPlayerVelocity()` takes its default `considerExplosion = true`.
#[allow(clippy::too_many_arguments)]
pub fn velocity_grim_packet_handler(
    state: &mut VelocityGrimState,
    is_interact_or_attack_or_spectator_or_use_item_on: bool,
    is_move_player_with_position: bool,
    is_pong: bool,
    is_block_update: bool,
    block_update_pos: Option<(i64, i64, i64)>,
    player_block_pos: (i64, i64, i64),
    is_local_player_damage: bool,
    is_local_player_velocity: bool,
) -> VelocityGrimPacketOutcome {
    let mut cancelled = false;

    if is_interact_or_attack_or_spectator_or_use_item_on {
        state.should_skip = true;
        return VelocityGrimPacketOutcome::SkipFlagged;
    }

    if is_move_player_with_position && state.wait_for_update {
        // event.cancelEvent()
        cancelled = true;
    }

    if is_pong && state.wait_for_ping {
        // waitTicks(1)
        state.wait_for_update = false;
        state.wait_for_ping = false;
        return VelocityGrimPacketOutcome::PongReceived;
    }

    if cancelled {
        return VelocityGrimPacketOutcome::CancelledMovePlayer;
    }

    if let Some(pos) = block_update_pos {
        if is_block_update && state.wait_for_update && pos == player_block_pos {
            // waitTicks(1)
            state.wait_for_ping = true;
            state.need_click = false;
            return VelocityGrimPacketOutcome::BlockUpdateAtPlayer;
        }
    }

    if state.wait_for_update || state.delay {
        return VelocityGrimPacketOutcome::Waiting;
    }

    // Check for damage to make sure it will only cancel damage velocity (that
    // all we need), and not affect other types of velocity
    if is_local_player_damage {
        state.cancel_next_velocity = true;
        return VelocityGrimPacketOutcome::DamageSeen;
    } else if state.cancel_next_velocity && is_local_player_velocity {
        // event.cancelEvent()
        state.delay = true;
        state.cancel_next_velocity = false;
        state.need_click = true;
        return VelocityGrimPacketOutcome::VelocityCancelled;
    }

    VelocityGrimPacketOutcome::Untouched
}

/// What the blink queue handler did with a `BlinkPacketEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityGrimBlinkOutcome {
    /// The event is untouched.
    Untouched,
    /// Upstream `event.action = BlinkManager.Action.QUEUE`.
    Queue,
}

/// Upstream `queuePacketHandler`, a `handler` on `BlinkPacketEvent`.
pub fn velocity_grim_blink_packet_handler(
    state: &VelocityGrimState,
    origin_incoming: bool,
) -> VelocityGrimBlinkOutcome {
    if state.wait_for_update || !state.delay || !origin_incoming {
        return VelocityGrimBlinkOutcome::Untouched;
    }

    VelocityGrimBlinkOutcome::Queue
}

/// The `ServerboundMovePlayerPacket` the click sends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityGrimClickSend {
    /// `ServerboundMovePlayerPacket.Rot(player.yRot, 90f, onGround,
    /// horizontalCollision)`, sent when the server rotation pitch is not 90.
    Rot {
        yaw: f32,
        on_ground: bool,
        horizontal_collision: bool,
    },
    /// `ServerboundMovePlayerPacket.StatusOnly(onGround, horizontalCollision)`.
    StatusOnly {
        on_ground: bool,
        horizontal_collision: bool,
    },
}

/// What the player tick handler did on a `PlayerTickEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityGrimTickOutcome {
    /// Nothing happened.
    Untouched,
    /// The click ran: incoming flush, `useItemOn` (with `swing` telling
    /// whether `shouldSwingHand()` triggered the DO_NOT_HIDE swing) and the
    /// rotation/status packet sent.
    Clicked { swing: bool, sent: VelocityGrimClickSend },
    /// The tick was cancelled while waiting for the block update.
    Frozen,
    /// The freeze cap (`MAX_FREEZE_TICKS = 20`) was exceeded and all waits
    /// cleared.
    FreezeCapCleared,
}

/// Upstream `playerTickHandler`, a `handler` on `PlayerTickEvent`. The
/// `traceFromPlayer(rotation = serverRotation.copy(pitch = 90F))` raytrace
/// arrives resolved as `trace_hit`, and `useItemOn` as `use_item_on_swing`.
#[allow(clippy::too_many_arguments)]
pub fn velocity_grim_player_tick_handler(
    state: &mut VelocityGrimState,
    trace_hit: Option<VelocityGrimBlockHit>,
    player_block_pos: (i64, i64, i64),
    is_using_item: bool,
    server_rotation_pitch: f32,
    player_yaw: f32,
    on_ground: bool,
    horizontal_collision: bool,
    use_item_on_swing: bool,
) -> VelocityGrimTickOutcome {
    let mut outcome = VelocityGrimTickOutcome::Untouched;

    if state.need_click && !state.should_skip && !is_using_item {
        state.hit_result = trace_hit
            .filter(|hit| velocity_grim_hit_relative(*hit) == player_block_pos);
    }

    if state.hit_result.is_some() {
        state.delay = false;

        // BlinkManager.flush(TransferOrigin.INCOMING)
        // interaction.useItemOn(player, MAIN_HAND, hitResult).shouldSwingHand()
        //   -> SwingMode.DO_NOT_HIDE.interactSwing(MAIN_HAND)
        let swing = use_item_on_swing;

        let sent = if server_rotation_pitch != 90.0 {
            VelocityGrimClickSend::Rot {
                yaw: player_yaw,
                on_ground,
                horizontal_collision,
            }
        } else {
            VelocityGrimClickSend::StatusOnly {
                on_ground,
                horizontal_collision,
            }
        };

        state.freeze_ticks = 0;
        state.wait_for_update = true;
        state.hit_result = None;
        state.need_click = false;
        outcome = VelocityGrimTickOutcome::Clicked { swing, sent };
    }

    if state.wait_for_update {
        // event.cancelEvent()
        state.freeze_ticks += 1;
        if state.freeze_ticks > VELOCITY_GRIM_MAX_FREEZE_TICKS {
            state.wait_for_update = false;
            state.wait_for_ping = false;
            state.need_click = false;
            return VelocityGrimTickOutcome::FreezeCapCleared;
        } else if outcome == VelocityGrimTickOutcome::Untouched {
            outcome = VelocityGrimTickOutcome::Frozen;
        }
    }

    state.should_skip = false;
    outcome
}

#[cfg(test)]
mod velocity_hypixel_grim_tests {
    use super::*;

    #[test]
    fn velocity_hypixel_cancels_the_first_air_velocity_and_rewrites_the_rest() {
        let mut state = VelocityHypixelState::default();

        // Fly enabled short-circuits everything
        assert_eq!(
            velocity_hypixel_packet_handler(&mut state, true, true, true, false, 0.3, 0.4),
            VelocityHypixelPacketOutcome::Untouched
        );

        // Not a SetEntityMotion for the local player: untouched
        assert_eq!(
            velocity_hypixel_packet_handler(&mut state, false, false, true, false, 0.3, 0.4),
            VelocityHypixelPacketOutcome::Untouched
        );

        // First in-air velocity: cancelled and absorbed
        assert_eq!(
            velocity_hypixel_packet_handler(&mut state, false, true, true, false, 0.3, 0.4),
            VelocityHypixelPacketOutcome::CancelledAndAbsorbed
        );
        assert!(state.absorbed_velocity);

        // The next in-air velocity: movement rewritten to the player delta
        assert_eq!(
            velocity_hypixel_packet_handler(&mut state, false, true, true, false, 0.3, 0.4),
            VelocityHypixelPacketOutcome::MovementReplaced { x: 0.3, z: 0.4 }
        );

        // On the ground the absorbed flag resets
        velocity_hypixel_game_tick_handler(&mut state, true);
        assert!(!state.absorbed_velocity);
        // ...and the first air velocity cancels again
        assert_eq!(
            velocity_hypixel_packet_handler(&mut state, false, true, true, false, 0.3, 0.4),
            VelocityHypixelPacketOutcome::CancelledAndAbsorbed
        );

        // disable() clears the flag
        velocity_hypixel_disable(&mut state);
        assert!(!state.absorbed_velocity);
    }

    #[test]
    fn velocity_grim_packet_flow_cancels_the_damage_velocity_once() {
        let mut state = VelocityGrimState::default();
        velocity_grim_enable(&mut state);

        // Interact/attack/use-item-on packets flag the skip
        assert_eq!(
            velocity_grim_packet_handler(&mut state, true, false, false, false, None, (0, 0, 0), false, false),
            VelocityGrimPacketOutcome::SkipFlagged
        );
        assert!(state.should_skip);

        // Damage recorded, then the velocity cancelled and the click armed
        assert_eq!(
            velocity_grim_packet_handler(&mut state, false, false, false, false, None, (0, 0, 0), true, false),
            VelocityGrimPacketOutcome::DamageSeen
        );
        assert!(state.cancel_next_velocity);
        assert_eq!(
            velocity_grim_packet_handler(&mut state, false, false, false, false, None, (0, 0, 0), false, true),
            VelocityGrimPacketOutcome::VelocityCancelled
        );
        assert!(state.delay);
        assert!(state.need_click);
        assert!(!state.cancel_next_velocity);

        // While delay holds, further packets wait
        assert_eq!(
            velocity_grim_packet_handler(&mut state, false, false, false, false, None, (0, 0, 0), true, false),
            VelocityGrimPacketOutcome::Waiting
        );

        // Incoming packets queue while delay is on and not waiting for an update
        assert_eq!(
            velocity_grim_blink_packet_handler(&state, true),
            VelocityGrimBlinkOutcome::Queue
        );
        assert_eq!(
            velocity_grim_blink_packet_handler(&state, false),
            VelocityGrimBlinkOutcome::Untouched
        );
    }

    #[test]
    fn velocity_grim_block_update_and_pong_clear_the_waits() {
        let mut state = VelocityGrimState::default();
        velocity_grim_enable(&mut state);
        state.delay = true;
        state.need_click = true;

        // The click ran, so the mode waits for the block update
        state.wait_for_update = true;

        // Move player packets with a position are cancelled while waiting
        assert_eq!(
            velocity_grim_packet_handler(&mut state, false, true, false, false, None, (5, 60, 5), false, false),
            VelocityGrimPacketOutcome::CancelledMovePlayer
        );

        // A block update elsewhere does nothing
        assert_eq!(
            velocity_grim_packet_handler(&mut state, false, false, false, true, Some((9, 60, 9)), (5, 60, 5), false, false),
            VelocityGrimPacketOutcome::Waiting
        );

        // The block update at the player's own block position: ping armed
        assert_eq!(
            velocity_grim_packet_handler(&mut state, false, false, false, true, Some((5, 60, 5)), (5, 60, 5), false, false),
            VelocityGrimPacketOutcome::BlockUpdateAtPlayer
        );
        assert!(state.wait_for_ping);
        assert!(!state.need_click);

        // Incoming packets no longer queue while waiting for the update
        assert_eq!(
            velocity_grim_blink_packet_handler(&state, true),
            VelocityGrimBlinkOutcome::Untouched
        );

        // The pong clears both waits after waitTicks(1)
        assert_eq!(
            velocity_grim_packet_handler(&mut state, false, false, true, false, None, (5, 60, 5), false, false),
            VelocityGrimPacketOutcome::PongReceived
        );
        assert!(!state.wait_for_ping);
        assert!(!state.wait_for_update);

        // disable() always flushes
        assert_eq!(
            velocity_grim_disable(&mut state),
            VelocityReduceDisableOutcome::FlushedIncoming
        );
    }

    #[test]
    fn velocity_grim_click_arms_from_the_trace_and_freezes() {
        let mut state = VelocityGrimState::default();
        velocity_grim_enable(&mut state);
        state.delay = true;
        state.need_click = true;

        // A trace that does not resolve to the block above the player's own
        // block position is dropped
        let miss = VelocityGrimBlockHit {
            block_pos: (9, 60, 9),
            direction: VelocityGrimDirection::Up,
        };
        let hit = VelocityGrimBlockHit {
            block_pos: (5, 59, 5),
            direction: VelocityGrimDirection::Up,
        };
        assert_eq!(
            velocity_grim_player_tick_handler(
                &mut state, Some(miss), (5, 60, 5), false, 0.0, 10.0, true, false, true
            ),
            VelocityGrimTickOutcome::Untouched
        );
        assert_eq!(state.hit_result, None);

        // The right trace arms the hit and the click runs in the same tick:
        // flush, useItemOn swing, rotation packet, freeze armed
        assert_eq!(
            velocity_grim_player_tick_handler(
                &mut state, Some(hit), (5, 60, 5), false, 0.0, 10.0, true, false, true
            ),
            VelocityGrimTickOutcome::Clicked {
                swing: true,
                sent: VelocityGrimClickSend::Rot {
                    yaw: 10.0,
                    on_ground: true,
                    horizontal_collision: false
                }
            }
        );
        assert_eq!(state.hit_result, None);
        assert!(state.wait_for_update);
        assert!(!state.delay);
        assert!(!state.need_click);

        // Waiting ticks freeze the player event
        assert_eq!(
            velocity_grim_player_tick_handler(
                &mut state, None, (5, 60, 5), false, 0.0, 10.0, true, false, true
            ),
            VelocityGrimTickOutcome::Frozen
        );
        // the click tick itself also froze (upstream freezes in the same handler pass)
        assert_eq!(state.freeze_ticks, 2);

        // Using the item blocks the trace arm; shouldSkip skips it too
        state.wait_for_update = false;
        state.need_click = true;
        state.should_skip = true;
        assert_eq!(
            velocity_grim_player_tick_handler(
                &mut state, Some(hit), (5, 60, 5), false, 0.0, 10.0, true, false, true
            ),
            VelocityGrimTickOutcome::Untouched
        );
        assert_eq!(state.hit_result, None);
        // shouldSkip clears at the end of every tick
        assert!(!state.should_skip);

        // Server pitch already at 90 sends the status-only packet (still in
        // the same tick as the trace)
        let mut state = VelocityGrimState::default();
        velocity_grim_enable(&mut state);
        state.need_click = true;
        assert_eq!(
            velocity_grim_player_tick_handler(
                &mut state, Some(hit), (5, 60, 5), false, 90.0, 10.0, true, false, false
            ),
            VelocityGrimTickOutcome::Clicked {
                swing: false,
                sent: VelocityGrimClickSend::StatusOnly {
                    on_ground: true,
                    horizontal_collision: false
                }
            }
        );
    }

    #[test]
    fn velocity_grim_freeze_cap_clears_the_waits() {
        let mut state = VelocityGrimState::default();
        velocity_grim_enable(&mut state);
        state.wait_for_update = true;
        state.wait_for_ping = true;
        state.need_click = true;
        state.freeze_ticks = VELOCITY_GRIM_MAX_FREEZE_TICKS;

        assert_eq!(
            velocity_grim_player_tick_handler(
                &mut state, None, (5, 60, 5), false, 0.0, 10.0, true, false, true
            ),
            VelocityGrimTickOutcome::FreezeCapCleared
        );
        assert!(!state.wait_for_update);
        assert!(!state.wait_for_ping);
        assert!(!state.need_click);
    }
}

// ----- velocity/mode/VelocityGrim2344MC117.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Duplicate-exempt Grim bypass. Abuses the C06 duplicate exempt to bypass
// the velocity check: after sending a finish-mining digging packet that
// coincides with the player's collision box and canceling the knockback
// packet sent by the server before the player's movement packet is sent,
// Grim seems to ignore the player's knockback.
// https://github.com/GrimAnticheat/Grim/issues/1133

/// Upstream `VelocityMode("Grim-2.3.44-MC1.17", aliases = listOf("Grim2344-117"))`.
pub const VELOCITY_GRIM2344_NAME: &str = "Grim-2.3.44-MC1.17";
pub const VELOCITY_GRIM2344_ALIASES: [&str; 1] = ["Grim2344-117"];

pub fn velocity_grim2344() -> Module {
    Module::new(
        "Grim2344",
        COMBAT,
        vec![boolean("AlternativeBypass", true)],
    )
}

/// Upstream `VelocityGrim2344MC117.canCancel`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityGrim2344State {
    pub can_cancel: bool,
}

/// Upstream `enable()`.
pub fn velocity_grim2344_enable(state: &mut VelocityGrim2344State) {
    state.can_cancel = false;
}

/// The `ServerboundPlayerActionPacket` the mode sends: `STOP_DESTROY_BLOCK`
/// at the player's block position with `player.direction.opposite`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VelocityGrim2344StopDestroy {
    pub block_pos: (i64, i64, i64),
    pub direction_opposite: VelocityGrimDirection,
}

/// What the packet handler did with a `PacketEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityGrim2344PacketOutcome {
    /// The event is untouched.
    Untouched,
    /// The damage velocity was cancelled, `waitTicks(1)` ran and the
    /// duplicate-exempt packets were sent.
    VelocityCancelled {
        /// `repeat(if (alternativeBypass) 4 else 1)`: the `PosRot` sends.
        pos_rot_sends: u32,
        stop_destroy_block: VelocityGrim2344StopDestroy,
    },
}

/// Upstream `packetHandler`, a `sequenceHandler` on `PacketEvent`.
/// `isLocalPlayerVelocity()` takes its default `considerExplosion = true`.
#[allow(clippy::too_many_arguments)]
pub fn velocity_grim2344_packet_handler(
    module: &Module,
    state: &mut VelocityGrim2344State,
    is_damage_event_for_player: bool,
    is_local_player_velocity: bool,
    player_block_pos: (i64, i64, i64),
    player_direction: VelocityGrimDirection,
) -> VelocityGrim2344PacketOutcome {
    // Check for damage to make sure it will only cancel
    // damage velocity (that all we need) and not affect other types of velocity
    if is_damage_event_for_player {
        state.can_cancel = true;
    }

    if is_local_player_velocity && state.can_cancel {
        // event.cancelEvent()
        // waitTicks(1)
        let alternative_bypass = tb_setting_bool(module, "AlternativeBypass", true);
        let pos_rot_sends = if alternative_bypass { 4 } else { 1 };
        // network.send(PosRot(player.x, player.y, player.z, player.yRot,
        //   player.xRot, player.onGround(), player.horizontalCollision)) x N
        // network.send(ServerboundPlayerActionPacket(
        //   STOP_DESTROY_BLOCK, player.blockPosition(), player.direction.opposite))
        state.can_cancel = false;
        return VelocityGrim2344PacketOutcome::VelocityCancelled {
            pos_rot_sends,
            stop_destroy_block: VelocityGrim2344StopDestroy {
                block_pos: player_block_pos,
                direction_opposite: velocity_grim_direction_opposite(player_direction),
            },
        };
    }

    VelocityGrim2344PacketOutcome::Untouched
}

/// Upstream `player.direction.opposite` on a block face direction.
pub fn velocity_grim_direction_opposite(direction: VelocityGrimDirection) -> VelocityGrimDirection {
    match direction {
        VelocityGrimDirection::Up => VelocityGrimDirection::Down,
        VelocityGrimDirection::Down => VelocityGrimDirection::Up,
        VelocityGrimDirection::North => VelocityGrimDirection::South,
        VelocityGrimDirection::South => VelocityGrimDirection::North,
        VelocityGrimDirection::West => VelocityGrimDirection::East,
        VelocityGrimDirection::East => VelocityGrimDirection::West,
    }
}

// ----- velocity/mode/VelocityIntave20261002.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Intave (2026-10-02): an attack-timed knockback reducer plus a
// chance-driven jump reset that absorbs hurt velocity on the ground.

/// Upstream `VelocityMode("Intave-2026-10-02", aliases = listOf("Intave"))`.
pub const VELOCITY_INTAVE_NAME: &str = "Intave-2026-10-02";
pub const VELOCITY_INTAVE_ALIASES: [&str; 1] = ["Intave"];

pub fn velocity_intave() -> Module {
    Module::new(
        "Intave",
        COMBAT,
        vec![
            // ReduceOnAttack group
            boolean("ReduceOnAttack", true),
            float("Factor", 0.6, 0.6, 1.0),
            int_range("HurtTime", 5, 7, 1, 10),
            int("LastAttackTimeToReduce", 2000, 1, 10000),
            // JumpReset group
            boolean("JumpReset", true),
            float("Chance", 50.0, 0.0, 100.0),
            // Randomize group
            boolean("Randomize", false),
            int_range("DelayTicks", 0, 5, 0, 10),
        ],
    )
}

/// Upstream `ReduceOnAttack.lastAttackTime` plus the `JumpReset` delay state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityIntaveState {
    pub last_attack_time: u64,
    pub current_delay: i64,
    pub delay_counter: i64,
}

/// What the attack handler did on an `AttackEntityEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityIntaveAttackOutcome {
    /// The delta movement is untouched.
    Untouched,
    /// `player.deltaMovement.multiply(factorX = reduceFactor, factorZ =
    /// reduceFactor)`: the new horizontal delta.
    DeltaReduced { x: f64, z: f64 },
}

/// Upstream `ReduceOnAttack.attackHandler`, a `handler` on
/// `AttackEntityEvent`. `lastAttackTime` updates on every attack.
pub fn velocity_intave_attack_handler(
    module: &Module,
    state: &mut VelocityIntaveState,
    now_ms: u64,
    hurt_time: i32,
    delta_x: f64,
    delta_y: f64,
    delta_z: f64,
) -> VelocityIntaveAttackOutcome {
    let _ = delta_y;
    let mut outcome = VelocityIntaveAttackOutcome::Untouched;

    if tb_setting_bool(module, "ReduceOnAttack", true)
        && velocity_intave_hurt_time_contains(module, hurt_time)
        && now_ms.saturating_sub(state.last_attack_time)
            <= tb_setting_i64(module, "LastAttackTimeToReduce", 2000) as u64
    {
        let factor = tb_setting_f64(module, "Factor", 0.6);
        outcome = VelocityIntaveAttackOutcome::DeltaReduced {
            x: delta_x * factor,
            z: delta_z * factor,
        };
    }

    state.last_attack_time = now_ms;
    outcome
}

/// Upstream `player.hurtTime in hurtTime` on the `HurtTime` intRange setting.
fn velocity_intave_hurt_time_contains(module: &Module, hurt_time: i32) -> bool {
    let (from, to) = module
        .setting("HurtTime")
        .map(|value| &value.kind)
        .and_then(|kind| kind.int_range())
        .unwrap_or((5, 7));
    (from..=to).contains(&(hurt_time as i64))
}

/// What the movement-input handler decided for the jump input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityIntaveJumpOutcome {
    /// The jump input stays untouched.
    Untouched,
    /// Upstream `it.jump = true`.
    JumpQueued,
    /// Randomize advanced past its current delay and re-rolled
    /// `currentDelay` from the `DelayTicks` range.
    DelayRerolled { new_delay: i64 },
    /// Jump queued and the delay re-rolled in the same tick.
    JumpQueuedAndDelayRerolled { new_delay: i64 },
}

/// Upstream `JumpReset.tickJumpHandler`, a `handler` on
/// `MovementInputEvent`. `Random.nextInt(100) < chance` arrives as the
/// host-rolled `rng_roll` in 0..100; `LocalPlayerFallDamageTracker
/// .isCurrentFallDamage` as `is_current_fall_damage`; `mc.gui.screen() !is
/// InventoryScreen` as `screen_is_inventory`.
#[allow(clippy::too_many_arguments)]
pub fn velocity_intave_movement_input_handler(
    module: &Module,
    state: &mut VelocityIntaveState,
    rng_roll: i64,
    hurt_time: i32,
    is_current_fall_damage: bool,
    on_ground: bool,
    screen_is_inventory: bool,
    delay_roll: f64,
) -> VelocityIntaveJumpOutcome {
    if !tb_setting_bool(module, "JumpReset", true) {
        return VelocityIntaveJumpOutcome::Untouched;
    }

    let chance = tb_setting_f64(module, "Chance", 50.0);
    let should_jump = (rng_roll as f64) < chance
        && hurt_time > 5
        && !is_current_fall_damage;
    let can_jump = on_ground && !screen_is_inventory;
    let should_finally_jump = should_jump && can_jump;

    if tb_setting_bool(module, "Randomize", false) {
        state.delay_counter += 1;

        if state.delay_counter >= state.current_delay {
            let new_delay = velocity_jump_range_random(module, "DelayTicks", 0, 5, delay_roll);
            state.delay_counter = 0;
            state.current_delay = new_delay;
            return if should_finally_jump {
                VelocityIntaveJumpOutcome::JumpQueuedAndDelayRerolled { new_delay }
            } else {
                VelocityIntaveJumpOutcome::DelayRerolled { new_delay }
            };
        }
        return VelocityIntaveJumpOutcome::Untouched;
    }

    if should_finally_jump {
        return VelocityIntaveJumpOutcome::JumpQueued;
    }

    VelocityIntaveJumpOutcome::Untouched
}

#[cfg(test)]
mod velocity_grim2344_intave_tests {
    use super::*;

    fn grim2344_module() -> Module {
        velocity_grim2344()
    }

    fn intave_module() -> Module {
        velocity_intave()
    }

    #[test]
    fn velocity_grim2344_sends_the_duplicate_exempt_packets_after_a_damage_velocity() {
        let module = grim2344_module();
        let mut state = VelocityGrim2344State::default();

        // Velocity without a prior damage packet: untouched
        assert_eq!(
            velocity_grim2344_packet_handler(
                &module, &mut state, false, true, (5, 60, 5), VelocityGrimDirection::North
            ),
            VelocityGrim2344PacketOutcome::Untouched
        );

        // Damage packet for the local player arms the cancel
        assert_eq!(
            velocity_grim2344_packet_handler(
                &module, &mut state, true, false, (5, 60, 5), VelocityGrimDirection::North
            ),
            VelocityGrim2344PacketOutcome::Untouched
        );
        assert!(state.can_cancel);

        // The velocity then cancels: waitTicks(1), 4 PosRot sends
        // (AlternativeBypass default true) and the stop-destroy action with
        // the opposite face direction
        assert_eq!(
            velocity_grim2344_packet_handler(
                &module, &mut state, false, true, (5, 60, 5), VelocityGrimDirection::North
            ),
            VelocityGrim2344PacketOutcome::VelocityCancelled {
                pos_rot_sends: 4,
                stop_destroy_block: VelocityGrim2344StopDestroy {
                    block_pos: (5, 60, 5),
                    direction_opposite: VelocityGrimDirection::South,
                },
            }
        );
        assert!(!state.can_cancel);

        // enable() clears the flag
        state.can_cancel = true;
        velocity_grim2344_enable(&mut state);
        assert!(!state.can_cancel);
    }

    #[test]
    fn velocity_grim2344_alternative_bypass_sends_one_pos_rot() {
        let mut module = grim2344_module();
        module.setting_mut("AlternativeBypass").unwrap().kind = ValueKind::Bool(false);
        let mut state = VelocityGrim2344State::default();
        state.can_cancel = true;

        assert_eq!(
            velocity_grim2344_packet_handler(
                &module, &mut state, false, true, (5, 60, 5), VelocityGrimDirection::East
            ),
            VelocityGrim2344PacketOutcome::VelocityCancelled {
                pos_rot_sends: 1,
                stop_destroy_block: VelocityGrim2344StopDestroy {
                    block_pos: (5, 60, 5),
                    direction_opposite: VelocityGrimDirection::West,
                },
            }
        );
    }

    #[test]
    fn velocity_intave_attack_reduces_the_delta_inside_the_hurt_and_time_windows() {
        let module = intave_module();
        let mut state = VelocityIntaveState::default();

        // No prior attack recorded: the window check passes (now - 0 <= 2000)
        // and the delta reduces by the 0.6 factor
        assert_eq!(
            velocity_intave_attack_handler(&module, &mut state, 1000, 6, 0.5, 0.1, 0.9),
            VelocityIntaveAttackOutcome::DeltaReduced { x: 0.3, z: 0.54 }
        );
        assert_eq!(state.last_attack_time, 1000);

        // hurtTime outside the 5..7 window: untouched, but the time records
        assert_eq!(
            velocity_intave_attack_handler(&module, &mut state, 1200, 4, 0.5, 0.1, 0.9),
            VelocityIntaveAttackOutcome::Untouched
        );
        assert_eq!(state.last_attack_time, 1200);

        // A second attack inside 2000 ms with the right hurtTime: reduced
        assert_eq!(
            velocity_intave_attack_handler(&module, &mut state, 1500, 7, 1.0, 0.1, 0.0),
            VelocityIntaveAttackOutcome::DeltaReduced { x: 0.6, z: 0.0 }
        );

        // Beyond the 2000 ms window: untouched
        assert_eq!(
            velocity_intave_attack_handler(&module, &mut state, 4000, 6, 1.0, 0.1, 0.0),
            VelocityIntaveAttackOutcome::Untouched
        );
    }

    #[test]
    fn velocity_intave_jump_reset_queues_the_jump_on_the_ground() {
        let module = intave_module();
        let mut state = VelocityIntaveState::default();

        // In air: no jump even with a winning roll
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 10, 6, false, false, false, 0.0
            ),
            VelocityIntaveJumpOutcome::Untouched
        );

        // Inventory open: no jump
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 10, 6, false, true, true, 0.0
            ),
            VelocityIntaveJumpOutcome::Untouched
        );

        // Current fall damage: no jump
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 10, 6, true, true, false, 0.0
            ),
            VelocityIntaveJumpOutcome::Untouched
        );

        // hurtTime at or below 5: no jump
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 10, 5, false, true, false, 0.0
            ),
            VelocityIntaveJumpOutcome::Untouched
        );

        // Winning roll (0 < 50) on the ground: jump queued
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 10, 6, false, true, false, 0.0
            ),
            VelocityIntaveJumpOutcome::JumpQueued
        );

        // Losing roll (99 >= 50): untouched
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 99, 6, false, true, false, 0.0
            ),
            VelocityIntaveJumpOutcome::Untouched
        );
    }

    #[test]
    fn velocity_intave_randomize_gates_the_jump_on_its_delay_counter() {
        let mut module = intave_module();
        module.setting_mut("Randomize").unwrap().kind = ValueKind::Bool(true);
        let mut state = VelocityIntaveState::default();
        // currentDelay 0 fires on the first tick
        state.current_delay = 0;

        // First tick: counter reaches the delay, jump queues and the delay
        // re-rolls from DelayTicks 0..5
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 10, 6, false, true, false, 0.75
            ),
            VelocityIntaveJumpOutcome::JumpQueuedAndDelayRerolled { new_delay: 4 }
        );
        assert_eq!(state.delay_counter, 0);
        assert_eq!(state.current_delay, 4);

        // The next three ticks only count up (counters 1, 2, 3 < 4)
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 10, 6, false, true, false, 0.0
            ),
            VelocityIntaveJumpOutcome::Untouched
        );
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 10, 6, false, true, false, 0.0
            ),
            VelocityIntaveJumpOutcome::Untouched
        );
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 10, 6, false, true, false, 0.0
            ),
            VelocityIntaveJumpOutcome::Untouched
        );
        assert_eq!(state.delay_counter, 3);

        // Losing roll when the counter expires: only the delay re-rolls
        assert_eq!(
            velocity_intave_movement_input_handler(
                &module, &mut state, 99, 6, false, true, false, 0.5
            ),
            VelocityIntaveJumpOutcome::DelayRerolled { new_delay: 3 }
        );
        assert_eq!(state.delay_counter, 0);
        assert_eq!(state.current_delay, 3);
    }
}

// ----- velocity/mode/VelocityAAC442.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Velocity for AAC4.4.2, pretty sure, it works on other versions

/// Upstream `VelocityMode("AAC-4.4.2", aliases = listOf("AAC4.4.2"))`.
pub const VELOCITY_AAC442_NAME: &str = "AAC-4.4.2";
pub const VELOCITY_AAC442_ALIASES: [&str; 1] = ["AAC4.4.2"];

pub fn velocity_aac442() -> Module {
    Module::new("AAC442", COMBAT, vec![float("Reduce", 0.62, 0.0, 1.0)])
}

/// What the game-tick handler did on a `GameTickEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityAac442TickOutcome {
    /// The delta movement is untouched.
    Untouched,
    /// `player.deltaMovement.multiply(factorX = reduce, factorZ = reduce)`.
    DeltaReduced { x: f64, z: f64 },
}

/// Upstream `repeatable`, a `handler` on `GameTickEvent`.
pub fn velocity_aac442_game_tick_handler(
    module: &Module,
    hurt_time: i32,
    on_ground: bool,
    delta_x: f64,
    delta_y: f64,
    delta_z: f64,
) -> VelocityAac442TickOutcome {
    let _ = delta_y;
    if hurt_time > 0 && !on_ground {
        let reduce = tb_setting_f64(module, "Reduce", 0.62);
        return VelocityAac442TickOutcome::DeltaReduced {
            x: delta_x * reduce,
            z: delta_z * reduce,
        };
    }

    VelocityAac442TickOutcome::Untouched
}

// ----- velocity/mode/VelocityBlocksMC20241123.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// BlocksMC velocity
// @author liquidsquid1

// Deprecated upstream: will be removed with the release of 0.43.0 and
// ported to LiquidBounce Legacy Add-on.

/// Upstream `VelocityMode("BlocksMC")`.
pub const VELOCITY_BLOCKSMC_NAME: &str = "BlocksMC";

pub fn velocity_blocksmc() -> Module {
    Module::new("BlocksMC", COMBAT, vec![])
}

/// What the packet handler did with a `PacketEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityBlocksMcPacketOutcome {
    /// Not a local `ClientboundSetEntityMotionPacket`: the event is untouched.
    Untouched,
    /// The velocity was cancelled, then the 1.21.5 start-sneaking and
    /// stop-sneaking packets went out in that order.
    CancelledAndSneaked,
}

/// Upstream `packetHandler`, a `handler` on `PacketEvent`. The
/// `send1_21_5StartSneaking()` and `send1_21_5StopSneaking()` network sends
/// are outcomes for the host to send in order.
pub fn velocity_blocksmc_packet_handler(
    is_set_entity_motion: bool,
    packet_id_matches_player: bool,
) -> VelocityBlocksMcPacketOutcome {
    // Check if this is a regular velocity update
    if is_set_entity_motion && packet_id_matches_player {
        // event.cancelEvent()
        // network.send1_21_5StartSneaking()
        // network.send1_21_5StopSneaking()
        return VelocityBlocksMcPacketOutcome::CancelledAndSneaked;
    }

    VelocityBlocksMcPacketOutcome::Untouched
}

#[cfg(test)]
mod velocity_aac442_blocksmc_tests {
    use super::*;

    #[test]
    fn velocity_aac442_reduces_the_air_hurt_delta() {
        let module = velocity_aac442();

        // hurtTime > 0 but on the ground: untouched
        assert_eq!(
            velocity_aac442_game_tick_handler(&module, 5, true, 0.5, 0.1, 0.9),
            VelocityAac442TickOutcome::Untouched
        );

        // In the air with no hurt time: untouched
        assert_eq!(
            velocity_aac442_game_tick_handler(&module, 0, false, 0.5, 0.1, 0.9),
            VelocityAac442TickOutcome::Untouched
        );

        // In the air hurt by the 0.62 factor
        assert_eq!(
            velocity_aac442_game_tick_handler(&module, 5, false, 0.5, 0.1, 0.9),
            VelocityAac442TickOutcome::DeltaReduced { x: 0.31, z: 0.558 }
        );

        // A raised Reduce setting scales further
        let mut module = velocity_aac442();
        module.setting_mut("Reduce").unwrap().kind = ValueKind::Float(1.0, 0.0, 1.0);
        assert_eq!(
            velocity_aac442_game_tick_handler(&module, 5, false, 0.5, 0.1, 0.9),
            VelocityAac442TickOutcome::DeltaReduced { x: 0.5, z: 0.9 }
        );
    }

    #[test]
    fn velocity_blocksmc_cancels_the_local_velocity_and_sneaks() {
        // Not a SetEntityMotion, or another entity's: untouched
        assert_eq!(
            velocity_blocksmc_packet_handler(false, true),
            VelocityBlocksMcPacketOutcome::Untouched
        );
        assert_eq!(
            velocity_blocksmc_packet_handler(true, false),
            VelocityBlocksMcPacketOutcome::Untouched
        );

        // The local velocity: cancelled, start- then stop-sneaking sent
        assert_eq!(
            velocity_blocksmc_packet_handler(true, true),
            VelocityBlocksMcPacketOutcome::CancelledAndSneaked
        );
    }
}

// ----- velocity/mode/VelocityDexland20241001.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----

// Deprecated upstream: will be removed with the release of 0.43.0 and
// ported to LiquidBounce Legacy Add-on.

/// Upstream `VelocityMode("Dexland")`.
pub const VELOCITY_DEXLAND_NAME: &str = "Dexland";

pub fn velocity_dexland() -> Module {
    Module::new(
        "Dexland",
        COMBAT,
        vec![
            float("HReduce", 0.3, 0.0, 1.0),
            int("AttacksToWork", 4, 1, 10),
        ],
    )
}

/// Upstream `VelocityDexland20241001.lastAttackTime` and `count` (count is a
/// public var upstream).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityDexlandState {
    pub last_attack_time: u64,
    pub count: i64,
}

/// What the attack handler did on an `AttackEntityEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityDexlandAttackOutcome {
    /// The delta movement is untouched.
    Untouched,
    /// `player.deltaMovement.multiply(factorX = hReduce, factorZ = hReduce)`.
    DeltaReduced { x: f64, z: f64 },
}

/// Upstream `attackHandler`, a `handler` on `AttackEntityEvent`. The count
/// increments on every attack before the modulo check.
pub fn velocity_dexland_attack_handler(
    module: &Module,
    state: &mut VelocityDexlandState,
    now_ms: u64,
    hurt_time: i32,
    delta_x: f64,
    delta_y: f64,
    delta_z: f64,
) -> VelocityDexlandAttackOutcome {
    let _ = delta_y;
    state.count += 1;

    let mut outcome = VelocityDexlandAttackOutcome::Untouched;
    if hurt_time > 0
        && state.count % tb_setting_i64(module, "AttacksToWork", 4) == 0
        && now_ms.saturating_sub(state.last_attack_time) <= 8000
    {
        let reduce = tb_setting_f64(module, "HReduce", 0.3);
        outcome = VelocityDexlandAttackOutcome::DeltaReduced {
            x: delta_x * reduce,
            z: delta_z * reduce,
        };
    }

    state.last_attack_time = now_ms;
    outcome
}

// ----- velocity/mode/VelocityHylex20241117.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Hylex Velocity
//
// Works because of a silly exemption from Hylex
// @author @liquidsquid1

// Deprecated upstream: will be removed with the release of 0.43.0 and
// ported to LiquidBounce Legacy Add-on.

/// Upstream `VelocityMode("Hylex")`.
pub const VELOCITY_HYLEX_NAME: &str = "Hylex";

pub fn velocity_hylex() -> Module {
    Module::new("Hylex", COMBAT, vec![])
}

/// What the attack handler did on an `AttackEntityEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VelocityHylexAttackOutcome {
    /// The delta movement is untouched (not moving, not sprinting, or a
    /// hurtTime with no entry in the factor table).
    Untouched,
    /// The hurtTime-specific x/z factor applied to the delta.
    DeltaReduced { x: f64, z: f64 },
}

/// Upstream `attackHandler`, a `handler` on `AttackEntityEvent`: a
/// hurtTime-keyed factor table (9 -> 0.8, 8 -> 0.11, 7 -> 0.4, 4 -> 0.37)
/// applied to x and z only.
pub fn velocity_hylex_attack_handler(
    is_moving: bool,
    is_sprinting: bool,
    hurt_time: i32,
    delta_x: f64,
    delta_y: f64,
    delta_z: f64,
) -> VelocityHylexAttackOutcome {
    if !is_moving || !is_sprinting {
        return VelocityHylexAttackOutcome::Untouched;
    }

    let factor: f64 = match hurt_time {
        9 => 0.8,
        8 => 0.11,
        7 => 0.4,
        4 => 0.37,
        _ => return VelocityHylexAttackOutcome::Untouched,
    };

    VelocityHylexAttackOutcome::DeltaReduced {
        x: delta_x * factor,
        z: delta_z * factor,
    }
}

/// Upstream `repeatable`, a `handler` on `GameTickEvent`: jump from the
/// ground while the hurt time is above 5. Returns whether
/// `player.jumpFromGround()` ran.
pub fn velocity_hylex_game_tick_handler(hurt_time: i32, on_ground: bool) -> bool {
    let should_jump = hurt_time > 5;
    let can_jump = on_ground;

    should_jump && can_jump
}

#[cfg(test)]
mod velocity_dexland_hylex_tests {
    use super::*;

    #[test]
    fn velocity_dexland_reduces_every_n_th_attack_in_the_hurt_and_time_windows() {
        let module = velocity_dexland();
        let mut state = VelocityDexlandState::default();

        // Attacks 1..3 (hurt or not): untouched, count still counts up
        for _ in 0..3 {
            assert_eq!(
                velocity_dexland_attack_handler(&module, &mut state, 1000, 6, 1.0, 0.1, 0.9),
                VelocityDexlandAttackOutcome::Untouched
            );
        }

        // Attack 4 (count % 4 == 0), hurt, within 8000 ms of the first:
        // reduced by the 0.3 factor
        assert_eq!(
            velocity_dexland_attack_handler(&module, &mut state, 1000, 6, 1.0, 0.1, 0.9),
            VelocityDexlandAttackOutcome::DeltaReduced { x: 0.3, z: 0.27 }
        );
        assert_eq!(state.count, 4);

        // Attack 5 without hurt time: untouched, window re-arms
        assert_eq!(
            velocity_dexland_attack_handler(&module, &mut state, 1500, 0, 1.0, 0.1, 0.9),
            VelocityDexlandAttackOutcome::Untouched
        );

        // Attack 6 beyond the 8000 ms window: untouched
        assert_eq!(
            velocity_dexland_attack_handler(&module, &mut state, 20000, 6, 1.0, 0.1, 0.9),
            VelocityDexlandAttackOutcome::Untouched
        );
        assert_eq!(state.count, 6);
        assert_eq!(state.last_attack_time, 20000);
    }

    #[test]
    fn velocity_hylex_reduces_by_the_hurt_time_factor_table() {
        // Not moving or not sprinting short-circuits every hurt time
        assert_eq!(
            velocity_hylex_attack_handler(false, true, 9, 1.0, 0.1, 1.0),
            VelocityHylexAttackOutcome::Untouched
        );
        assert_eq!(
            velocity_hylex_attack_handler(true, false, 9, 1.0, 0.1, 1.0),
            VelocityHylexAttackOutcome::Untouched
        );

        // The hurtTime-keyed factors apply to x and z only
        assert_eq!(
            velocity_hylex_attack_handler(true, true, 9, 1.0, 0.1, 1.0),
            VelocityHylexAttackOutcome::DeltaReduced { x: 0.8, z: 0.8 }
        );
        assert_eq!(
            velocity_hylex_attack_handler(true, true, 8, 1.0, 0.1, 1.0),
            VelocityHylexAttackOutcome::DeltaReduced { x: 0.11, z: 0.11 }
        );
        assert_eq!(
            velocity_hylex_attack_handler(true, true, 7, 1.0, 0.1, 1.0),
            VelocityHylexAttackOutcome::DeltaReduced { x: 0.4, z: 0.4 }
        );
        assert_eq!(
            velocity_hylex_attack_handler(true, true, 4, 1.0, 0.1, 1.0),
            VelocityHylexAttackOutcome::DeltaReduced { x: 0.37, z: 0.37 }
        );

        // Hurt times without a table entry leave the delta alone
        assert_eq!(
            velocity_hylex_attack_handler(true, true, 5, 1.0, 0.1, 1.0),
            VelocityHylexAttackOutcome::Untouched
        );
    }

    #[test]
    fn velocity_hylex_jumps_on_the_ground_above_hurt_time_five() {
        // hurtTime 6+ on the ground: jumpFromGround
        assert!(velocity_hylex_game_tick_handler(6, true));
        assert!(velocity_hylex_game_tick_handler(9, true));

        // hurtTime 5 or below: no jump
        assert!(!velocity_hylex_game_tick_handler(5, true));
        assert!(!velocity_hylex_game_tick_handler(0, true));

        // In the air: no jump
        assert!(!velocity_hylex_game_tick_handler(9, false));
    }
}


// ----- combat/spearkill/ModuleSpearKill.kt + SpearKillTargetFinder.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// Upstream `ModuleSpearKill("SpearKill", aliases = listOf("AutoSpear"))`.

/// Upstream `Color4b.RED` / `Color4b.WHITE` and `Color4b.alpha(a)` for the
/// Preview group's default colors.
impl TbColor {
    pub fn red() -> Self {
        TbColor { r: 0xFF, g: 0x00, b: 0x00, a: 0xFF }
    }

    pub fn white() -> Self {
        TbColor { r: 0xFF, g: 0xFF, b: 0xFF, a: 0xFF }
    }

    pub fn alpha(&self, a: u8) -> Self {
        TbColor { r: self.r, g: self.g, b: self.b, a }
    }
}

pub const SPEAR_KILL_NAME: &str = "SpearKill";

pub fn spear_kill() -> Module {
    Module::new(
        "SpearKill",
        COMBAT,
        vec![
            float("MaxTargetDistance", 50.0, 3.0, 200.0),
            float("MaxSpeed", 7.0, 2.0, 10.0),
            // tree(Preview): ToggleableValueGroup("Preview", true) with the two
            // color children, flattened onto the module's settings.
            boolean("Preview", true),
            crate::value::color("PreviewFillColor", 0xFF, 0x00, 0x00, 67), // Color4b.RED.alpha(67)
            crate::value::color("PreviewOutlineColor", 0xFF, 0xFF, 0xFF, 167), // Color4b.WHITE.alpha(167)
        ],
    )
}

/// Upstream `net.minecraft.world.item.component.KineticWeapon` as a seam: the
/// two values the module reads off the held item's data component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KineticWeaponParams {
    pub delay_ticks: i32,
    pub damage_use_duration: i32,
}

/// Upstream `KineticWeapon.isChargeAttackActive`: the damage window opens at
/// `delayTicks` and ends at `computeDamageUseDuration()`.
pub fn kinetic_weapon_is_charge_attack_active(
    weapon: &KineticWeaponParams,
    ticks_using_item: i32,
) -> bool {
    ticks_using_item < weapon.damage_use_duration
}

/// Upstream `KineticWeapon.hasChargeStarted`.
pub fn kinetic_weapon_has_charge_started(
    weapon: &KineticWeaponParams,
    ticks_using_item: i32,
) -> bool {
    ticks_using_item >= weapon.delay_ticks
}

/// Upstream `KineticWeapon.isChargeSpent`.
pub fn kinetic_weapon_is_charge_spent(
    weapon: &KineticWeaponParams,
    ticks_using_item: i32,
) -> bool {
    ticks_using_item > weapon.damage_use_duration
}

fn sk_add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sk_sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn sk_dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sk_scale3(a: [f64; 3], f: f64) -> [f64; 3] {
    [a[0] * f, a[1] * f, a[2] * f]
}

fn sk_len_sqr3(a: [f64; 3]) -> f64 {
    sk_dot3(a, a)
}

fn sk_dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    sk_len_sqr3(sk_sub3(a, b)).sqrt()
}

/// Upstream `Vec3.normalize`: a zero-length vector stays zero.
fn sk_norm3(a: [f64; 3]) -> [f64; 3] {
    let len = sk_len_sqr3(a).sqrt();
    if len == 0.0 {
        [0.0; 3]
    } else {
        sk_scale3(a, 1.0 / len)
    }
}

/// Upstream `Vec3.closerThan(other, max)`.
fn sk_closer_than(a: [f64; 3], b: [f64; 3], max: f64) -> bool {
    sk_dist3(a, b) < max
}

/// Upstream `AABB.clip(from, to)`: the first intersection of the segment with
/// the box surface, `None` when the segment misses (`HitResult.MISS`).
pub fn sk_aabb_clip(min: [f64; 3], max: [f64; 3], from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]> {
    let d = sk_sub3(to, from);
    let mut tmin = 0.0f64;
    let mut tmax = 1.0f64;
    for i in 0..3 {
        if d[i].abs() < f64::EPSILON {
            if from[i] < min[i] || from[i] > max[i] {
                return None;
            }
        } else {
            let (mut t1, mut t2) = ((min[i] - from[i]) / d[i], (max[i] - from[i]) / d[i]);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    Some([
        from[0] + d[0] * tmin,
        from[1] + d[1] * tmin,
        from[2] + d[2] * tmin,
    ])
}

/// Upstream `AABB.expandTowards(d)`: grows the box toward `d`.
fn sk_expand_towards(min: [f64; 3], max: [f64; 3], d: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let (mut nmin, mut nmax) = (min, max);
    for i in 0..3 {
        if d[i] > 0.0 {
            nmax[i] += d[i];
        } else if d[i] < 0.0 {
            nmin[i] += d[i];
        }
    }
    (nmin, nmax)
}

/// Upstream `AABB.inflate(amount)`: grows every side.
fn sk_inflate_box(min: [f64; 3], max: [f64; 3], amount: f64) -> ([f64; 3], [f64; 3]) {
    (
        [min[0] - amount, min[1] - amount, min[2] - amount],
        [max[0] + amount, max[1] + amount, max[2] + amount],
    )
}

/// The slice of `player`/`mc` SpearKill reads.
pub struct SpearKillPlayer {
    /// `player.isUsingItem && player.useItem.isSpear` split at the seam.
    pub is_using_item: bool,
    pub use_item_is_spear: bool,
    pub ticks_using_item: i32,
    /// `player.usedItemHand`.
    pub used_item_hand: i32,
    pub eye_position: [f64; 3],
    pub look_angle: [f64; 3],
    pub delta_movement: [f64; 3],
    /// `player.boundingBox`.
    pub bounding_min: [f64; 3],
    pub bounding_max: [f64; 3],
    pub bb_width: f64,
    /// `mc.options.keyUse.isDown`.
    pub key_use_down: bool,
    /// `mc.options.keyAttack.isDown`.
    pub key_attack_down: bool,
}

/// One `LivingEntity` candidate as SpearKill reads it.
#[derive(Clone)]
pub struct SpearKillEntity {
    /// Engine entity id; the port carries identity, never the entity object.
    pub id: u64,
    /// Upstream `it !== player`.
    pub is_self: bool,
    pub is_alive: bool,
    /// Upstream `isWithinWorldBorder`.
    pub within_world_border: bool,
    /// Upstream `it.boundingBox`.
    pub bounding_min: [f64; 3],
    pub bounding_max: [f64; 3],
    /// Upstream `PositionExtrapolation.getBestForEntity(target)` picks the
    /// simulation variant for players.
    pub is_player: bool,
    /// Upstream `entity.position()` / `entity.lastPos`, the linear
    /// extrapolation inputs.
    pub position: [f64; 3],
    pub last_pos: [f64; 3],
    /// Upstream `PlayerSimulationExtrapolation`:
    /// `simulation.getSnapshotAt(ticks).pos`, an engine seam.
    pub simulation_snapshot_at: Option<std::sync::Arc<dyn Fn(i32) -> [f64; 3]>>,
    /// Upstream `target is EnderDragon`: the sub-entity boxes the preview
    /// draws instead of the main one.
    pub dragon_sub_boxes: Vec<([f64; 3], [f64; 3])>,
}

/// Upstream `PositionExtrapolation` (utils/entity/PositionExtrapolation.kt):
/// predicts a position n ticks ahead.
pub enum SpearKillExtrapolation {
    /// Upstream `LinearPositionExtrapolation`: assumes a constant per-tick
    /// speed (`basePosition.fma(ticks, velocity)`).
    Linear { base_position: [f64; 3], velocity: [f64; 3] },
    /// Upstream `PlayerSimulationExtrapolation`: the SimulatedPlayerCache
    /// lookup is an engine seam.
    PlayerSimulation(std::sync::Arc<dyn Fn(i32) -> [f64; 3]>),
}

impl SpearKillExtrapolation {
    pub fn position_in_ticks(&self, ticks: f64) -> [f64; 3] {
        match self {
            SpearKillExtrapolation::Linear { base_position, velocity } => {
                sk_add3(*base_position, sk_scale3(*velocity, ticks))
            }
            SpearKillExtrapolation::PlayerSimulation(snapshot_at) => {
                let ticks = ((ticks.min(30.0)).round() as i32).max(0);
                snapshot_at(ticks)
            }
        }
    }
}

/// Upstream `PositionExtrapolation.getBestForEntity(target)`.
pub fn spear_kill_extrapolation_for_entity(entity: &SpearKillEntity) -> SpearKillExtrapolation {
    if entity.is_player {
        if let Some(snapshot_at) = &entity.simulation_snapshot_at {
            return SpearKillExtrapolation::PlayerSimulation(std::sync::Arc::clone(snapshot_at));
        }
    }
    SpearKillExtrapolation::Linear {
        base_position: entity.position,
        velocity: sk_sub3(entity.position, entity.last_pos),
    }
}

/// The world seams the SpearKill target finder reads.
pub struct SpearKillWorld<'a> {
    /// Upstream `world.getEntitiesOfClass(LivingEntity, box)`: entities in the
    /// search box the port computes.
    pub entities: Box<dyn Fn([f64; 3], [f64; 3]) -> Vec<SpearKillEntity> + 'a>,
    /// Upstream `hasLineOfSight(eye, hitPosition)`.
    pub has_line_of_sight: Box<dyn Fn([f64; 3], [f64; 3]) -> bool + 'a>,
    /// Upstream `traceFromPlayer(range, block = ClipContext.Block.COLLIDER)`:
    /// `Some(hit location)` when the ray hits anything (`type != MISS`), None
    /// on a miss.
    pub trace_from_player: Box<dyn Fn(f64) -> Option<[f64; 3]> + 'a>,
    /// Upstream `player.getAttackRangeWith(player.useItem)`: the AttackRange
    /// component's effective (min, max) ranges, engine math.
    pub effective_range: (f64, f64),
}

/// Upstream `SpearKillTargetFinder.lungeTarget()`: the distance from the eye a
/// lunge should leave the target at, the middle of the spear's damage window.
pub fn spear_kill_lunge_target(player: &SpearKillPlayer, effective_range: (f64, f64)) -> f64 {
    let along = sk_dot3(player.delta_movement, player.look_angle).max(0.0);
    let max = effective_range.1 + along;
    (effective_range.0 + max) / 2.0
}

/// Upstream `SpearKillTargetFinder.findTarget(maxTargetDistance)`: the closest
/// entity along the player's look ray that a lunge can bring into the spear's
/// damage window and that is not occluded by terrain. Returns the entity and
/// its distance to the damage window.
pub fn spear_kill_find_target(
    player: &SpearKillPlayer,
    world: &SpearKillWorld,
    max_target_distance: f64,
) -> Option<(SpearKillEntity, f64)> {
    let eye = player.eye_position;
    // `eye.fma(maxTargetDistance, lookAngle)` = eye + distance * look.
    let look_end = sk_add3(eye, sk_scale3(player.look_angle, max_target_distance));

    let search_box = {
        let (min, max) = sk_expand_towards(
            player.bounding_min,
            player.bounding_max,
            sk_sub3(look_end, eye),
        );
        sk_inflate_box(min, max, player.bb_width / 2.0)
    };
    let lunge_target = spear_kill_lunge_target(player, world.effective_range);

    let mut best: Option<(SpearKillEntity, f64, f64)> = None;
    for entity in (world.entities)(search_box.0, search_box.1) {
        if entity.is_self || !entity.is_alive || !entity.within_world_border {
            continue;
        }
        let hit = match sk_aabb_clip(entity.bounding_min, entity.bounding_max, eye, look_end) {
            Some(h) => h,
            None => continue,
        };

        // scoreCandidate
        if !(world.has_line_of_sight)(eye, hit) {
            continue;
        }

        let distance_to_target = sk_dist3(hit, eye);
        let distance_to_damage = (distance_to_target - lunge_target).max(0.0);
        if distance_to_damage <= 0.0 {
            continue;
        }

        let dist_sq = distance_to_damage * distance_to_damage;
        if let Some((_, _, best_sq)) = best {
            if dist_sq >= best_sq {
                continue;
            }
        }

        if let Some(hit_location) = (world.trace_from_player)(distance_to_damage) {
            if sk_closer_than(hit_location, eye, distance_to_damage) {
                continue;
            }
        }

        best = Some((entity, distance_to_damage, dist_sq));
    }
    best.map(|(entity, distance_to_damage, _)| (entity, distance_to_damage))
}

/// What the world render pass draws for the Preview group.
#[derive(Clone, Debug, PartialEq)]
pub enum SpearKillRenderOp {
    /// Upstream `drawBox(box, Preview.fillColor, Preview.outlineColor)`.
    DrawBox { min: [f64; 3], max: [f64; 3], fill: TbColor, outline: TbColor },
}

/// What a tick mutates in the engine.
#[derive(Default, Debug, PartialEq)]
pub struct SpearKillActions {
    /// Upstream `player.deltaMovement = ...`.
    pub set_delta_movement: Option<[f64; 3]>,
    /// Upstream `interaction.releaseUsingItem(player)`.
    pub released_using_item: bool,
    /// Upstream `useItem(hand)` with `player.usedItemHand`.
    pub reused_item_hand: Option<i32>,
}

/// Upstream `ModuleSpearKill` state: the queued lunge movements and the
/// previewed target.
pub struct SpearKill {
    pub module: Module,
    /// Upstream `attackMovements: ArrayDeque<Vec3>`.
    pub attack_movements: std::collections::VecDeque<[f64; 3]>,
    /// Upstream `previewTarget: LivingEntity?`, as the engine entity id.
    pub preview_target: Option<u64>,
}

fn sk_setting_color(module: &Module, name: &str, default: TbColor) -> TbColor {
    match module.setting(name).map(|v| &v.kind) {
        Some(crate::value::ValueKind::Color(r, g, b, a)) => TbColor { r: *r, g: *g, b: *b, a: *a },
        _ => default,
    }
}

impl SpearKill {
    fn preview_enabled(&self) -> bool {
        tb_setting_bool(&self.module, "Preview", true)
    }

    fn max_target_distance(&self) -> f64 {
        tb_setting_f64(&self.module, "MaxTargetDistance", 50.0)
    }

    fn max_allowed_speed(&self) -> f64 {
        tb_setting_f64(&self.module, "MaxSpeed", 7.0)
    }

    fn preview_fill_color(&self) -> TbColor {
        sk_setting_color(&self.module, "PreviewFillColor", TbColor::red().alpha(67))
    }

    fn preview_outline_color(&self) -> TbColor {
        sk_setting_color(&self.module, "PreviewOutlineColor", TbColor::white().alpha(167))
    }

    /// Upstream `currentMovement`: the head of the queue or `Vec3.ZERO`.
    pub fn current_movement(&self) -> [f64; 3] {
        self.attack_movements.front().copied().unwrap_or([0.0; 3])
    }

    /// Upstream `currentChargeAttackMovement` (internal val).
    pub fn current_charge_attack_movement(&self) -> Option<[f64; 3]> {
        let movement = self.current_movement();
        if sk_len_sqr3(movement) > 0.0 {
            Some(movement)
        } else {
            None
        }
    }

    /// Upstream `resetAttack()`.
    fn reset_attack(&mut self, actions: &mut SpearKillActions) {
        self.preview_target = None;
        if !self.attack_movements.is_empty() {
            actions.set_delta_movement = Some([0.0; 3]);
        }
        self.attack_movements.clear();
    }

    /// Upstream `createAttackMovement(target, distance)`.
    fn create_attack_movement(
        &mut self,
        player: &SpearKillPlayer,
        target: &SpearKillEntity,
        distance: f64,
    ) {
        let ticks = (((distance / self.max_allowed_speed()).ceil()) as i64).max(1);
        let velocity = distance / ticks as f64;

        let extrapolation = spear_kill_extrapolation_for_entity(target);
        let extrapolated = extrapolation.position_in_ticks(ticks as f64);
        let mut direction = sk_norm3(sk_sub3(extrapolated, player.eye_position));
        if sk_len_sqr3(direction) == 0.0 {
            direction = sk_norm3(player.look_angle);
        }

        let movement = sk_scale3(direction, velocity);
        let reverse = sk_scale3(movement, -1.0);

        for _ in 0..ticks {
            self.attack_movements.push_back(movement);
        }
        for _ in 0..ticks {
            self.attack_movements.push_back(reverse);
        }
        self.attack_movements.push_back([0.0; 3]);
    }

    /// Upstream `tickHandler`, a `handler<GameTickEvent>` with
    /// `EventPriorityConvention.FINAL_DECISION`. `weapon` is
    /// `player.useItem.get(DataComponents.KINETIC_WEAPON)`.
    pub fn on_tick(
        &mut self,
        player: &SpearKillPlayer,
        weapon: Option<&KineticWeaponParams>,
        world: &SpearKillWorld,
        actions: &mut SpearKillActions,
    ) {
        let is_using_spear = player.is_using_item && player.use_item_is_spear;
        if !is_using_spear {
            self.reset_attack(actions);
            return;
        }

        let target = if self.preview_enabled() {
            spear_kill_find_target(player, world, self.max_target_distance())
        } else {
            None
        };
        self.preview_target = target.as_ref().map(|(entity, _)| entity.id);

        let spear = match weapon {
            Some(w) => *w,
            None => {
                self.reset_attack(actions);
                return;
            }
        };

        // Restarting while a lunge is in flight would reset the charge, clearing the lunge and dropping its
        // return leg, so the player would be left where the lunge took them.
        if player.key_use_down
            && kinetic_weapon_is_charge_spent(&spear, player.ticks_using_item)
            && self.attack_movements.is_empty()
        {
            actions.released_using_item = true;
            actions.reused_item_hand = Some(player.used_item_hand);
        }

        if !kinetic_weapon_has_charge_started(&spear, player.ticks_using_item) {
            self.attack_movements.clear();
            return;
        }

        if let Some(next) = self.attack_movements.pop_front() {
            actions.set_delta_movement = Some(next);
            return;
        }

        if !kinetic_weapon_is_charge_attack_active(&spear, player.ticks_using_item)
            || !player.key_attack_down
        {
            return;
        }

        let (entity, distance) = match target {
            Some(found) => found,
            None => match spear_kill_find_target(player, world, self.max_target_distance()) {
                Some(found) => found,
                None => return,
            },
        };
        self.preview_target = Some(entity.id);
        self.create_attack_movement(player, &entity, distance);
    }
}

/// Upstream `renderHandler`, a `handler<WorldRenderEvent>`: draw the preview
/// target's box (or an EnderDragon's sub-entity boxes) with the Preview
/// colors. `entities` is the current world entity list.
pub fn spear_kill_render_handler(
    state: &SpearKill,
    entities: &[SpearKillEntity],
) -> Vec<SpearKillRenderOp> {
    if !state.preview_enabled() {
        return Vec::new();
    }
    let fill = state.preview_fill_color();
    let outline = state.preview_outline_color();
    let mut ops = Vec::new();
    if let Some(id) = state.preview_target {
        if let Some(target) = entities.iter().find(|e| e.id == id) {
            if target.dragon_sub_boxes.is_empty() {
                ops.push(SpearKillRenderOp::DrawBox {
                    min: target.bounding_min,
                    max: target.bounding_max,
                    fill,
                    outline,
                });
            } else {
                for (min, max) in &target.dragon_sub_boxes {
                    ops.push(SpearKillRenderOp::DrawBox { min: *min, max: *max, fill, outline });
                }
            }
        }
    }
    ops
}

#[cfg(test)]
mod spear_kill_tests {
    use super::*;

    fn entity(id: u64, min: [f64; 3], max: [f64; 3]) -> SpearKillEntity {
        SpearKillEntity {
            id,
            is_self: false,
            is_alive: true,
            within_world_border: true,
            bounding_min: min,
            bounding_max: max,
            is_player: false,
            position: min,
            last_pos: min,
            simulation_snapshot_at: None,
            dragon_sub_boxes: Vec::new(),
        }
    }

    fn player_at(eye: [f64; 3]) -> SpearKillPlayer {
        SpearKillPlayer {
            is_using_item: true,
            use_item_is_spear: true,
            ticks_using_item: 3,
            used_item_hand: 0,
            eye_position: eye,
            look_angle: [1.0, 0.0, 0.0],
            delta_movement: [0.0; 3],
            bounding_min: eye,
            bounding_max: [eye[0] + 0.6, eye[1] + 1.8, eye[2] + 0.6],
            bb_width: 0.6,
            key_use_down: false,
            key_attack_down: false,
        }
    }

    fn plain_world(entities: Vec<SpearKillEntity>) -> SpearKillWorld<'static> {
        SpearKillWorld {
            entities: Box::new(move |_, _| entities.clone()),
            has_line_of_sight: Box::new(|_, _| true),
            trace_from_player: Box::new(|_| None),
            effective_range: (1.0, 3.0),
        }
    }

    #[test]
    fn spear_kill_extrapolation_linear_and_simulation() {
        let linear = SpearKillExtrapolation::Linear {
            base_position: [1.0, 2.0, 3.0],
            velocity: [0.5, 0.0, -0.5],
        };
        assert_eq!(linear.position_in_ticks(4.0), [3.0, 2.0, 1.0]);

        let sim = SpearKillExtrapolation::PlayerSimulation(std::sync::Arc::new(|t: i32| {
            assert!((0..=30).contains(&t));
            [t as f64, 0.0, 0.0]
        }));
        // Clamped to 30 and rounded.
        assert_eq!(sim.position_in_ticks(100.5), [30.0, 0.0, 0.0]);
        assert_eq!(sim.position_in_ticks(2.4), [2.0, 0.0, 0.0]);
    }

    #[test]
    fn spear_kill_find_target_scores_along_the_look_ray() {
        // Eye at the origin looking down +x. A near target, a far target, and
        // one behind terrain cover that LOS rejects.
        let player = player_at([0.0, 64.0, 0.0]);
        let near = entity(1, [7.9, 63.9, -0.4], [8.9, 64.9, 0.4]);
        let far = entity(2, [19.9, 63.9, -0.4], [20.9, 64.9, 0.4]);
        let blocked = entity(3, [11.9, 63.9, -0.4], [12.9, 64.9, 0.4]);

        let world = SpearKillWorld {
            entities: Box::new(move |_, _| vec![near.clone(), far.clone(), blocked.clone()]),
            has_line_of_sight: Box::new(|_, hit| !(11.0..=13.0).contains(&hit[0])),
            trace_from_player: Box::new(|_| None),
            effective_range: (1.0, 3.0),
        };

        // lungeTarget = (1 + (3 + 0)) / 2 = 2.0, so distanceToDamage = hit.x - 2.
        let (found, distance) = spear_kill_find_target(&player, &world, 50.0).unwrap();
        assert_eq!(found.id, 1);
        assert!((distance - 5.9).abs() < 1e-9);
    }

    #[test]
    fn spear_kill_find_target_skips_occluded_and_out_of_damage_window() {
        let player = player_at([0.0, 64.0, 0.0]);
        // Terrain blocks the lunge path closer than the damage distance.
        let covered = entity(1, [7.9, 63.9, -0.4], [8.9, 64.9, 0.4]);
        let world = SpearKillWorld {
            entities: Box::new(move |_, _| vec![covered.clone()]),
            has_line_of_sight: Box::new(|_, _| true),
            trace_from_player: Box::new(|range| Some([range * 0.5, 64.0, 0.0])),
            effective_range: (1.0, 3.0),
        };
        assert!(spear_kill_find_target(&player, &world, 50.0).is_none());
    }

    #[test]
    fn spear_kill_tick_drains_the_lunge_legs_in_order() {
        let mut state = SpearKill {
            module: spear_kill(),
            attack_movements: Default::default(),
            preview_target: None,
        };
        let mut player = player_at([0.0, 64.0, 0.0]);
        let world = plain_world(vec![entity(7, [7.9, 63.9, -0.4], [8.9, 64.9, 0.4])]);
        let weapon = KineticWeaponParams { delay_ticks: 2, damage_use_duration: 20 };

        // Charge active (3 < 20) and attack held: builds the lunge plan. The
        // distance-to-damage is ~5.9, so ticks = ceil(5.9/7) = 1.
        let mut actions = SpearKillActions::default();
        player.key_attack_down = true;
        state.on_tick(&player, Some(&weapon), &world, &mut actions);
        assert!(actions.set_delta_movement.is_none());
        assert_eq!(state.attack_movements.len(), 3);

        // Drain: one forward leg, one reverse leg, then the zero terminator.
        let forward = state.attack_movements[0];
        let reverse = state.attack_movements[1];
        // The return leg is exactly the negative of the forward leg.
        for i in 0..3 {
            assert!((reverse[i] + forward[i]).abs() < 1e-9);
        }
        assert!(forward[0] > 0.0);

        let mut drained: Vec<[f64; 3]> = Vec::new();
        for _ in 0..3 {
            actions = SpearKillActions::default();
            state.on_tick(&player, Some(&weapon), &world, &mut actions);
            drained.push(actions.set_delta_movement.unwrap());
        }
        assert_eq!(drained[2], [0.0; 3]);

        // Queue empty again and still attacking: a fresh lunge is planned.
        actions = SpearKillActions::default();
        state.on_tick(&player, Some(&weapon), &world, &mut actions);
        assert_eq!(state.attack_movements.len(), 3);
    }

    #[test]
    fn spear_kill_tick_resets_without_the_spear_and_restarts_a_spent_charge() {
        let mut state = SpearKill {
            module: spear_kill(),
            attack_movements: Default::default(),
            preview_target: None,
        };
        state.attack_movements.push_back([1.0, 0.0, 0.0]);
        state.preview_target = Some(3);
        let mut player = player_at([0.0, 64.0, 0.0]);
        player.is_using_item = false;
        let world = plain_world(Vec::new());
        let mut actions = SpearKillActions::default();
        state.on_tick(&player, None, &world, &mut actions);
        assert!(state.attack_movements.is_empty());
        assert_eq!(actions.set_delta_movement, Some([0.0; 3]));
        assert_eq!(state.preview_target, None);

        // Charge spent while keyUse is held and no lunge is in flight:
        // release and re-use the item hand.
        let mut player = player_at([0.0, 64.0, 0.0]);
        player.ticks_using_item = 21;
        player.key_use_down = true;
        let weapon = KineticWeaponParams { delay_ticks: 2, damage_use_duration: 20 };
        let mut actions = SpearKillActions::default();
        state.on_tick(&player, Some(&weapon), &world, &mut actions);
        assert!(actions.released_using_item);
        assert_eq!(actions.reused_item_hand, Some(0));
    }

    #[test]
    fn spear_kill_render_draws_the_preview_boxes() {
        let mut state = SpearKill {
            module: spear_kill(),
            attack_movements: Default::default(),
            preview_target: None,
        };
        let entities = vec![entity(7, [7.9, 63.9, -0.4], [8.9, 64.9, 0.4])];

        // No preview target yet: nothing drawn.
        assert!(spear_kill_render_handler(&state, &entities).is_empty());

        state.preview_target = Some(7);
        let ops = spear_kill_render_handler(&state, &entities);
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            SpearKillRenderOp::DrawBox {
                min: [7.9, 63.9, -0.4],
                max: [8.9, 64.9, 0.4],
                fill: TbColor { r: 0xFF, g: 0x00, b: 0x00, a: 67 },
                outline: TbColor { r: 0xFF, g: 0xFF, b: 0xFF, a: 167 },
            }
        );

        // A dragon draws its sub-entity boxes instead.
        let mut dragon = entity(7, [7.9, 63.9, -0.4], [8.9, 64.9, 0.4]);
        dragon.dragon_sub_boxes = vec![([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])];
        let dragon_entities = vec![dragon];
        let ops = spear_kill_render_handler(&state, &dragon_entities);
        assert_eq!(
            ops,
            vec![SpearKillRenderOp::DrawBox {
                min: [0.0, 0.0, 0.0],
                max: [1.0, 1.0, 1.0],
                fill: TbColor { r: 0xFF, g: 0x00, b: 0x00, a: 67 },
                outline: TbColor { r: 0xFF, g: 0xFF, b: 0xFF, a: 167 },
            }]
        );
    }
}

// ----- clicking/ClickPlan.kt + ClickTiming.kt + ConstantClickTiming.kt + HumanClickTiming.kt + ItemCooldown.kt + Clicker.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// The click scheduler under AutoClicker and TriggerBot. Presses are planned in
// milliseconds and batched into ticks the way `Minecraft.handleKeybinds`
// drains every click queued since the previous tick.

/// Upstream `java.util.Random` (the JDK 48-bit LCG plus the Box-Muller
/// gaussian), ported so the click timing draws the same stream a Java run
/// would.
#[derive(Clone, Debug)]
pub struct JdkRandom {
    seed: u64,
    have_next_next_gaussian: bool,
    next_next_gaussian: f64,
}

impl JdkRandom {
    pub fn new(seed: u64) -> Self {
        // Upstream constructor scrambles the seed once.
        let seed = (seed ^ 0x5DEECE66D) & ((1u64 << 48) - 1);
        JdkRandom { seed, have_next_next_gaussian: false, next_next_gaussian: 0.0 }
    }

    fn next_bits(&mut self, bits: u32) -> i64 {
        self.seed = self
            .seed
            .wrapping_mul(0x5DEECE66D)
            .wrapping_add(0xB)
            & ((1u64 << 48) - 1);
        let raw = (self.seed >> (48 - bits)) as i64;
        if bits < 32 {
            (raw << (32 - bits)) >> (32 - bits)
        } else {
            raw
        }
    }

    pub fn next_double(&mut self) -> f64 {
        ((self.next_bits(26) as f64) * 134217728.0 + self.next_bits(27) as f64)
            / 9007199254740992.0
    }

    pub fn next_gaussian(&mut self) -> f64 {
        if self.have_next_next_gaussian {
            self.have_next_next_gaussian = false;
            return self.next_next_gaussian;
        }
        loop {
            let v1 = 2.0 * self.next_double() - 1.0;
            let v2 = 2.0 * self.next_double() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s >= 1.0 || s == 0.0 {
                continue;
            }
            let multiplier = (-2.0 * s.ln() / s).sqrt();
            self.next_next_gaussian = v2 * multiplier;
            self.have_next_next_gaussian = true;
            return v1 * multiplier;
        }
    }
}

/// Upstream `ClickTechnique`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ClickTechnique {
    Human,
    /// Evenly spaced at the top of the CPS range, for anticheats that only
    /// look at the time since the last attack.
    Constant,
}

/// Upstream `ClickTiming`: source of the time between two presses.
pub trait ClickTiming {
    /// `recent`: intervals of the current combo in ms, oldest first;
    /// `combo_ms`: time from the combo start to the press the interval starts
    /// at. Returns ms until the next press.
    fn next_interval(
        &mut self,
        recent: &[i64],
        combo_ms: i64,
        cps: (i64, i64),
        random: &mut JdkRandom,
    ) -> i64;
}

/// Upstream `ConstantClickTiming`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ConstantClickTiming;

impl ClickTiming for ConstantClickTiming {
    fn next_interval(
        &mut self,
        _recent: &[i64],
        _combo_ms: i64,
        cps: (i64, i64),
        _random: &mut JdkRandom,
    ) -> i64 {
        (1000.0 / cps.1 as f64).round() as i64
    }
}

const HUMAN_SIGMA: f64 = 0.45;
const MIN_INTERVAL_MS: i64 = 10;
const MAX_INTERVAL_MS: i64 = 1000;

/// Upstream `HumanClickTiming`: log-normal intervals around a CPS drawn once
/// per combo.
#[derive(Clone, Debug, Default)]
pub struct HumanClickTiming {
    combo_cps: f64,
    range: Option<(i64, i64)>,
}

impl ClickTiming for HumanClickTiming {
    fn next_interval(
        &mut self,
        _recent: &[i64],
        combo_ms: i64,
        cps: (i64, i64),
        random: &mut JdkRandom,
    ) -> i64 {
        if combo_ms == 0 || Some(cps) != self.range {
            self.combo_cps = cps.0 as f64 + random.next_double() * (cps.1 - cps.0) as f64;
            self.range = Some(cps);
        }

        // mu shifted by -sigma^2/2 so the mean interval, not the median, lands on the target CPS
        let mu = (1000.0 / self.combo_cps).ln() - HUMAN_SIGMA * HUMAN_SIGMA / 2.0;
        ((mu + HUMAN_SIGMA * random.next_gaussian())
            .exp()
            .round() as i64)
            .clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS)
    }
}

pub const CLICK_PLAN_TICK_MS: i64 = 50;
const CLICK_PLAN_HORIZON_MS: i64 = 1000;
const CLICK_PLAN_IDLE_MS: i64 = 250;
const CLICK_PLAN_HISTORY: usize = 32;
const CLICK_PLAN_NONE: i64 = i64::MIN;

/// Upstream `ClickPlan`: press timestamps planned ahead in milliseconds and
/// batched into ticks. Tick `n` consumes the presses in
/// `(tickTime + (n - 1) * 50, tickTime + n * 50]`, so `clicks_at` is what that
/// tick will consume.
pub struct ClickPlan {
    /// Upstream `Clicker`'s timing closure, which picks the technique each
    /// call. Pushed by `Clicker.tick` each game tick.
    pub technique: ClickTechnique,
    /// Upstream `HumanClickTiming` instance owned by Clicker.
    human: HumanClickTiming,
    random: JdkRandom,
    cps: (i64, i64),
    max_per_tick: i32,
    /// Upstream `enforced: IntPredicate`; rebuilt by `Clicker.tick` each game
    /// tick over the cooldown seams.
    pub enforced: Option<Box<dyn Fn(i32) -> bool>>,
    pub tick_time: i64,
    times: Vec<i64>,
    intervals: Vec<i64>,
    next: usize,
    combo_start: i64,
    due: i32,
    consumed: i32,
    idle_since: i64,
}

impl ClickPlan {
    pub fn new(seed: u64) -> Self {
        ClickPlan {
            technique: ClickTechnique::Human,
            human: HumanClickTiming::default(),
            random: JdkRandom::new(seed),
            cps: (11, 14),
            max_per_tick: 2,
            enforced: None,
            tick_time: 0,
            times: Vec::new(),
            intervals: Vec::new(),
            next: 0,
            combo_start: 0,
            due: 0,
            consumed: 0,
            idle_since: CLICK_PLAN_NONE,
        }
    }

    /// Upstream `cps` setter: a change replans.
    pub fn set_cps(&mut self, cps: (i64, i64)) {
        if self.cps != cps {
            self.cps = cps;
            self.replan();
        }
    }

    /// Upstream `maxPerTick` setter: a change replans.
    pub fn set_max_per_tick(&mut self, max_per_tick: i32) {
        if self.max_per_tick != max_per_tick {
            self.max_per_tick = max_per_tick;
            self.replan();
        }
    }

    /// Upstream `comboMs`.
    pub fn combo_ms(&self) -> i64 {
        self.tick_time - self.combo_start
    }

    /// Upstream `reset(now)`: starts a new combo with a press at `now`.
    pub fn reset(&mut self, now: i64) {
        self.times.clear();
        self.intervals.clear();
        self.next = 0;
        self.due = 0;
        self.consumed = 0;
        self.idle_since = CLICK_PLAN_NONE;
        self.combo_start = now;
        self.tick_time = now - CLICK_PLAN_TICK_MS;
        self.times.push(now);
    }

    /// Upstream `tick(now)`: advances to the next tick and consumes the
    /// presses due at `now`. Presses nobody used for a while, or a gap in
    /// ticks, end the combo.
    pub fn tick(&mut self, now: i64) {
        if self.consumed > 0 {
            self.idle_since = CLICK_PLAN_NONE;
        } else if self.due > 0 && self.idle_since == CLICK_PLAN_NONE {
            self.idle_since = self.tick_time;
        }

        if self.times.is_empty()
            || now - self.tick_time > CLICK_PLAN_IDLE_MS
            || (self.idle_since != CLICK_PLAN_NONE && now - self.idle_since > CLICK_PLAN_IDLE_MS)
        {
            self.reset(now);
        }

        // Stay on the 50 ms grid so lookahead matches consumption, unless the real tick drifted off it
        let expected = self.tick_time + CLICK_PLAN_TICK_MS;
        self.tick_time = if (now - expected).abs() > CLICK_PLAN_TICK_MS {
            now
        } else {
            expected
        };

        self.plan_until(self.tick_time + CLICK_PLAN_HORIZON_MS);
        let mut count = 0;
        while self.next < self.times.len() && self.times[self.next] <= self.tick_time {
            self.next += 1;
            count += 1;
        }
        self.due = count.min(self.max_per_tick);
        self.consumed = 0;
        self.trim();
    }

    /// Upstream `clicksAt(tick)`: presses consumed by the tick `tick` ticks
    /// from now.
    pub fn clicks_at(&mut self, tick: i32) -> i32 {
        let planned = if tick <= 0 { self.due } else { self.planned_at(tick) };
        if planned == 0 {
            if let Some(enforced) = &self.enforced {
                if enforced(tick.max(0)) {
                    return 1;
                }
            }
        }
        planned
    }

    fn planned_at(&mut self, tick: i32) -> i32 {
        let from = self.tick_time + (tick as i64 - 1) * CLICK_PLAN_TICK_MS;
        let to = from + CLICK_PLAN_TICK_MS;
        self.plan_until(to);
        let mut count = 0;
        for i in self.next..self.times.len() {
            let time = self.times[i];
            if time > to {
                break;
            }
            if time > from {
                count += 1;
            }
        }
        (count as i32).min(self.max_per_tick)
    }

    /// Upstream `take()` (`@PublishedApi internal`): a press is available this
    /// tick.
    pub fn take(&mut self) -> bool {
        if self.consumed >= self.clicks_at(0) {
            return false;
        }
        self.consumed += 1;
        true
    }

    /// Upstream `consume(gate, block)`: uses up the presses of the current
    /// tick; a press failing `gate` is dropped, as vanilla drops presses while
    /// `missTime` is set. `Clicker.click` inlines this loop so the gate and
    /// the success side effects can touch the clicker's own state.
    pub fn consume(&mut self, gate: &mut dyn FnMut() -> bool, block: &mut dyn FnMut() -> bool) -> i32 {
        let mut clicks = 0;
        while self.take() {
            if gate() && block() {
                clicks += 1;
            }
        }
        clicks
    }

    /// Upstream `pressedByPlayer()`: the player pressed the key. The press
    /// takes the place of our next planned one, so the presses keep our rhythm
    /// and CPS instead of adding up.
    pub fn pressed_by_player(&mut self) {
        if self.next >= self.times.len() {
            return;
        }

        let interval = self.intervals.len() as i64 - (self.times.len() - self.next) as i64;
        if interval >= 0 {
            self.intervals.remove(interval as usize);
        }
        self.times.remove(self.next);
    }

    /// Upstream `pressedBy(leader, time)`: another clicker on the same key
    /// pressed at `time`. It counts as one of this tick's presses, and the
    /// presses after it follow the plan of `leader`: drawing our own would
    /// race it, and the earlier of two random presses comes faster than either
    /// plan's CPS.
    pub fn pressed_by(&mut self, leader: &ClickPlan, time: i64) {
        if self.times.is_empty() {
            return;
        }

        if self.consumed < self.due {
            self.consumed += 1;
        }

        self.replan();
        if time >= *self.times.last().expect("click plan has a combo press") {
            self.append(time);
        }
        self.next = self.times.len();

        for &planned in &leader.times[leader.next..] {
            if planned > time {
                self.append(planned);
            }
        }
    }

    /// Upstream `replan()`: drops the presses planned after this tick, so they
    /// are planned again with changed settings.
    fn replan(&mut self) {
        let keep = (self.next as i64).max(1) as usize;
        if self.times.len() <= keep {
            return;
        }
        let dropped = self.times.len() - keep;
        self.times.truncate(keep);
        let keep_intervals = self.intervals.len().saturating_sub(dropped);
        self.intervals.truncate(keep_intervals);
    }

    fn plan_until(&mut self, time: i64) {
        while *self.times.last().expect("click plan has a combo press") <= time {
            self.plan_next();
        }
    }

    fn plan_next(&mut self) {
        let last = *self.times.last().expect("click plan has a combo press");
        let interval = match self.technique {
            ClickTechnique::Human => {
                self.human
                    .next_interval(&self.intervals, last - self.combo_start, self.cps, &mut self.random)
            }
            ClickTechnique::Constant => {
                ConstantClickTiming
                    .next_interval(&self.intervals, last - self.combo_start, self.cps, &mut self.random)
            }
        };
        let mut time = last + interval;
        if self.times.len() >= self.max_per_tick as usize {
            time = time.max(self.times[self.times.len() - self.max_per_tick as usize] + CLICK_PLAN_TICK_MS);
        }

        self.append(time);
    }

    fn append(&mut self, time: i64) {
        self.intervals.push(time - *self.times.last().expect("click plan has a combo press"));
        if self.intervals.len() > CLICK_PLAN_HISTORY {
            self.intervals.remove(0);
        }
        self.times.push(time);
    }

    fn trim(&mut self) {
        let excess = self.next as i64 - CLICK_PLAN_HISTORY as i64;
        if excess > 0 {
            self.times.drain(0..excess as usize);
            self.next -= excess as usize;
        }
    }
}

/// Upstream `ItemCooldown` (ValueGroup "ItemCooldown", aliases "Cooldown"),
/// flattened: the `Minimum` floatRange lands on the parent Clicker's settings
/// and the cooldown progress is the engine seam.
pub struct ItemCooldown {
    pub next_cooldown: f64,
    /// Upstream `CombatManager.attackCooldownProgress(baseTime)` (the server's
    /// view of `Player.getAttackStrengthScale`), engine seam.
    pub attack_cooldown_progress: std::sync::Arc<dyn Fn(i32) -> f64>,
}

impl ItemCooldown {
    /// Upstream `isCooldownPassed(ticks)`.
    pub fn is_cooldown_passed(&self, ticks: i32) -> bool {
        (self.attack_cooldown_progress)(ticks) >= self.next_cooldown
    }
}

pub const CLICKER_TICKS_AHEAD: i32 = 20;

/// Upstream `Clicker` (an attack scheduler): presses are planned by a
/// `ClickTiming` and batched into ticks by a `ClickPlan`, so future clicks can
/// be predicted. Settings read the parent module under `prefix`.
pub struct Clicker {
    /// Upstream key binding identity: 0 = keyAttack, 1 = keyUse.
    pub key: u8,
    /// Upstream `keyBinding == mc.options.keyAttack`, which selects
    /// MissCooldown and the simulate-attack-key-down handler.
    pub is_attack_key: bool,
    /// Setting-name prefix inside the parent module ("Attack"/"Use" for the
    /// AutoClicker groups, empty for TriggerBot's own clicker).
    pub prefix: &'static str,
    /// Upstream `simulateAttackKeyDown`.
    pub simulate_attack_key_down: bool,
    /// Upstream `mc.missTime`, refreshed by the engine each tick.
    pub miss_time: i32,
    /// Upstream `player.hasCooldown`, refreshed by the engine each tick.
    pub player_has_cooldown: bool,
    /// Upstream `itemCooldown`: None mirrors `itemCooldown = null`.
    pub item_cooldown: Option<ItemCooldown>,
    /// Upstream `CombatManager.attackCooldownProgress(baseTime)`, engine seam.
    pub attack_cooldown_progress: std::sync::Arc<dyn Fn(i32) -> f64>,
    pub plan: ClickPlan,
    /// Upstream `clickAmount`: clicks executed by `click` in the current tick.
    pub click_amount: Option<i32>,
    pub ticks_since_last_click: i32,
    /// Upstream ValueGroup `running`.
    pub running: bool,
}

impl Clicker {
    #[allow(clippy::too_many_arguments, unused_variables)]
    pub fn new(
        key: u8,
        is_attack_key: bool,
        prefix: &'static str,
        simulate_attack_key_down: bool,
        max_cps: i64,
        seed: u64,
        with_item_cooldown: bool,
        attack_cooldown_progress: std::sync::Arc<dyn Fn(i32) -> f64>,
    ) -> Self {
        Clicker {
            key,
            is_attack_key,
            prefix,
            simulate_attack_key_down,
            miss_time: 0,
            player_has_cooldown: false,
            item_cooldown: if with_item_cooldown {
                Some(ItemCooldown { next_cooldown: 1.0, attack_cooldown_progress: std::sync::Arc::clone(&attack_cooldown_progress) })
            } else {
                None
            },
            attack_cooldown_progress,
            plan: ClickPlan::new(seed),
            click_amount: None,
            ticks_since_last_click: 0,
            running: false,
        }
    }

    fn prefixed(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name)
    }

    pub fn technique(&self, module: &Module) -> ClickTechnique {
        match module.setting(&self.prefixed("Technique")).and_then(|v| v.kind.choice()) {
            Some("Constant") => ClickTechnique::Constant,
            _ => ClickTechnique::Human,
        }
    }

    pub fn cps(&self, module: &Module) -> (i64, i64) {
        tb_setting_int_range(module, &self.prefixed("CPS"), (11, 14))
    }

    pub fn max_per_tick(&self, module: &Module) -> i32 {
        tb_setting_i64(module, &self.prefixed("MaxPerTick"), 2) as i32
    }

    fn miss_cooldown(&self, module: &Module) -> bool {
        tb_setting_bool(module, &self.prefixed("MissCooldown"), true)
    }

    /// Upstream `passesMissCooldown`.
    fn passes_miss_cooldown(&self, module: &Module) -> bool {
        !(self.is_attack_key && self.miss_cooldown(module) && self.miss_time > 0)
    }

    /// Upstream `isClickTick`.
    pub fn is_click_tick(&mut self) -> bool {
        self.will_click_at(0)
    }

    /// Upstream `ticksUntilClick`.
    pub fn ticks_until_click(&mut self) -> i32 {
        for tick in 0..CLICKER_TICKS_AHEAD {
            if self.plan.clicks_at(tick) > 0 {
                return tick;
            }
        }
        CLICKER_TICKS_AHEAD
    }

    /// Upstream `willClickAt(tick)`.
    pub fn will_click_at(&mut self, tick: i32) -> bool {
        self.plan.clicks_at(tick) > 0
    }

    /// Upstream `getClickAmount(tick)`.
    pub fn get_click_amount(&mut self, tick: i32) -> i32 {
        self.plan.clicks_at(tick)
    }

    /// Upstream `simulateAttackKeyDown`'s `KeybindIsPressedEvent` handler:
    /// simulate the click so the game handles the logic as if we clicked.
    pub fn simulated_key_is_pressed(&self) -> bool {
        self.click_amount.map_or(false, |clicks| clicks > 0)
    }

    /// Upstream `gameHandler` (FIRST_PRIORITY): per-tick plan maintenance.
    pub fn tick(&mut self, module: &Module, now_ms: i64) {
        self.ticks_since_last_click += 1;
        self.click_amount = None;

        self.plan.technique = self.technique(module);
        self.plan.set_cps(self.cps(module));
        self.plan.set_max_per_tick(self.max_per_tick(module));

        // Once, on the tick the cooldown fills up; one that is always ready would otherwise click every tick
        let progress = std::sync::Arc::clone(&self.attack_cooldown_progress);
        let has_cooldown = self.player_has_cooldown;
        let item_cooldown_next = self.item_cooldown.as_ref().map(|c| c.next_cooldown);
        self.plan.enforced = Some(Box::new(move |tick: i32| {
            if !has_cooldown {
                return false;
            }
            let Some(next) = item_cooldown_next else {
                return false;
            };
            progress(tick) >= next && !(progress(tick - 1) >= next)
        }));

        self.plan.tick(now_ms);
    }

    /// Upstream `click(block)`: uses the presses of this tick. If the cooldown
    /// is not passed, the press is dropped. `block` should return true if the
    /// click was successful; otherwise it will not count as a click.
    /// `press_others` is upstream `pressedForOthers`, wired by the engine's
    /// per-key clicker registry (`ClickerGroup.click`).
    pub fn click(
        &mut self,
        module: &Module,
        now_ms: i64,
        block: &mut dyn FnMut() -> bool,
        mut press_others: Option<&mut dyn FnMut(&ClickPlan, i64)>,
    ) -> i32 {
        let mut clicks = 0;
        while self.plan.take() {
            let gate = self.passes_miss_cooldown(module)
                && self.item_cooldown.as_ref().map(|c| c.is_cooldown_passed(0)) != Some(false);
            if gate && block() {
                if let Some(_cooldown) = self.item_cooldown.as_ref() {
                    // Upstream `itemCooldown?.newCooldown()` draws
                    // `minimumCooldown.random()` from the module's range.
                    let (min, max) = tb_setting_float_range(
                        module,
                        &self.prefixed("ItemCooldownMinimum"),
                        (1.0, 1.0),
                    );
                    let next = min + self.plan.random.next_double() * (max - min);
                    self.item_cooldown.as_mut().unwrap().next_cooldown = next;
                }
                self.ticks_since_last_click = 0;
                if let Some(press) = press_others.as_deref_mut() {
                    press(&self.plan, now_ms);
                }
                clicks += 1;
            }
        }

        self.click_amount = Some(self.click_amount.unwrap_or(0) + clicks);
        clicks
    }
}

/// Upstream `Clicker.clickers` companion registry: clickers grouped by key
/// binding, so presses on the same key by the player or other clickers count
/// as ours instead of adding up.
pub struct ClickerGroup {
    pub clickers: Vec<Clicker>,
}

impl ClickerGroup {
    pub fn new(clickers: Vec<Clicker>) -> Self {
        ClickerGroup { clickers }
    }

    /// Upstream `onPlayerPress` via the mouse/keyboard handlers: Minecraft
    /// only clicks keys while no screen is open.
    pub fn on_player_press(&mut self, key: u8, pressed: bool, screen_open: bool) {
        if !pressed || screen_open {
            return;
        }
        for clicker in self.clickers.iter_mut() {
            if clicker.key == key {
                clicker.plan.pressed_by_player();
            }
        }
    }

    /// Upstream per-clicker `gameHandler`s.
    pub fn tick(&mut self, module: &Module, now_ms: i64) {
        for clicker in self.clickers.iter_mut() {
            clicker.tick(module, now_ms);
        }
    }

    /// Upstream `click(block)` for clicker `index`, including
    /// `pressedForOthers`: other running clickers on the same key take the
    /// leader's presses instead of drawing their own.
    pub fn click(
        &mut self,
        index: usize,
        module: &Module,
        now_ms: i64,
        block: &mut dyn FnMut() -> bool,
    ) -> i32 {
        let key = self.clickers[index].key;
        let other_running: Vec<bool> = self
            .clickers
            .iter()
            .enumerate()
            .map(|(j, c)| j != index && c.key == key && c.running)
            .collect();
        let (head, rest) = self.clickers.split_at_mut(index);
        let (leader_slot, tail) = rest.split_at_mut(1);
        let mut others: Vec<&mut Clicker> = Vec::new();
        for (j, clicker) in head.iter_mut().enumerate() {
            if other_running[j] {
                others.push(clicker);
            }
        }
        for (j, clicker) in tail.iter_mut().enumerate() {
            if other_running[index + 1 + j] {
                others.push(clicker);
            }
        }
        let leader = &mut leader_slot[0];
        let mut press_others = |plan: &ClickPlan, t: i64| {
            for other in others.iter_mut() {
                other.plan.pressed_by(plan, t);
            }
        };
        leader.click(module, now_ms, block, Some(&mut press_others))
    }
}

fn tb_setting_str(module: &Module, name: &str, default: &str) -> String {
    module
        .setting(name)
        .and_then(|v| v.kind.choice())
        .unwrap_or(default)
        .to_string()
}

fn tb_setting_int_range(module: &Module, name: &str, default: (i64, i64)) -> (i64, i64) {
    module.setting(name).and_then(|v| v.kind.int_range()).unwrap_or(default)
}

fn tb_setting_float_range(module: &Module, name: &str, default: (f64, f64)) -> (f64, f64) {
    module.setting(name).and_then(|v| v.kind.float_range()).unwrap_or(default)
}

fn tb_setting_items(module: &Module, name: &str) -> Vec<String> {
    module.setting(name).and_then(|v| v.kind.items()).map(|items| items.to_vec()).unwrap_or_default()
}

// ----- combat/autoclicker/CrosshairAttacker.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----

/// Upstream `CriticalsSelectionMode` (ModuleCriticals.kt): how the attacker
/// treats critical hits.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CriticalsSelectionMode {
    Smart,
    Ignore,
    Always,
}

impl CriticalsSelectionMode {
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "Ignore" => CriticalsSelectionMode::Ignore,
            "Always" => CriticalsSelectionMode::Always,
            _ => CriticalsSelectionMode::Smart,
        }
    }

    /// Upstream `isCriticalHit()`: whether the next click will be (or counts
    /// as) a critical hit. The Criticals module state arrives as the two
    /// seams: `shouldWaitForCrit(ignoreState)` and `wouldDoCriticalHit()`.
    pub fn is_critical_hit(
        &self,
        should_wait_for_crit: &dyn Fn(bool) -> bool,
        would_do_critical_hit: &dyn Fn() -> bool,
    ) -> bool {
        match self {
            CriticalsSelectionMode::Ignore => true,
            CriticalsSelectionMode::Smart => !should_wait_for_crit(true),
            CriticalsSelectionMode::Always => would_do_critical_hit(),
        }
    }

    /// Upstream `shouldStopSprinting(clicker, target)`.
    pub fn should_stop_sprinting(
        &self,
        player_on_ground: bool,
        target_is_some: bool,
        clicker_will_click_at_1: bool,
    ) -> bool {
        // If we don't care about critical hits we don't have to stop sprinting.
        if *self == CriticalsSelectionMode::Ignore {
            return false;
        }

        // On ground, we cannot do critical hits anyway.
        if player_on_ground {
            return false;
        }

        // If we are about to do a critical hit, we should stop sprinting.
        target_is_some && clicker_will_click_at_1
    }
}

/// Upstream `Use` (CrosshairAttacker's OnItemUse setting).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CrosshairUse {
    Wait,
    Stop,
    Ignore,
}

/// The engine inputs and actions CrosshairAttacker's `attack()` reads and
/// performs.
pub struct CrosshairAttackerDeps<'a> {
    /// Upstream `mc.hitResult`: `Some(entity id)` on an entity hit, None
    /// otherwise.
    pub crosshair_entity: Option<u64>,
    /// Upstream `crosshair.entity.shouldBeAttacked()` (TriggerBot's
    /// wantsToAttack), engine seam.
    pub crosshair_should_be_attacked: bool,
    pub player_is_using_item: bool,
    pub now_ms: i64,
    /// Upstream `weapon.matchesAny(player.mainHandItem)`, engine seam given
    /// the module's Weapon multi-choice tags.
    pub weapon_matches: Box<dyn Fn(&[String]) -> bool + 'a>,
    /// Upstream `ModuleCriticals.CriticalsSelectionMode`'s seams.
    pub should_wait_for_crit: Box<dyn Fn(bool) -> bool + 'a>,
    pub would_do_critical_hit: Box<dyn Fn() -> bool + 'a>,
    /// Upstream `ModuleAutoWeapon.onTarget(entity)`.
    pub on_target: Box<dyn FnMut(u64) + 'a>,
    /// Upstream `interaction.releaseUsingItem(player)`.
    pub release_using_item: Box<dyn FnMut() + 'a>,
    /// Upstream the click block: `KeyMapping.click(mc.options.keyAttack.key)`.
    pub press_attack_key: Box<dyn FnMut() -> bool + 'a>,
}

/// Upstream `CrosshairAttacker<T>`: clicks the attack key on the crosshair
/// target for AutoClicker and TriggerBot. Its settings are added to `parent`,
/// after the ones `parent` declared before creating it.
pub struct CrosshairAttacker {
    /// Setting-name prefix inside the parent module ("Attack" for the
    /// AutoClicker group, empty for TriggerBot's own attacker).
    prefix: &'static str,
    /// Upstream `tickUntil { !player.isUsingItem }` in flight.
    waiting_item_use: bool,
    /// Upstream `waitTicks(delayPostStopUse)` in flight.
    post_stop_ticks_left: i32,
}

impl CrosshairAttacker {
    pub fn new(prefix: &'static str) -> Self {
        CrosshairAttacker { prefix, waiting_item_use: false, post_stop_ticks_left: 0 }
    }

    fn on_item_use(module: &Module, prefix: &str) -> CrosshairUse {
        match tb_setting_str(module, &format!("{}OnItemUse", prefix), "Wait").as_str() {
            "Stop" => CrosshairUse::Stop,
            "Ignore" => CrosshairUse::Ignore,
            _ => CrosshairUse::Wait,
        }
    }

    fn delay_post_stop_use(module: &Module, prefix: &str) -> i32 {
        tb_setting_i64(module, &format!("{}DelayPostStopUse", prefix), 0) as i32
    }

    fn weapon_tags(module: &Module, prefix: &str) -> Vec<String> {
        module
            .setting(&format!("{}Weapon", prefix))
            .map(|v| match &v.kind {
                crate::value::ValueKind::MultiChoice(selected, _) => selected.clone(),
                _ => Vec::new(),
            })
            .unwrap_or_else(|| vec!["Any".to_string()])
    }

    /// Upstream `isAttacking`.
    pub fn is_attacking(
        &self,
        module: &Module,
        deps: &CrosshairAttackerDeps,
        wants_to_attack: bool,
    ) -> bool {
        wants_to_attack && (deps.weapon_matches)(&Self::weapon_tags(module, self.prefix))
    }

    /// Upstream `attack()`: clicks as often as `clicker` plans for this tick,
    /// unless a critical hit is still pending. Returns false when the tick
    /// went to waiting for or stopping an item use instead.
    ///
    /// Suspend note: upstream suspends at `tickUntil`/`waitTicks` inside the
    /// per-tick coroutine; the port re-enters each tick and tracks the waits
    /// in state, so the click lands after the same total delay.
    pub fn attack(&mut self, module: &Module, deps: &mut CrosshairAttackerDeps, clicker: &mut Clicker) -> bool {
        // Resumed suspension: a post-stop delay from a previous tick.
        if self.post_stop_ticks_left > 0 {
            self.post_stop_ticks_left -= 1;
            return false;
        }

        // Resumed suspension: tickUntil { !player.isUsingItem }.
        if self.waiting_item_use {
            if deps.player_is_using_item {
                return false;
            }
            self.waiting_item_use = false;
            self.post_stop_ticks_left = Self::delay_post_stop_use(module, self.prefix);
            return false;
        }

        let crosshair_target = deps.crosshair_entity;
        if let Some(entity) = crosshair_target {
            (deps.on_target)(entity);

            let mode = CriticalsSelectionMode::from_tag(&tb_setting_str(module, &format!("{}Criticals", self.prefix), "Smart"));
            if !mode.is_critical_hit(&*deps.should_wait_for_crit, &*deps.would_do_critical_hit) {
                return true;
            }
        }

        if deps.player_is_using_item {
            match Self::on_item_use(module, self.prefix) {
                CrosshairUse::Wait => {
                    self.waiting_item_use = true;
                    return false;
                }
                CrosshairUse::Stop => {
                    (deps.release_using_item)();
                    self.post_stop_ticks_left = Self::delay_post_stop_use(module, self.prefix);
                    return false;
                }
                CrosshairUse::Ignore => {}
            }
        }

        clicker.click(module, deps.now_ms, &mut *deps.press_attack_key, None);
        true
    }
}

// ----- combat/autoclicker/ModuleAutoClicker.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// AutoClicker: clicks automatically when holding down a mouse button.

pub const AUTO_CLICKER_NAME: &str = "AutoClicker";

/// Upstream `SPECIAL_ITEMS_FOR_IGNORE`.
pub const AUTO_CLICKER_SPECIAL_ITEMS_FOR_IGNORE: [&str; 9] = [
    "minecraft:red_bed",
    "minecraft:player_head",
    "minecraft:compass",
    "minecraft:emerald",
    "minecraft:lapis_lazuli",
    "minecraft:green_dye",
    "minecraft:gray_dye",
    "minecraft:pink_dye",
    "minecraft:slime_ball",
];

/// Upstream `WeaponType` tags (utils/item/WeaponType.kt).
pub const WEAPON_TYPE_TAGS: [&str; 10] = [
    "Any",
    "Sword",
    "Axe",
    "Mace",
    "Spear",
    "Pickaxe",
    "Shovel",
    "Hoe",
    "Knockback",
    "FireAspect",
];

pub fn auto_clicker() -> Module {
    Module::new(
        "AutoClicker",
        COMBAT,
        vec![
            // tree(AttackButton): ToggleableValueGroup(this, "Attack", true)
            boolean("Attack", true),
            // Clicker(this, mc.options.keyAttack, simulateAttackKeyDown = true)
            choice("AttackTechnique", "Human", &["Human", "Constant"]),
            int_range("AttackCPS", 11, 14, 1, 30),
            int("AttackMaxPerTick", 2, 1, 5),
            boolean("AttackMissCooldown", true),
            float_range("AttackItemCooldownMinimum", 1.0, 1.0, 0.0, 2.0),
            boolean("AttackDelayOnBroken", true),
            // enumChoice("Objective", ObjectiveType.ANY)
            choice("AttackObjective", "Any", &["Enemy", "Entity", "Block", "Any"]),
            // CrosshairAttacker(this, clicker) { attack && isOnObjective() }
            choice("AttackOnItemUse", "Wait", &["Wait", "Stop", "Ignore"]),
            multi_choice("AttackWeapon", &["Any"], &WEAPON_TYPE_TAGS),
            choice("AttackCriticals", "Smart", &["Smart", "Ignore", "Always"]),
            int("AttackDelayPostStopUse", 0, 0, 20),
            // tree(UseButton): ToggleableValueGroup(this, "Use", false)
            boolean("Use", false),
            // Clicker(this, mc.options.keyUse, null): no ItemCooldown group.
            choice("UseTechnique", "Human", &["Human", "Constant"]),
            int_range("UseCPS", 11, 14, 1, 30),
            int("UseMaxPerTick", 2, 1, 5),
            items(
                "UseHoldingItemsForIgnore",
                &[
                    "minecraft:water_bucket",
                    "minecraft:lava_bucket",
                    "minecraft:ender_pearl",
                    "minecraft:ender_eye",
                    "minecraft:player_head",
                ],
            ),
            // blocks("UseBlocksForIgnore", ...): the engine registry supplies
            // the door/fence-gate/trapdoor ids (seam); the check ports.
            items("UseBlocksForIgnore", &[]),
            boolean("UseDelayStart", false),
            boolean("UseOnlyBlock", false),
            boolean("UseRequiresNoInput", false),
        ],
    )
}

/// Upstream `mc.hitResult` classification.
#[derive(Clone, Debug, PartialEq)]
pub enum CrosshairTarget {
    /// `EntityHitResult` with `shouldBeAttacked()` split at the seam.
    Entity { id: u64, should_be_attacked: bool },
    Block,
    None,
}

/// What a tick mutates in the engine.
#[derive(Default, Debug, PartialEq)]
pub struct AutoClickerOutcome {
    pub attack_clicks: i32,
    pub use_clicks: i32,
}

/// The engine inputs AutoClicker's tick handler reads.
pub struct AutoClickerTickDeps<'a> {
    /// Upstream `mc.options.keyAttack.isPressedOnAny`.
    pub attack_pressed: bool,
    /// Upstream `mc.options.keyUse.isPressedOnAny`.
    pub use_pressed: bool,
    /// Upstream `interaction.isDestroying`.
    pub is_destroying: bool,
    /// Upstream `mc.hitResult`.
    pub crosshair: CrosshairTarget,
    /// Upstream `player.mainHandItem` / `player.offhandItem` item ids.
    pub main_hand_item: &'a str,
    pub off_hand_item: &'a str,
    /// Upstream `mainHandStack.customName != null`.
    pub main_hand_custom_name: bool,
    /// Upstream `item is BlockItem`.
    pub main_hand_is_block_item: bool,
    pub off_hand_is_block_item: bool,
    /// Upstream `mc.level?.getBlockState(crosshair.blockPos)?.block` id.
    pub crosshair_block: Option<&'a str>,
    /// Upstream the UseButton click block:
    /// `KeyMapping.click(mc.options.keyUse.key)`.
    pub press_use_key: Box<dyn FnMut() -> bool + 'a>,
    /// Upstream `System.currentTimeMillis()`.
    pub now_ms: i64,
}

/// Upstream `ModuleAutoClicker` state.
pub struct AutoClicker {
    pub module: Module,
    /// Upstream `lastFinishBreak`.
    pub last_finish_break_ms: i64,
    /// Upstream `UseButton.needToWait`.
    pub need_to_wait: bool,
    /// Upstream `waitTicks(2)` for DelayStart.
    use_delay_ticks_left: i32,
    pub attack_clicker: Clicker,
    pub use_clicker: Clicker,
    pub attacker: CrosshairAttacker,
}

impl AutoClicker {
    pub fn new(seed: u64, attack_cooldown_progress: std::sync::Arc<dyn Fn(i32) -> f64>) -> Self {
        AutoClicker {
            module: auto_clicker(),
            last_finish_break_ms: 0,
            need_to_wait: true,
            use_delay_ticks_left: 0,
            attack_clicker: Clicker::new(
                0,
                true,
                "Attack",
                true,
                30,
                seed,
                true,
                std::sync::Arc::clone(&attack_cooldown_progress),
            ),
            use_clicker: Clicker::new(1, false, "Use", false, 30, seed ^ 0x9E3779B97F4A7C15, false, attack_cooldown_progress),
            attacker: CrosshairAttacker::new("Attack"),
        }
    }

    fn objective_type(&self) -> String {
        tb_setting_str(&self.module, "AttackObjective", "Any")
    }

    /// Upstream `AttackButton.isOnObjective()`.
    fn is_on_objective(&self, deps: &AutoClickerTickDeps) -> bool {
        match self.objective_type().as_str() {
            "Enemy" => matches!(&deps.crosshair, CrosshairTarget::Entity { should_be_attacked: true, .. }),
            "Entity" => matches!(deps.crosshair, CrosshairTarget::Entity { .. }),
            "Block" => deps.crosshair == CrosshairTarget::Block,
            _ => true,
        }
    }

    /// Upstream `packetHandler` for `ServerboundPlayerActionPacket`.
    pub fn on_player_action_packet(&mut self, action: &str, now_ms: i64) {
        if action == "STOP_DESTROY_BLOCK" {
            self.last_finish_break_ms = now_ms;
        }
    }

    /// Upstream `tickHandler`. `attacker_deps` wires the CrosshairAttacker
    /// seams (criticals, AutoWeapon, the attack click itself).
    pub fn on_tick(
        &mut self,
        deps: &mut AutoClickerTickDeps,
        attacker_deps: &mut CrosshairAttackerDeps,
    ) -> AutoClickerOutcome {
        let mut outcome = AutoClickerOutcome::default();

        // AttackButton.run
        if tb_setting_bool(&self.module, "Attack", true) {
            let wants_to_attack = deps.attack_pressed && self.is_on_objective(deps);
            let attacking = self.attacker.is_attacking(&self.module, attacker_deps, wants_to_attack);
            if attacking {
                // Check if the player is breaking a block, if so, return
                if !deps.is_destroying {
                    if !(deps.now_ms - self.last_finish_break_ms < 300
                        && tb_setting_bool(&self.module, "AttackDelayOnBroken", true))
                    {
                        // Upstream `if (!attacker.attack()) return@tickHandler`.
                        if !self.attacker.attack(&self.module, attacker_deps, &mut self.attack_clicker) {
                            return outcome;
                        }
                        outcome.attack_clicks = self.attack_clicker.click_amount.unwrap_or(0);
                    }
                }
            }
        }

        // UseButton.run
        if tb_setting_bool(&self.module, "Use", false) {
            let requires_no_input = tb_setting_bool(&self.module, "UseRequiresNoInput", false);
            if !(deps.use_pressed || requires_no_input) {
                self.need_to_wait = true;
            } else {
                let holding = tb_setting_items(&self.module, "UseHoldingItemsForIgnore");
                let blocks_ignore = tb_setting_items(&self.module, "UseBlocksForIgnore");
                let only_block = tb_setting_bool(&self.module, "UseOnlyBlock", false);
                let allowed = !(AUTO_CLICKER_SPECIAL_ITEMS_FOR_IGNORE
                    .contains(&deps.main_hand_item)
                    && deps.main_hand_custom_name)
                    && !holding
                        .iter()
                        .any(|item| item == deps.main_hand_item || item == deps.off_hand_item)
                    && !(only_block && !deps.main_hand_is_block_item && !deps.off_hand_is_block_item)
                    && match &deps.crosshair {
                        CrosshairTarget::Block => match deps.crosshair_block {
                            Some(block) => !blocks_ignore.iter().any(|b| b == block),
                            None => true,
                        },
                        _ => true,
                    };
                if allowed {
                    if tb_setting_bool(&self.module, "UseDelayStart", false) && self.need_to_wait {
                        self.need_to_wait = false;
                        self.use_delay_ticks_left = 2;
                        return outcome;
                    }
                    if self.use_delay_ticks_left > 0 {
                        self.use_delay_ticks_left -= 1;
                        if self.use_delay_ticks_left > 0 {
                            return outcome;
                        }
                    }

                    let press = &mut *deps.press_use_key;
                    outcome.use_clicks = self.use_clicker.click(&self.module, deps.now_ms, press, None);
                }
            }
        }

        outcome
    }
}

// ----- combat/autoclicker/ModuleTriggerBot.kt (GPL-3.0, (c) 2015-2026 CCBlueX) -----
//
// TriggerBot: attacks the enemy under the crosshair without holding the
// attack button.

pub const TRIGGER_BOT_NAME: &str = "TriggerBot";

pub fn trigger_bot() -> Module {
    Module::new(
        "TriggerBot",
        COMBAT,
        vec![
            // Clicker(this, mc.options.keyAttack, simulateAttackKeyDown = true)
            choice("Technique", "Human", &["Human", "Constant"]),
            int_range("CPS", 11, 14, 1, 30),
            int("MaxPerTick", 2, 1, 5),
            boolean("MissCooldown", true),
            float_range("ItemCooldownMinimum", 1.0, 1.0, 0.0, 2.0),
            // CrosshairAttacker(this, clicker) settings on the module itself.
            choice("OnItemUse", "Wait", &["Wait", "Stop", "Ignore"]),
            multi_choice("Weapon", &["Any"], &WEAPON_TYPE_TAGS),
            choice("Criticals", "Smart", &["Smart", "Ignore", "Always"]),
            int("DelayPostStopUse", 0, 0, 20),
        ],
    )
}

/// Upstream `ModuleTriggerBot` state.
pub struct TriggerBot {
    pub module: Module,
    pub clicker: Clicker,
    pub attacker: CrosshairAttacker,
}

impl TriggerBot {
    pub fn new(seed: u64, attack_cooldown_progress: std::sync::Arc<dyn Fn(i32) -> f64>) -> Self {
        TriggerBot {
            module: trigger_bot(),
            clicker: Clicker::new(0, true, "", true, 30, seed, true, attack_cooldown_progress),
            attacker: CrosshairAttacker::new(""),
        }
    }

    /// Upstream `tickHandler`. Returns whether the tick clicked.
    pub fn on_tick(&mut self, deps: &mut CrosshairAttackerDeps) -> bool {
        let wants_to_attack =
            deps.crosshair_entity.is_some() && deps.crosshair_should_be_attacked;
        if self.attacker.is_attacking(&self.module, deps, wants_to_attack) {
            self.attacker.attack(&self.module, deps, &mut self.clicker)
        } else {
            false
        }
    }
}

mod autoclicker_triggerbot_tests {
    use super::*;

    fn always_pass_progress() -> std::sync::Arc<dyn Fn(i32) -> f64> {
        std::sync::Arc::new(|_ticks: i32| 10.0)
    }

    #[test]
    fn jdk_random_draws_a_deterministic_stream() {
        let mut a = JdkRandom::new(42);
        let mut b = JdkRandom::new(42);
        for _ in 0..16 {
            assert!((a.next_double() - b.next_double()).abs() < f64::EPSILON);
        }
        for _ in 0..8 {
            assert!((a.next_gaussian() - b.next_gaussian()).abs() < f64::EPSILON);
        }
        // JDK sanity: a fresh Random(42).nextDouble() is 0.7275636800328681.
        let mut c = JdkRandom::new(42);
        assert!((c.next_double() - 0.7275636800328681).abs() < 1e-12);
    }

    #[test]
    fn click_plan_plans_presses_and_resets_after_idle() {
        let mut plan = ClickPlan::new(7);
        plan.technique = ClickTechnique::Constant;
        plan.set_cps((2, 2));

        plan.tick(0);
        // Press at 0 is due this tick; the 500 ms spacing puts the next press
        // on tick 10 ((0, 500] with tickTime 0).
        assert_eq!(plan.clicks_at(0), 1);
        assert_eq!(plan.clicks_at(1), 0);
        assert_eq!(plan.clicks_at(10), 1);

        plan.tick(50);
        assert_eq!(plan.clicks_at(0), 0);

        // A gap longer than IDLE_MS ends the combo and starts a fresh one.
        plan.tick(550);
        assert_eq!(plan.clicks_at(0), 1);
        assert_eq!(plan.combo_ms(), 0);
    }

    #[test]
    fn clicker_click_drops_presses_on_failed_gates_and_blocks() {
        let module = Module::new(
            "AutoClicker",
            COMBAT,
            vec![
                boolean("Attack", true),
                choice("AttackTechnique", "Constant", &["Human", "Constant"]),
                int_range("AttackCPS", 1, 1, 1, 30),
                int("AttackMaxPerTick", 2, 1, 5),
                boolean("AttackMissCooldown", true),
                float_range("AttackItemCooldownMinimum", 1.0, 1.0, 0.0, 2.0),
                boolean("AttackDelayOnBroken", true),
                choice("AttackObjective", "Any", &["Enemy", "Entity", "Block", "Any"]),
                choice("AttackOnItemUse", "Wait", &["Wait", "Stop", "Ignore"]),
                multi_choice("AttackWeapon", &["Any"], &WEAPON_TYPE_TAGS),
                choice("AttackCriticals", "Smart", &["Smart", "Ignore", "Always"]),
                int("AttackDelayPostStopUse", 0, 0, 20),
                boolean("Use", false),
                choice("UseTechnique", "Human", &["Human", "Constant"]),
                int_range("UseCPS", 11, 14, 1, 30),
                int("UseMaxPerTick", 2, 1, 5),
                items("UseHoldingItemsForIgnore", &[]),
                items("UseBlocksForIgnore", &[]),
                boolean("UseDelayStart", false),
                boolean("UseOnlyBlock", false),
                boolean("UseRequiresNoInput", false),
            ],
        );
        let mut clicker = Clicker::new(0, true, "Attack", true, 30, 11, true, always_pass_progress());
        clicker.running = true;
        clicker.tick(&module, 0);

        // One press due at tick 0 (interval 1000 ms at CPS 1).
        assert_eq!(clicker.get_click_amount(0), 1);

        let mut block = || true;
        assert_eq!(clicker.click(&module, 0, &mut block, None), 1);
        assert_eq!(clicker.click_amount, Some(1));
        assert_eq!(clicker.ticks_since_last_click, 0);
        // The plan consumed the tick's single press.
        assert_eq!(clicker.click(&module, 0, &mut block, None), 0);

        // Miss cooldown: a press is consumed but dropped.
        clicker.tick(&module, 1000);
        clicker.miss_time = 1;
        assert_eq!(clicker.click(&module, 1000, &mut block, None), 0);
        assert_eq!(clicker.click_amount, Some(0));
        clicker.miss_time = 0;

        // A failed block consumes the press but does not count as a click.
        clicker.tick(&module, 2000);
        let mut fail = || false;
        assert_eq!(clicker.click(&module, 2000, &mut fail, None), 0);
        assert_eq!(clicker.click_amount, Some(0));

        // An open item-cooldown gate blocks the press too.
        clicker.tick(&module, 3000);
        clicker.item_cooldown.as_mut().unwrap().next_cooldown = 100.0;
        assert_eq!(clicker.click(&module, 3000, &mut block, None), 0);
    }

    #[test]
    fn clicker_group_player_press_replaces_a_planned_press() {
        let module = Module::new("AutoClicker", COMBAT, vec![
            boolean("Attack", true),
            choice("AttackTechnique", "Constant", &["Human", "Constant"]),
            int_range("AttackCPS", 2, 2, 1, 30),
            int("AttackMaxPerTick", 2, 1, 5),
            boolean("AttackMissCooldown", true),
            float_range("AttackItemCooldownMinimum", 1.0, 1.0, 0.0, 2.0),
            boolean("AttackDelayOnBroken", true),
            choice("AttackObjective", "Any", &["Enemy", "Entity", "Block", "Any"]),
            choice("AttackOnItemUse", "Wait", &["Wait", "Stop", "Ignore"]),
            multi_choice("AttackWeapon", &["Any"], &WEAPON_TYPE_TAGS),
            choice("AttackCriticals", "Smart", &["Smart", "Ignore", "Always"]),
            int("AttackDelayPostStopUse", 0, 0, 20),
            boolean("Use", false),
            choice("UseTechnique", "Human", &["Human", "Constant"]),
            int_range("UseCPS", 11, 14, 1, 30),
            int("UseMaxPerTick", 2, 1, 5),
            items("UseHoldingItemsForIgnore", &[]),
            items("UseBlocksForIgnore", &[]),
            boolean("UseDelayStart", false),
            boolean("UseOnlyBlock", false),
            boolean("UseRequiresNoInput", false),
        ]);
        let mut group = ClickerGroup::new(vec![Clicker::new(
            0, true, "Attack", true, 30, 5, true, always_pass_progress(),
        )]);
        group.clickers[0].running = true;
        group.tick(&module, 0);
        assert_eq!(group.clickers[0].get_click_amount(10), 1);

        group.on_player_press(0, true, false);
        assert_eq!(group.clickers[0].get_click_amount(10), 0);

        // A press with a screen open is not a click on the key.
        group.on_player_press(0, true, true);
        assert_eq!(group.clickers[0].get_click_amount(10), 0);
    }

    #[test]
    fn clicker_group_other_clicker_follows_the_leader_plan() {
        let module = Module::new("AutoClicker", COMBAT, vec![
            boolean("Attack", true),
            choice("AttackTechnique", "Constant", &["Human", "Constant"]),
            int_range("AttackCPS", 2, 2, 1, 30),
            int("AttackMaxPerTick", 2, 1, 5),
            boolean("AttackMissCooldown", true),
            float_range("AttackItemCooldownMinimum", 1.0, 1.0, 0.0, 2.0),
            boolean("AttackDelayOnBroken", true),
            choice("AttackObjective", "Any", &["Enemy", "Entity", "Block", "Any"]),
            choice("AttackOnItemUse", "Wait", &["Wait", "Stop", "Ignore"]),
            multi_choice("AttackWeapon", &["Any"], &WEAPON_TYPE_TAGS),
            choice("AttackCriticals", "Smart", &["Smart", "Ignore", "Always"]),
            int("AttackDelayPostStopUse", 0, 0, 20),
            boolean("Use", false),
            choice("UseTechnique", "Human", &["Human", "Constant"]),
            int_range("UseCPS", 11, 14, 1, 30),
            int("UseMaxPerTick", 2, 1, 5),
            items("UseHoldingItemsForIgnore", &[]),
            items("UseBlocksForIgnore", &[]),
            boolean("UseDelayStart", false),
            boolean("UseOnlyBlock", false),
            boolean("UseRequiresNoInput", false),
        ]);
        let mut group = ClickerGroup::new(vec![
            Clicker::new(0, true, "Attack", true, 30, 5, true, always_pass_progress()),
            Clicker::new(0, true, "Attack", true, 30, 6, true, always_pass_progress()),
        ]);
        group.clickers[0].running = true;
        group.clickers[1].running = true;
        group.tick(&module, 0);

        let mut block = || true;
        let clicks = group.click(0, &module, 0, &mut block);
        assert_eq!(clicks, 1);
        // The follower adopted the leader's future presses (500 ms apart) in
        // place of its own 1000 ms plan.
        assert_eq!(group.clickers[1].get_click_amount(10), 1);
        assert_eq!(group.clickers[1].get_click_amount(20), 1);
    }

    type SharedVec = std::cell::RefCell<Vec<u64>>;
    type SharedBool = std::cell::RefCell<bool>;

    fn attacker_deps<'a>(
        crosshair_entity: Option<u64>,
        using_item: bool,
        now_ms: i64,
        on_target: &'a SharedVec,
        released: &'a SharedBool,
        pressed: &'a SharedBool,
    ) -> CrosshairAttackerDeps<'a> {
        CrosshairAttackerDeps {
            crosshair_entity,
            crosshair_should_be_attacked: true,
            player_is_using_item: using_item,
            now_ms,
            weapon_matches: Box::new(|_tags| true),
            should_wait_for_crit: Box::new(|_ignore_state| false),
            would_do_critical_hit: Box::new(|| true),
            on_target: Box::new(move |entity| on_target.borrow_mut().push(entity)),
            release_using_item: Box::new(|| *released.borrow_mut() = true),
            press_attack_key: Box::new(|| {
                *pressed.borrow_mut() = true;
                true
            }),
        }
    }

    #[test]
    fn crosshair_attacker_attack_clicks_or_waits() {
        let mut module = Module::new("TriggerBot", COMBAT, vec![
            choice("Technique", "Human", &["Human", "Constant"]),
            int_range("CPS", 11, 14, 1, 30),
            int("MaxPerTick", 2, 1, 5),
            boolean("MissCooldown", true),
            float_range("ItemCooldownMinimum", 1.0, 1.0, 0.0, 2.0),
            choice("OnItemUse", "Wait", &["Wait", "Stop", "Ignore"]),
            multi_choice("Weapon", &["Any"], &WEAPON_TYPE_TAGS),
            choice("Criticals", "Smart", &["Smart", "Ignore", "Always"]),
            int("DelayPostStopUse", 0, 0, 20),
        ]);
        let on_target = std::cell::RefCell::new(Vec::new());
        let _released = std::cell::RefCell::new(false);
        let pressed = std::cell::RefCell::new(false);
        let mut clicker = Clicker::new(0, true, "", true, 30, 3, true, always_pass_progress());
        clicker.running = true;

        // A critical hit still pending (Smart + shouldWaitForCrit) returns
        // true without clicking.
        let mut deps = attacker_deps(Some(9), false, 0, &on_target, &_released, &pressed);
        deps.should_wait_for_crit = Box::new(|_| true);
        let mut attacker = CrosshairAttacker::new("");
        assert!(attacker.attack(&module, &mut deps, &mut clicker));
        assert_eq!(on_target.borrow().clone(), vec![9]);
        assert!(clicker.click_amount.is_none() || clicker.click_amount == Some(0));

        // Ignore mode: no critical gating, the click lands.
        clicker.tick(&module, 0);
        module.setting_mut("Criticals").unwrap().kind =
            crate::value::ValueKind::Choice("Ignore".to_string(), vec!["Smart".to_string(), "Ignore".to_string(), "Always".to_string()]);
        let mut deps = attacker_deps(Some(9), false, 0, &on_target, &_released, &pressed);
        assert!(attacker.attack(&module, &mut deps, &mut clicker));
        assert!(*pressed.borrow());

        // Using an item with Wait: the tick is consumed, then the delay runs.
        module.setting_mut("Criticals").unwrap().kind =
            crate::value::ValueKind::Choice("Smart".to_string(), vec!["Smart".to_string(), "Ignore".to_string(), "Always".to_string()]);
        *pressed.borrow_mut() = false;
        let mut deps = attacker_deps(Some(9), true, 0, &on_target, &_released, &pressed);
        assert!(!attacker.attack(&module, &mut deps, &mut clicker));
        // Not using anymore: the Wait suspension releases into the delay.
        let mut deps = attacker_deps(Some(9), false, 0, &on_target, &_released, &pressed);
        assert!(!attacker.attack(&module, &mut deps, &mut clicker));
        // DelayPostStopUse defaults to 0, so the next tick clicks.
        clicker.tick(&module, 2000);
        let mut deps = attacker_deps(Some(9), false, 2000, &on_target, &_released, &pressed);
        assert!(attacker.attack(&module, &mut deps, &mut clicker));
        assert!(*pressed.borrow());
    }

    #[test]
    fn trigger_bot_attacks_the_crosshair_entity() {
        let mut bot = TriggerBot::new(4, always_pass_progress());
        let on_target = std::cell::RefCell::new(Vec::new());
        let released = std::cell::RefCell::new(false);
        let pressed = std::cell::RefCell::new(false);
        let mut deps = attacker_deps(None, false, 0, &on_target, &released, &pressed);
        assert!(!bot.on_tick(&mut deps));

        let mut deps = attacker_deps(Some(3), false, 0, &on_target, &released, &pressed);
        deps.crosshair_should_be_attacked = false;
        assert!(!bot.on_tick(&mut deps));

        bot.clicker.tick(&bot.module, 0);
        let mut deps = attacker_deps(Some(3), false, 0, &on_target, &released, &pressed);
        assert!(bot.on_tick(&mut deps));
        assert_eq!(on_target.borrow().clone(), vec![3]);
        assert!(*pressed.borrow());
    }

    #[test]
    fn auto_clicker_on_tick_gates_and_clicks() {
        let mut auto = AutoClicker::new(5, always_pass_progress());
        auto.module
            .setting_mut("AttackCriticals")
            .unwrap()
            .kind = crate::value::ValueKind::Choice(
            "Ignore".to_string(),
            vec!["Smart".to_string(), "Ignore".to_string(), "Always".to_string()],
        );
        auto.module.setting_mut("Use").unwrap().kind = crate::value::ValueKind::Bool(true);
        auto.module
            .setting_mut("UseRequiresNoInput")
            .unwrap()
            .kind = crate::value::ValueKind::Bool(true);

        // Tick 1: attack pressed on open crosshair (Any objective), past the
        // finish-break window.
        let mut on_target: Vec<u64> = Vec::new();
        let mut attack_pressed_flag = false;
        {
            auto.attack_clicker.tick(&auto.module, 1000);
            let mut deps = AutoClickerTickDeps {
                attack_pressed: true,
                use_pressed: false,
                is_destroying: false,
                crosshair: CrosshairTarget::None,
                main_hand_item: "minecraft:stone_sword",
                off_hand_item: "minecraft:shield",
                main_hand_custom_name: false,
                main_hand_is_block_item: false,
                off_hand_is_block_item: false,
                crosshair_block: None,
                press_use_key: Box::new(|| true),
                now_ms: 1000,
            };
            let mut attacker_deps = CrosshairAttackerDeps {
                crosshair_entity: None,
                crosshair_should_be_attacked: true,
                player_is_using_item: false,
                now_ms: 1000,
                weapon_matches: Box::new(|_| true),
                should_wait_for_crit: Box::new(|_| false),
                would_do_critical_hit: Box::new(|| true),
                on_target: Box::new(|e| on_target.push(e)),
                release_using_item: Box::new(|| {}),
                press_attack_key: Box::new(|| {
                    attack_pressed_flag = true;
                    true
                }),
            };
            let outcome = auto.on_tick(&mut deps, &mut attacker_deps);
            assert_eq!(outcome.attack_clicks, 1);
            assert_eq!(outcome.use_clicks, 0);
        }
        assert!(attack_pressed_flag);
        assert!(on_target.is_empty());

        // Tick 2: breaking a block, so no attack click.
        {
            auto.attack_clicker.tick(&auto.module, 2000);
            let mut deps = AutoClickerTickDeps {
                attack_pressed: true,
                use_pressed: false,
                is_destroying: true,
                crosshair: CrosshairTarget::None,
                main_hand_item: "minecraft:stone_sword",
                off_hand_item: "minecraft:shield",
                main_hand_custom_name: false,
                main_hand_is_block_item: false,
                off_hand_is_block_item: false,
                crosshair_block: None,
                press_use_key: Box::new(|| true),
                now_ms: 2000,
            };
            let mut attacker_deps = CrosshairAttackerDeps {
                crosshair_entity: None,
                crosshair_should_be_attacked: true,
                player_is_using_item: false,
                now_ms: 2000,
                weapon_matches: Box::new(|_| true),
                should_wait_for_crit: Box::new(|_| false),
                would_do_critical_hit: Box::new(|| true),
                on_target: Box::new(|_| {}),
                release_using_item: Box::new(|| {}),
                press_attack_key: Box::new(|| true),
            };
            let outcome = auto.on_tick(&mut deps, &mut attacker_deps);
            assert_eq!(outcome.attack_clicks, 0);
        }

        // Tick 3: a finished break 100 ms ago, within the DelayOnBroken
        // window, so no attack click.
        auto.on_player_action_packet("STOP_DESTROY_BLOCK", 2500);
        {
            auto.attack_clicker.tick(&auto.module, 2600);
            let mut deps = AutoClickerTickDeps {
                attack_pressed: true,
                use_pressed: false,
                is_destroying: false,
                crosshair: CrosshairTarget::None,
                main_hand_item: "minecraft:stone_sword",
                off_hand_item: "minecraft:shield",
                main_hand_custom_name: false,
                main_hand_is_block_item: false,
                off_hand_is_block_item: false,
                crosshair_block: None,
                press_use_key: Box::new(|| true),
                now_ms: 2600,
            };
            let mut attacker_deps = CrosshairAttackerDeps {
                crosshair_entity: None,
                crosshair_should_be_attacked: true,
                player_is_using_item: false,
                now_ms: 2600,
                weapon_matches: Box::new(|_| true),
                should_wait_for_crit: Box::new(|_| false),
                would_do_critical_hit: Box::new(|| true),
                on_target: Box::new(|_| {}),
                release_using_item: Box::new(|| {}),
                press_attack_key: Box::new(|| true),
            };
            let outcome = auto.on_tick(&mut deps, &mut attacker_deps);
            assert_eq!(outcome.attack_clicks, 0);
        }

        // Tick 4: objective Enemy with no entity under the crosshair, so no
        // attack click; then an enemy appears and the attack click lands with
        // AutoWeapon.onTarget called.
        auto.module
            .setting_mut("AttackObjective")
            .unwrap()
            .kind = crate::value::ValueKind::Choice(
            "Enemy".to_string(),
            vec!["Enemy".to_string(), "Entity".to_string(), "Block".to_string(), "Any".to_string()],
        );
        {
            auto.attack_clicker.tick(&auto.module, 4000);
            let mut deps = AutoClickerTickDeps {
                attack_pressed: true,
                use_pressed: false,
                is_destroying: false,
                crosshair: CrosshairTarget::None,
                main_hand_item: "minecraft:stone_sword",
                off_hand_item: "minecraft:shield",
                main_hand_custom_name: false,
                main_hand_is_block_item: false,
                off_hand_is_block_item: false,
                crosshair_block: None,
                press_use_key: Box::new(|| true),
                now_ms: 4000,
            };
            let mut attacker_deps = CrosshairAttackerDeps {
                crosshair_entity: None,
                crosshair_should_be_attacked: true,
                player_is_using_item: false,
                now_ms: 4000,
                weapon_matches: Box::new(|_| true),
                should_wait_for_crit: Box::new(|_| false),
                would_do_critical_hit: Box::new(|| true),
                on_target: Box::new(|_| {}),
                release_using_item: Box::new(|| {}),
                press_attack_key: Box::new(|| true),
            };
            let outcome = auto.on_tick(&mut deps, &mut attacker_deps);
            assert_eq!(outcome.attack_clicks, 0);
        }
        {
            auto.attack_clicker.tick(&auto.module, 5000);
            let mut deps = AutoClickerTickDeps {
                attack_pressed: true,
                use_pressed: false,
                is_destroying: false,
                crosshair: CrosshairTarget::Entity { id: 7, should_be_attacked: true },
                main_hand_item: "minecraft:stone_sword",
                off_hand_item: "minecraft:shield",
                main_hand_custom_name: false,
                main_hand_is_block_item: false,
                off_hand_is_block_item: false,
                crosshair_block: None,
                press_use_key: Box::new(|| true),
                now_ms: 5000,
            };
            let mut attacker_deps = CrosshairAttackerDeps {
                crosshair_entity: Some(7),
                crosshair_should_be_attacked: true,
                player_is_using_item: false,
                now_ms: 5000,
                weapon_matches: Box::new(|_| true),
                should_wait_for_crit: Box::new(|_| false),
                would_do_critical_hit: Box::new(|| true),
                on_target: Box::new(|e| on_target.push(e)),
                release_using_item: Box::new(|| {}),
                press_attack_key: Box::new(|| true),
            };
            let outcome = auto.on_tick(&mut deps, &mut attacker_deps);
            assert_eq!(outcome.attack_clicks, 1);
        }
        assert_eq!(on_target, vec![7]);
    }

    #[test]
    fn auto_clicker_use_button_gates_and_delay_start() {
        let mut auto = AutoClicker::new(9, always_pass_progress());
        auto.module.setting_mut("Use").unwrap().kind = crate::value::ValueKind::Bool(true);
        auto.module
            .setting_mut("UseRequiresNoInput")
            .unwrap()
            .kind = crate::value::ValueKind::Bool(true);

        // A held item on the ignore list blocks the use click.
        {
            auto.use_clicker.tick(&auto.module, 0);
            let mut deps = AutoClickerTickDeps {
                attack_pressed: false,
                use_pressed: true,
                is_destroying: false,
                crosshair: CrosshairTarget::None,
                main_hand_item: "minecraft:ender_pearl",
                off_hand_item: "minecraft:shield",
                main_hand_custom_name: false,
                main_hand_is_block_item: false,
                off_hand_is_block_item: false,
                crosshair_block: None,
                press_use_key: Box::new(|| true),
                now_ms: 0,
            };
            let mut attacker_deps = CrosshairAttackerDeps {
                crosshair_entity: None,
                crosshair_should_be_attacked: false,
                player_is_using_item: false,
                now_ms: 0,
                weapon_matches: Box::new(|_| true),
                should_wait_for_crit: Box::new(|_| false),
                would_do_critical_hit: Box::new(|| true),
                on_target: Box::new(|_| {}),
                release_using_item: Box::new(|| {}),
                press_attack_key: Box::new(|| true),
            };
            let outcome = auto.on_tick(&mut deps, &mut attacker_deps);
            assert_eq!(outcome.use_clicks, 0);
        }

        // OnlyBlock with no block item in either hand blocks the use click.
        auto.module.setting_mut("UseOnlyBlock").unwrap().kind = crate::value::ValueKind::Bool(true);
        {
            auto.use_clicker.tick(&auto.module, 1000);
            let mut deps = AutoClickerTickDeps {
                attack_pressed: false,
                use_pressed: true,
                is_destroying: false,
                crosshair: CrosshairTarget::None,
                main_hand_item: "minecraft:stone_sword",
                off_hand_item: "minecraft:shield",
                main_hand_custom_name: false,
                main_hand_is_block_item: false,
                off_hand_is_block_item: false,
                crosshair_block: None,
                press_use_key: Box::new(|| true),
                now_ms: 1000,
            };
            let mut attacker_deps = CrosshairAttackerDeps {
                crosshair_entity: None,
                crosshair_should_be_attacked: false,
                player_is_using_item: false,
                now_ms: 1000,
                weapon_matches: Box::new(|_| true),
                should_wait_for_crit: Box::new(|_| false),
                would_do_critical_hit: Box::new(|| true),
                on_target: Box::new(|_| {}),
                release_using_item: Box::new(|| {}),
                press_attack_key: Box::new(|| true),
            };
            let outcome = auto.on_tick(&mut deps, &mut attacker_deps);
            assert_eq!(outcome.use_clicks, 0);
        }
        auto.module.setting_mut("UseOnlyBlock").unwrap().kind = crate::value::ValueKind::Bool(false);

        // DelayStart: two ticks pass before the first use click.
        auto.module.setting_mut("UseDelayStart").unwrap().kind = crate::value::ValueKind::Bool(true);
        let mut use_clicked = 0;
        for tick_ms in [2000_i64, 3000, 4000] {
            auto.use_clicker.tick(&auto.module, tick_ms);
            let mut deps = AutoClickerTickDeps {
                attack_pressed: false,
                use_pressed: true,
                is_destroying: false,
                crosshair: CrosshairTarget::None,
                main_hand_item: "minecraft:stone_sword",
                off_hand_item: "minecraft:shield",
                main_hand_custom_name: false,
                main_hand_is_block_item: false,
                off_hand_is_block_item: false,
                crosshair_block: None,
                press_use_key: Box::new(|| {
                    use_clicked += 1;
                    true
                }),
                now_ms: tick_ms,
            };
            let mut attacker_deps = CrosshairAttackerDeps {
                crosshair_entity: None,
                crosshair_should_be_attacked: false,
                player_is_using_item: false,
                now_ms: tick_ms,
                weapon_matches: Box::new(|_| true),
                should_wait_for_crit: Box::new(|_| false),
                would_do_critical_hit: Box::new(|| true),
                on_target: Box::new(|_| {}),
                release_using_item: Box::new(|| {}),
                press_attack_key: Box::new(|| true),
            };
            let outcome = auto.on_tick(&mut deps, &mut attacker_deps);
            assert_eq!(outcome.use_clicks, if tick_ms == 4000 { 1 } else { 0 });
        }
        assert_eq!(use_clicked, 1);
        // The next use tick clicks straight away.
        {
            auto.use_clicker.tick(&auto.module, 5000);
            let mut deps = AutoClickerTickDeps {
                attack_pressed: false,
                use_pressed: true,
                is_destroying: false,
                crosshair: CrosshairTarget::None,
                main_hand_item: "minecraft:stone_sword",
                off_hand_item: "minecraft:shield",
                main_hand_custom_name: false,
                main_hand_is_block_item: false,
                off_hand_is_block_item: false,
                crosshair_block: None,
                press_use_key: Box::new(|| {
                    use_clicked += 1;
                    true
                }),
                now_ms: 5000,
            };
            let mut attacker_deps = CrosshairAttackerDeps {
                crosshair_entity: None,
                crosshair_should_be_attacked: false,
                player_is_using_item: false,
                now_ms: 5000,
                weapon_matches: Box::new(|_| true),
                should_wait_for_crit: Box::new(|_| false),
                would_do_critical_hit: Box::new(|| true),
                on_target: Box::new(|_| {}),
                release_using_item: Box::new(|| {}),
                press_attack_key: Box::new(|| true),
            };
            let outcome = auto.on_tick(&mut deps, &mut attacker_deps);
            assert_eq!(outcome.use_clicks, 1);
        }
        assert_eq!(use_clicked, 2);
    }
}
