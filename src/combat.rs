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
