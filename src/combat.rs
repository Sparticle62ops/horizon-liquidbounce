//! Port of `features/module/modules/combat/*` (slice 2 start): the small
//! top-level combat modules. Game-facing calls stay behind the same seams
//! upstream uses (events, `player`, `shouldBeAttacked`), injected so the
//! logic itself stays exact.

use crate::category::COMBAT;
use crate::module::Module;
use crate::value::{boolean, choice, float, float_range, int, int_range, multi_choice, ValueKind};

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
