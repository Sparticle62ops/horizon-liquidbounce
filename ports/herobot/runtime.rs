pub mod ai {
// HeroBot AI script model ported 1:1 from HerobaneNair/HeRoBot
// (herobot-common/.../ai: AiScript, VarType, VarDecl, FuncDecl, Comment),
// CC-BY-NC-4.0. Java records/annotations become plain structs; the
// JSON codec (AiScriptCodec.java, 547 lines) lands next.

pub const CURRENT_VERSION: i32 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VarType { Int, Double, Bool, Position, Rotation, String, Uuid, Item }

impl VarType {
    pub fn display_name(self) -> &'static str {
        match self {
            VarType::Bool => "boolean", VarType::Int => "int",
            VarType::Double => "decimal", VarType::Position => "position",
            VarType::Rotation => "direction", VarType::String => "String",
            VarType::Uuid => "UUID", VarType::Item => "Item",
        }
    }
    pub fn chip_label(self) -> &'static str {
        match self {
            VarType::Bool => "bool", VarType::Int => "int",
            VarType::Double => "deci", VarType::Position => "Pos",
            VarType::Rotation => "Dir", VarType::String => "String",
            VarType::Uuid => "UUID", VarType::Item => "Item",
        }
    }
}

/// A script variable slot. Java's  becomes Value;
/// the folder prefix matches upstream "folder/name" qualified names.
#[derive(Clone, Debug)]
pub struct VarDecl {
    pub name: String,
    pub ty: VarType,
    pub default_value: Option<String>,
    pub folder: String,
}

impl VarDecl {
    pub fn qualified_name(&self) -> String {
        if self.folder.is_empty() { self.name.clone() } else { format!("{}/{}", self.folder, self.name) }
    }
}

#[derive(Clone, Debug)]
pub struct FuncDecl {
    pub name: String,
    pub params: Vec<VarType>,
    pub folder: String,
}

impl FuncDecl {
    pub fn qualified_name(&self) -> String {
        if self.folder.is_empty() { self.name.clone() } else { format!("{}/{}", self.folder, self.name) }
    }
    pub fn num_params(&self) -> usize { self.params.len() }
    pub fn param_type(&self, index: usize) -> Option<VarType> { self.params.get(index).copied() }
    pub fn with_param_added(mut self, ty: VarType) -> Self { self.params.push(ty); self }
    pub fn with_param_removed(mut self, index: usize) -> Self {
        if index < self.params.len() { self.params.remove(index); }
        self
    }
    pub fn with_param_type(mut self, index: usize, ty: VarType) -> Self {
        if let Some(p) = self.params.get_mut(index) { *p = ty; }
        self
    }
}

/// Comment style flags, same bit values as upstream.
pub const STYLE_BOLD: u8 = 1;
pub const STYLE_ITALIC: u8 = 2;
pub const STYLE_UNDERLINE: u8 = 4;
pub const STYLE_STRIKE: u8 = 8;

#[derive(Clone, Debug)]
pub struct Comment {
    pub id: i32,
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub styles: Vec<u8>,
    pub attached_to: i32,
    pub off_x: f64,
    pub off_y: f64,
}

impl Comment {
    pub fn new(id: i32, x: f64, y: f64, text: &str) -> Self {
        let text = text.to_string();
        let styles = vec![0u8; text.len()];
        Self { id, x, y, text, styles, attached_to: -1, off_x: 0.0, off_y: 0.0 }
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.styles = vec![0u8; self.text.len()];
    }

    pub fn style_at(&self, i: usize) -> u8 { self.styles.get(i).copied().unwrap_or(0) }

    pub fn set_styles(&mut self, s: Vec<u8>) {
        if s.len() == self.text.len() { self.styles = s; }
    }

    pub fn insert(&mut self, idx: usize, s: &str, style: u8) {
        if s.is_empty() { return; }
        let idx = idx.min(self.text.len());
        self.text.insert_str(idx, s);
        let mut ns = Vec::with_capacity(self.styles.len() + s.len());
        ns.extend_from_slice(&self.styles[..idx]);
        ns.extend(std::iter::repeat(style).take(s.len()));
        ns.extend_from_slice(&self.styles[idx..]);
        self.styles = ns;
    }

    pub fn delete(&mut self, from: usize, to: usize) {
        let from = from.min(self.text.len());
        let to = to.max(from).min(self.text.len());
        if from == to { return; }
        self.text.replace_range(from..to, "");
        self.styles.drain(from..to);
    }

    pub fn toggle_style(&mut self, from: usize, to: usize, flag: u8) {
        let from = from.min(self.styles.len());
        let to = to.max(from).min(self.styles.len());
        if from == to { return; }
        let all = self.styles[from..to].iter().all(|s| s & flag != 0);
        for s in &mut self.styles[from..to] {
            *s = if all { *s & !flag } else { *s | flag };
        }
    }

    pub fn range_has_style(&self, from: usize, to: usize, flag: u8) -> bool {
        let from = from.min(self.styles.len());
        let to = to.max(from).min(self.styles.len());
        if from == to { return false; }
        self.styles[from..to].iter().all(|s| s & flag != 0)
    }
}

/// Wire between two blocks: from block/port to block (and optional port).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wire {
    pub from_block_id: i32,
    pub out_port: i32,
    pub to_block_id: i32,
    pub to_port: i32,
}

impl Wire {
    pub fn new(from_block_id: i32, out_port: i32, to_block_id: i32) -> Self {
        Self { from_block_id, out_port, to_block_id, to_port: -1 }
    }
    pub fn with_to_port(from_block_id: i32, out_port: i32, to_block_id: i32, to_port: i32) -> Self {
        Self { from_block_id, out_port, to_block_id, to_port }
    }
}

/// A parameter value: what upstream keeps as  in
/// BlockInstance.params. jsonToParam's shape: bool, int, double, string.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Bool(bool),
    Int(i64),
    Double(f64),
    Str(String),
}

impl ParamValue {
    /// Upstream jsonToParam fallback: numbers that are whole become int.
    pub fn from_json(v: &serde_json::Value) -> ParamValue {
        match v {
            serde_json::Value::Bool(b) => ParamValue::Bool(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ParamValue::Int(i)
                } else if let Some(d) = n.as_f64() {
                    if d == d.floor() && d.is_finite() {
                        ParamValue::Int(d as i64)
                    } else {
                        ParamValue::Double(d)
                    }
                } else {
                    ParamValue::Str(n.to_string())
                }
            }
            serde_json::Value::Null => ParamValue::Str(String::new()),
            other => ParamValue::Str(match other {
                serde_json::Value::String(s) => s.clone(),
                _ => other.to_string(),
            }),
        }
    }
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ParamValue::Bool(b) => serde_json::Value::Bool(*b),
            ParamValue::Int(i) => serde_json::json!(i),
            ParamValue::Double(d) => serde_json::json!(d),
            ParamValue::Str(s) => serde_json::json!(s),
        }
    }
}

/// A placed block in the editor graph. block_type is the BlockType
/// registry index; params/reporter_params/paired_id match upstream
/// BlockInstance.java (pairedId >= 0 links paired C-shape blocks).
#[derive(Clone, Debug)]
pub struct BlockInstance {
    pub id: i32,
    pub block_type: i32,
    pub x: f64,
    pub y: f64,
    pub paired_id: i32,
    pub params: std::collections::BTreeMap<String, ParamValue>,
    pub reporter_params: std::collections::BTreeMap<String, BlockInstance>,
}

impl BlockInstance {
    pub fn new(id: i32, block_type: i32, x: f64, y: f64) -> Self {
        Self {
            id,
            block_type,
            x,
            y,
            paired_id: -1,
            params: Default::default(),
            reporter_params: Default::default(),
        }
    }
    pub fn get_param(&self, key: &str) -> Option<&ParamValue> { self.params.get(key) }
    pub fn set_param(&mut self, key: impl Into<String>, value: ParamValue) {
        self.params.insert(key.into(), value);
    }
    pub fn set_reporter(&mut self, key: impl Into<String>, b: BlockInstance) {
        self.reporter_params.insert(key.into(), b);
    }
    /// clone with fresh ids (upstream cloneWithNewIds); id map filled by
    /// the clipboard module.
    pub fn clone_at(&self, id: i32, dx: f64, dy: f64) -> Self {
        let mut c = self.clone();
        c.id = id;
        c.x += dx;
        c.y += dy;
        c
    }
}

/// The script graph itself: variables, functions, blocks, wires, comments,
/// and the nextId allocator, 1:1 with AiScript.java.
#[derive(Clone, Debug)]
pub struct AiScript {
    pub name: String,
    pub version: i32,
    pub variables: Vec<VarDecl>,
    pub var_folders: Vec<String>,
    pub functions: Vec<FuncDecl>,
    pub blocks: Vec<BlockInstance>,
    pub wires: Vec<Wire>,
    pub comments: Vec<Comment>,
    next_id: i32,
}

impl AiScript {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: CURRENT_VERSION,
            variables: Vec::new(),
            var_folders: Vec::new(),
            functions: Vec::new(),
            blocks: Vec::new(),
            wires: Vec::new(),
            comments: Vec::new(),
            next_id: 1,
        }
    }

    pub fn function(&self, qualified_name: &str) -> Option<&FuncDecl> {
        self.functions.iter().find(|f| f.qualified_name() == qualified_name)
    }

    pub fn function_index(&self, qualified_name: &str) -> Option<usize> {
        self.functions.iter().position(|f| f.qualified_name() == qualified_name)
    }

    pub fn add_comment(&mut self, x: f64, y: f64, text: &str) -> &Comment {
        let id = self.next_id;
        self.next_id += 1;
        self.comments.push(Comment::new(id, x, y, text));
        self.comments.last().unwrap()
    }

    pub fn remove_comment(&mut self, id: i32) {
        self.comments.retain(|c| c.id != id);
    }

    pub fn comment(&self, id: i32) -> Option<&Comment> {
        self.comments.iter().find(|c| c.id == id)
    }

    pub fn add_block(&mut self, block_type: i32, x: f64, y: f64) -> BlockInstance {
        let b = BlockInstance::new(self.next_id, block_type, x, y);
        self.next_id += 1;
        self.blocks.push(b.clone());
        b
    }

    /// Upstream putBlock: insert/replace and bump the id allocator.
    pub fn put_block(&mut self, b: BlockInstance) {
        self.bump_next_id(b.id);
        if let Some(e) = self.blocks.iter_mut().find(|e| e.id == b.id) {
            *e = b;
        } else {
            self.blocks.push(b);
        }
    }

    /// Upstream registerNestedIds: reporter children bump nextId too.
    pub fn register_nested_ids(&mut self, b: &BlockInstance) {
        self.bump_next_id(b.id);
        for child in b.reporter_params.values() {
            self.register_nested_ids(child);
        }
    }

    pub fn fresh_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn bump_next_id(&mut self, id: i32) {
        if id >= self.next_id { self.next_id = id + 1; }
    }

    pub fn remove_block(&mut self, id: i32) {
        self.blocks.retain(|b| b.id != id);
        self.wires.retain(|w| w.from_block_id != id && w.to_block_id != id);
    }

    pub fn add_wire(&mut self, w: Wire) {
        self.wires.push(w);
    }

    pub fn outgoing(&self, block_id: i32, out_port: i32) -> Vec<&Wire> {
        self.wires.iter()
            .filter(|w| w.from_block_id == block_id && w.out_port == out_port)
            .collect()
    }

    /// Hat blocks of one type, sorted by x then y like upstream
    /// hatBlocks. block_type here is the BlockDef registry index.
    pub fn hat_blocks(&self, block_type: i32) -> Vec<&BlockInstance> {
        let mut r: Vec<&BlockInstance> = self.blocks.iter()
            .filter(|b| b.block_type == block_type).collect();
        r.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal)));
        r
    }

    pub fn block(&self, id: i32) -> Option<&BlockInstance> {
        self.blocks.iter().find(|b| b.id == id)
    }
}
}
pub mod block_registry {
// HeroBot AI block registry core ported 1:1 from HerobaneNair/HeRoBot
// (herobot-common/.../ai/block: BlockType, BlockCategory, BlockShape,
// BlockDef, ParamSlot, ParamType), CC-BY-NC-4.0.
// Java records/enums become structs; the 358-line BlockDefRegistry
// (one BlockDef per type with params) ports next.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ParamType { Boolean, Int, Double, String, Position, Rotation, Uuid, VarRef, Enum, BlockState, Item }

#[derive(Clone, Debug)]
pub struct ParamSlot {
    pub name: &'static str,
    pub ty: ParamType,
    pub default_value: Option<String>,
    pub enum_choices: Vec<&'static str>,
}

impl ParamSlot {
    pub fn of(name: &'static str, ty: ParamType, default_value: impl Into<String>) -> Self {
        Self { name, ty, default_value: Some(default_value.into()), enum_choices: Vec::new() }
    }
    pub fn of_bool(name: &'static str, d: bool) -> Self { Self::of(name, ParamType::Boolean, d.to_string()) }
    pub fn of_int(name: &'static str, d: i64) -> Self { Self::of(name, ParamType::Int, d.to_string()) }
    pub fn of_double(name: &'static str, d: f64) -> Self { Self::of(name, ParamType::Double, d.to_string()) }
    pub fn of_string(name: &'static str, d: &str) -> Self { Self::of(name, ParamType::String, d) }
    pub fn of_enum(name: &'static str, choices: Vec<&'static str>, d: &str) -> Self {
        Self { name, ty: ParamType::Enum, default_value: Some(d.to_string()), enum_choices: choices }
    }
    pub fn of_var_ref(name: &'static str, d: &str) -> Self { Self::of(name, ParamType::VarRef, d) }
    pub fn of_uuid(name: &'static str) -> Self {
        Self::of(name, ParamType::Uuid, "@e[limit=1,sort=nearest,isSelf=false]")
    }
    pub fn of_pos(name: &'static str) -> Self { Self::of(name, ParamType::Position, "~ ~ ~") }
    pub fn of_rot(name: &'static str) -> Self { Self::of(name, ParamType::Rotation, "~ ~") }
    pub fn of_item(name: &'static str, d: &str) -> Self { Self::of(name, ParamType::Item, d) }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockShape { Hat, Statement, CShape, CEnd, Reporter, Boolean }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockCategory {
    Event, Action, Motion, Path, Look, Voice, Control, Variable,
    Operator, Inventory, DataManipulation, Sensor, Functions,
}

impl BlockCategory {
    /// Upstream display text and argb color, verbatim.
    pub fn display(self) -> &'static str {
        match self {
            Self::Event => "EVENT", Self::Action => "ACTION", Self::Motion => "MOTION",
            Self::Path => "PATH", Self::Look => "LOOK", Self::Voice => "VOICE",
            Self::Control => "CONTROL", Self::Variable => "VARIABLE",
            Self::Operator => "OPERATOR", Self::Inventory => "INVENTORY",
            Self::DataManipulation => "DATA MANIP", Self::Sensor => "SENSOR",
            Self::Functions => "FUNCTIONS",
        }
    }
    pub fn color(self) -> u32 {
        match self {
            Self::Event => 0xFFEEBB22, Self::Action => 0xFFFFAA11, Self::Motion => 0xFF4499FF,
            Self::Path => 0xFF55CC55, Self::Look => 0xFF55BBDD, Self::Voice => 0xFFE0556E,
            Self::Control => 0xFFFFBB00, Self::Variable => 0xFFFF8811,
            Self::Operator => 0xFF44BB44, Self::Inventory => 0xFFCC66CC,
            Self::DataManipulation => 0xFF11AAAA, Self::Sensor => 0xFF55BBDD,
            Self::Functions => 0xFF9B6BEF,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BlockType {
    Start, OnToggle, OnMessage, MsgText,
    Move, Strafe, StopMovement, Sneak, Sprint, Autojump,
    Use, Swing, Jump, AttemptAutojump, Attack, DropItem, SwapHands,
    PickBlock, SendMessage, StopAction, StopAll,
    LookDirection, LookCardinal, LookAtPos, LookAtEntity, Turn,
    GotoPos, GotoEntity, StopPath, PathSetting, PathMoveType, InPath, PathReached, PathFailed,
    PlaySound, StopSound, IsSpeaking, Bluetooth, StopBluetooth, IsBluetoothed,
    SelectHotbar, OpenInventory, CloseScreen, Handedness, InvClick, InvSwapHotbar,
    InvHeldThrow, InvHeldDrag, QuickLoot, RecipeBook, InventoryOpen, ContainerSize,
    TradeSelect, TradeRestock, TradeCheck,
    If, ElseIf, For, While, LoopIter, BlockEnd, Break, Wait, WaitUntil,
    StopScript, PauseScript, SetScript,
    SetVar, ChangeVar, ReadVar,
    Scoreboard, HasTag, Health, Position, Yaw, BlockAt, IsTouchingBlock,
    DistanceTo, TimeOfDay, EveryXTicks, OnDamage, AttackCooldown, HurtTime,
    Ping, ItemInSlot, Equipment, GetCount, MaxCount, CommandResult,
    Compare, Equality, Logic, And, Or, Not, Contains, RandomInt, RandomDouble,
    BoolCalc, NumCalc, DirCalc, PosCalc, StringCalc, Vec3, Rot, ToString,
    Ternary, PlaceBlock, BreakBlock, DoingAction,
    FuncDefine, FuncCall, FuncParam,
}

impl BlockType {
    /// Registry index order is the upstream enum declaration order.
    pub const ALL: &'static [BlockType] = &[BlockType::Start, BlockType::OnToggle, BlockType::OnMessage, BlockType::MsgText, BlockType::Move, BlockType::Strafe, BlockType::StopMovement, BlockType::Sneak, BlockType::Sprint, BlockType::Autojump, BlockType::Use, BlockType::Swing, BlockType::Jump, BlockType::AttemptAutojump, BlockType::Attack, BlockType::DropItem, BlockType::SwapHands, BlockType::PickBlock, BlockType::SendMessage, BlockType::StopAction, BlockType::StopAll, BlockType::LookDirection, BlockType::LookCardinal, BlockType::LookAtPos, BlockType::LookAtEntity, BlockType::Turn, BlockType::GotoPos, BlockType::GotoEntity, BlockType::StopPath, BlockType::PathSetting, BlockType::PathMoveType, BlockType::InPath, BlockType::PathReached, BlockType::PathFailed, BlockType::PlaySound, BlockType::StopSound, BlockType::IsSpeaking, BlockType::Bluetooth, BlockType::StopBluetooth, BlockType::IsBluetoothed, BlockType::SelectHotbar, BlockType::OpenInventory, BlockType::CloseScreen, BlockType::Handedness, BlockType::InvClick, BlockType::InvSwapHotbar, BlockType::InvHeldThrow, BlockType::InvHeldDrag, BlockType::QuickLoot, BlockType::RecipeBook, BlockType::InventoryOpen, BlockType::ContainerSize, BlockType::TradeSelect, BlockType::TradeRestock, BlockType::TradeCheck, BlockType::If, BlockType::ElseIf, BlockType::For, BlockType::While, BlockType::LoopIter, BlockType::BlockEnd, BlockType::Break, BlockType::Wait, BlockType::WaitUntil, BlockType::StopScript, BlockType::PauseScript, BlockType::SetScript, BlockType::SetVar, BlockType::ChangeVar, BlockType::ReadVar, BlockType::Scoreboard, BlockType::HasTag, BlockType::Health, BlockType::Position, BlockType::Yaw, BlockType::BlockAt, BlockType::IsTouchingBlock, BlockType::DistanceTo, BlockType::TimeOfDay, BlockType::EveryXTicks, BlockType::OnDamage, BlockType::AttackCooldown, BlockType::HurtTime, BlockType::Ping, BlockType::ItemInSlot, BlockType::Equipment, BlockType::GetCount, BlockType::MaxCount, BlockType::CommandResult, BlockType::Compare, BlockType::Equality, BlockType::Logic, BlockType::And, BlockType::Or, BlockType::Not, BlockType::Contains, BlockType::RandomInt, BlockType::RandomDouble, BlockType::BoolCalc, BlockType::NumCalc, BlockType::DirCalc, BlockType::PosCalc, BlockType::StringCalc, BlockType::Vec3, BlockType::Rot, BlockType::ToString, BlockType::Ternary, BlockType::PlaceBlock, BlockType::BreakBlock, BlockType::DoingAction, BlockType::FuncDefine, BlockType::FuncCall, BlockType::FuncParam,];
    pub fn index(self) -> i32 {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0) as i32
    }
    pub fn from_index(i: i32) -> BlockType {
        Self::ALL.get(i.max(0) as usize).copied().unwrap_or(BlockType::Start)
    }
}

/// Upstream BlockType.valueOf by json name.
pub fn find_block_type(json_name: &str) -> Option<BlockType> {
    BlockType::ALL.iter().copied().find(|t| t.json_name() == json_name)
}

impl BlockType {
    /// Upstream Wire-JSON name: the Java enum name in SCREAMING_SNAKE.
    pub fn json_name(self) -> &'static str {
        let s = match self {
            Self::Start => "START", Self::OnToggle => "ON_TOGGLE", Self::OnMessage => "ON_MESSAGE",
            Self::MsgText => "MSG_TEXT", Self::Move => "MOVE", Self::Strafe => "STRAFE",
            Self::StopMovement => "STOP_MOVEMENT", Self::Sneak => "SNEAK", Self::Sprint => "SPRINT",
            Self::Autojump => "AUTOJUMP", Self::Use => "USE", Self::Swing => "SWING",
            Self::Jump => "JUMP", Self::AttemptAutojump => "ATTEMPT_AUTOJUMP",
            Self::Attack => "ATTACK", Self::DropItem => "DROP_ITEM",
            Self::SwapHands => "SWAP_HANDS", Self::PickBlock => "PICK_BLOCK",
            Self::SendMessage => "SEND_MESSAGE", Self::StopAction => "STOP_ACTION",
            Self::StopAll => "STOP_ALL", Self::LookDirection => "LOOK_DIRECTION",
            Self::LookCardinal => "LOOK_CARDINAL", Self::LookAtPos => "LOOK_AT_POS",
            Self::LookAtEntity => "LOOK_AT_ENTITY", Self::Turn => "TURN",
            Self::GotoPos => "GOTO_POS", Self::GotoEntity => "GOTO_ENTITY",
            Self::StopPath => "STOP_PATH", Self::PathSetting => "PATH_SETTING",
            Self::PathMoveType => "PATH_MOVE_TYPE", Self::InPath => "IN_PATH",
            Self::PathReached => "PATH_REACHED", Self::PathFailed => "PATH_FAILED",
            Self::PlaySound => "PLAY_SOUND", Self::StopSound => "STOP_SOUND",
            Self::IsSpeaking => "IS_SPEAKING", Self::Bluetooth => "BLUETOOTH",
            Self::StopBluetooth => "STOP_BLUETOOTH", Self::IsBluetoothed => "IS_BLUETOOTHED",
            Self::SelectHotbar => "SELECT_HOTBAR", Self::OpenInventory => "OPEN_INVENTORY",
            Self::CloseScreen => "CLOSE_SCREEN", Self::Handedness => "HANDEDNESS",
            Self::InvClick => "INV_CLICK", Self::InvSwapHotbar => "INV_SWAP_HOTBAR",
            Self::InvHeldThrow => "INV_HELD_THROW", Self::InvHeldDrag => "INV_HELD_DRAG",
            Self::QuickLoot => "QUICK_LOOT", Self::RecipeBook => "RECIPE_BOOK",
            Self::InventoryOpen => "INVENTORY_OPEN", Self::ContainerSize => "CONTAINER_SIZE",
            Self::TradeSelect => "TRADE_SELECT", Self::TradeRestock => "TRADE_RESTOCK",
            Self::TradeCheck => "TRADE_CHECK", Self::If => "IF", Self::ElseIf => "ELSE_IF",
            Self::For => "FOR", Self::While => "WHILE", Self::LoopIter => "LOOP_ITER",
            Self::BlockEnd => "BLOCK_END", Self::Break => "BREAK", Self::Wait => "WAIT",
            Self::WaitUntil => "WAIT_UNTIL", Self::StopScript => "STOP_SCRIPT",
            Self::PauseScript => "PAUSE_SCRIPT", Self::SetScript => "SET_SCRIPT",
            Self::SetVar => "SET_VAR", Self::ChangeVar => "CHANGE_VAR", Self::ReadVar => "READ_VAR",
            Self::Scoreboard => "SCOREBOARD", Self::HasTag => "HAS_TAG", Self::Health => "HEALTH",
            Self::Position => "POSITION", Self::Yaw => "YAW", Self::BlockAt => "BLOCK_AT",
            Self::IsTouchingBlock => "IS_TOUCHING_BLOCK", Self::DistanceTo => "DISTANCE_TO",
            Self::TimeOfDay => "TIME_OF_DAY", Self::EveryXTicks => "EVERY_X_TICKS",
            Self::OnDamage => "ON_DAMAGE", Self::AttackCooldown => "ATTACK_COOLDOWN",
            Self::HurtTime => "HURT_TIME", Self::Ping => "PING",
            Self::ItemInSlot => "ITEM_IN_SLOT", Self::Equipment => "EQUIPMENT",
            Self::GetCount => "GET_COUNT", Self::MaxCount => "MAX_COUNT",
            Self::CommandResult => "COMMAND_RESULT", Self::Compare => "COMPARE",
            Self::Equality => "EQUALITY", Self::Logic => "LOGIC", Self::And => "AND",
            Self::Or => "OR", Self::Not => "NOT", Self::Contains => "CONTAINS",
            Self::RandomInt => "RANDOM_INT", Self::RandomDouble => "RANDOM_DOUBLE",
            Self::BoolCalc => "BOOL_CALC", Self::NumCalc => "NUM_CALC",
            Self::DirCalc => "DIR_CALC", Self::PosCalc => "POS_CALC",
            Self::StringCalc => "STRING_CALC", Self::Vec3 => "VEC3", Self::Rot => "ROT",
            Self::ToString => "TO_STRING", Self::Ternary => "TERNARY",
            Self::PlaceBlock => "PLACE_BLOCK", Self::BreakBlock => "BREAK_BLOCK",
            Self::DoingAction => "DOING_ACTION", Self::FuncDefine => "FUNC_DEFINE",
            Self::FuncCall => "FUNC_CALL", Self::FuncParam => "FUNC_PARAM",
        };
        s
    }

    /// Upstream OWNER_REFS: LoopIter, FuncParam, MsgText.
    pub fn refs_owner(self) -> bool {
        matches!(self, Self::LoopIter | Self::FuncParam | Self::MsgText)
    }
}

#[derive(Clone, Debug)]
pub struct BlockDef {
    pub ty: BlockType,
    pub category: BlockCategory,
    pub shape: BlockShape,
    pub label: &'static str,
    pub out_ports: i32,
    pub params: Vec<ParamSlot>,
}

impl BlockDef {
    pub fn has_input(&self) -> bool {
        !matches!(self.shape, BlockShape::Hat | BlockShape::Reporter | BlockShape::Boolean)
    }
    pub fn is_reporter(&self) -> bool {
        matches!(self.shape, BlockShape::Reporter | BlockShape::Boolean)
    }
    pub fn param_slot(&self, name: &str) -> Option<&ParamSlot> {
        self.params.iter().find(|p| p.name == name)
    }
}

/// Upstream BlockDefRegistry.get. Upstream throws IllegalArgumentException
/// on an unknown type; here an unknown type resolves to None and callers
/// fall back gracefully, like upstream on a null def.
pub fn def_for(ty: BlockType) -> Option<&'static BlockDef> {
    super::block_def_registry::defs().get(&ty)
}
}
pub mod block_def_registry {
// HeroBot BlockDefRegistry ported 1:1 from HerobaneNair/HeRoBot
// (herobot-common/.../ai/block/BlockDefRegistry.java, 358 lines), CC-BY-NC-4.0.
// One BlockDef per BlockType with upstream labels, categories, shapes,
// out-port counts and param slots, in upstream registration order.
// Upstream's static EnumMap becomes a LazyLock HashMap. get()'s
// IllegalArgumentException maps to def_for() returning None, which callers
// already fall back on like upstream on a null def.

use super::block_registry::{BlockCategory, BlockDef, BlockShape, BlockType, ParamSlot, ParamType};
use super::sensor::ITEM_SLOT_NAMES;
use std::collections::HashMap;
use std::sync::LazyLock;

const ACTION_MODES: &[&str] = &["once", "continuous", "interval"];
const TWICE_MODES: &[&str] = &["once", "twice", "continuous", "interval"];
const DIRECTIONS: &[&str] = &["north", "south", "east", "west", "up", "down"];
const LOOK_AT_MODES: &[&str] = &["eyes", "closest", "feet"];
const DISTANCE_SHAPES: &[&str] = &["position", "hitbox"];
const DISTANCE_MODES: &[&str] = &["normal", "horizontal", "vertical"];
const MOVE_TYPES: &[&str] = &["WALK", "SPRINT", "SPRINT_JUMP"];
const ACTION_TYPES: &[&str] =
    &["USE", "SWING", "JUMP", "ATTACK", "DROP_ITEM", "DROP_STACK", "SWAP_HANDS"];
const HANDEDNESS: &[&str] = &["left", "right"];
const DROP_AMOUNTS: &[&str] = &["1", "stack"];
const PICK_BLOCK_DATA: &[&str] = &["no data", "with data"];
const DOING_ACTIONS: &[&str] = &[
    "walking", "strafing", "sprinting", "sneaking", "using", "attacking", "swinging", "jumping",
    "dropping", "swapping",
];
const COMPARATORS: &[&str] = &["<", ">", "\u{2264}", "\u{2265}"];
const EQUALITY_OPS: &[&str] = &["=", "\u{2260}"];
const LOGIC_OPS: &[&str] = &["and", "or", "xor"];
const HOTBAR_SLOTS: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9"];
const SWAP_TARGETS: &[&str] = &["offhand", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
const RECIPE_MODES: &[&str] = &["single", "max"];
const COUNT_KINDS: &[&str] = &["count", "durability"];
const COOLDOWN_KINDS: &[&str] = &["fraction", "ticksLeft"];
const MOVE_DIRS: &[&str] = &["forward", "backward", "stop"];
const STRAFE_DIRS: &[&str] = &["left", "right", "stop"];
const PATH_SETTINGS: &[&str] = &[
    "maxHorizontalDistance", "maxVerticalDistance",
    "nodeHorizontalDistance", "nodeVerticalDistance",
    "horizontalMoveCost", "verticalMoveCost", "swimCostMultiplier",
    "stopFollowing", "debug",
];
const MENUS: &[&str] = &["inventory", "container"];
const CLICK_MODES: &[&str] = &["click", "rightClick", "shiftClick", "throw", "throwAll"];
const DRAG_BUTTONS: &[&str] = &["left", "right"];
const LOOT_SOURCES: &[&str] = &["container", "inventory"];
const AXES: &[&str] = &["yaw", "pitch"];
const EQUIP_SLOTS: &[&str] = &[
    "weapon.mainhand", "weapon.offhand",
    "armor.head", "armor.chest", "armor.legs", "armor.feet",
];
/// Upstream FACES is public on the registry.
pub const FACES: &[&str] = &["north", "south", "east", "west", "up", "down", "any"];

fn def(
    ty: BlockType,
    category: BlockCategory,
    shape: BlockShape,
    label: &'static str,
    out_ports: i32,
    params: Vec<ParamSlot>,
) -> BlockDef {
    BlockDef { ty, category, shape, label, out_ports, params }
}

fn stmt(ty: BlockType, category: BlockCategory, label: &'static str, params: Vec<ParamSlot>) -> BlockDef {
    def(ty, category, BlockShape::Statement, label, 1, params)
}

fn action_block(ty: BlockType, label: &'static str, modes: &[&'static str]) -> BlockDef {
    def(
        ty,
        BlockCategory::Action,
        BlockShape::Statement,
        label,
        1,
        vec![
            ParamSlot::of_enum("mode", modes.to_vec(), "once"),
            ParamSlot::of_int("interval", 1),
            ParamSlot::of_int("ticks", 0),
        ],
    )
}

fn reporter(ty: BlockType, category: BlockCategory, label: &'static str, params: Vec<ParamSlot>) -> BlockDef {
    def(ty, category, BlockShape::Reporter, label, 0, params)
}

fn bool_block(ty: BlockType, category: BlockCategory, label: &'static str, params: Vec<ParamSlot>) -> BlockDef {
    def(ty, category, BlockShape::Boolean, label, 0, params)
}

fn enum_slot(name: &'static str, choices: &[&'static str], default: &'static str) -> ParamSlot {
    ParamSlot::of_enum(name, choices.to_vec(), default)
}

fn uuid_slot(name: &'static str, default: &'static str) -> ParamSlot {
    ParamSlot::of(name, ParamType::Uuid, default)
}

/// Upstream static { start(); register(...); ... } initializer, in order.
fn register_all() -> HashMap<BlockType, BlockDef> {
    let mut v: Vec<BlockDef> = Vec::new();

    // start()
    v.push(def(BlockType::Start, BlockCategory::Event, BlockShape::Hat, "start", 1, vec![]));

    v.push(def(BlockType::OnToggle, BlockCategory::Event, BlockShape::Hat, "when toggled true", 1,
        vec![ParamSlot::of_bool("condition", false)]));
    v.push(def(BlockType::OnMessage, BlockCategory::Event, BlockShape::Hat, "on message", 1, vec![]));
    v.push(reporter(BlockType::MsgText, BlockCategory::Event, "message", vec![]));

    v.push(stmt(BlockType::Move, BlockCategory::Motion, "walk",
        vec![enum_slot("direction", MOVE_DIRS, "forward")]));
    v.push(stmt(BlockType::Strafe, BlockCategory::Motion, "strafe",
        vec![enum_slot("direction", STRAFE_DIRS, "left")]));
    v.push(stmt(BlockType::StopMovement, BlockCategory::Motion, "stop movement", vec![]));
    v.push(stmt(BlockType::Sneak, BlockCategory::Motion, "set sneaking",
        vec![ParamSlot::of_bool("value", true)]));
    v.push(stmt(BlockType::Sprint, BlockCategory::Motion, "set sprinting",
        vec![ParamSlot::of_bool("value", true)]));
    v.push(stmt(BlockType::Autojump, BlockCategory::Motion, "set auto-jump",
        vec![ParamSlot::of_bool("value", true)]));

    v.push(action_block(BlockType::Use, "use", TWICE_MODES));
    v.push(action_block(BlockType::Swing, "swing", ACTION_MODES));
    v.push(action_block(BlockType::Jump, "jump", ACTION_MODES));
    v.push(stmt(BlockType::AttemptAutojump, BlockCategory::Action, "auto jump", vec![]));
    v.push(action_block(BlockType::Attack, "attack", TWICE_MODES));
    v.push(def(BlockType::DropItem, BlockCategory::Action, BlockShape::Statement, "drop", 1,
        vec![
            enum_slot("amount", DROP_AMOUNTS, "1"),
            enum_slot("mode", ACTION_MODES, "once"),
            ParamSlot::of_int("interval", 1),
            ParamSlot::of_int("ticks", 0),
        ]));
    v.push(action_block(BlockType::SwapHands, "swap hands", ACTION_MODES));
    v.push(stmt(BlockType::PickBlock, BlockCategory::Action, "pick block",
        vec![enum_slot("data", PICK_BLOCK_DATA, "no data")]));
    v.push(stmt(BlockType::SendMessage, BlockCategory::Action, "send message",
        vec![ParamSlot::of_string("message", "Hello World")]));
    v.push(stmt(BlockType::StopAction, BlockCategory::Action, "stop action",
        vec![enum_slot("action", ACTION_TYPES, "USE")]));
    v.push(stmt(BlockType::StopAll, BlockCategory::Action, "stop all actions", vec![]));
    v.push(stmt(BlockType::PlaceBlock, BlockCategory::Action, "place",
        vec![
            ParamSlot::of_pos("position"),
            enum_slot("face", FACES, "any"),
            ParamSlot::of_bool("force", false),
        ]));
    v.push(stmt(BlockType::BreakBlock, BlockCategory::Action, "mine",
        vec![ParamSlot::of_pos("position"), ParamSlot::of_bool("force", false)]));
    v.push(bool_block(BlockType::DoingAction, BlockCategory::Action, "doing action",
        vec![enum_slot("action", DOING_ACTIONS, "walking")]));

    v.push(stmt(BlockType::LookDirection, BlockCategory::Look, "look direction",
        vec![
            ParamSlot::of_rot("direction"),
            ParamSlot::of_double("yawOffset", 0.0),
            ParamSlot::of_double("pitchOffset", 0.0),
            ParamSlot::of_int("ticks", 0),
        ]));
    v.push(stmt(BlockType::LookCardinal, BlockCategory::Look, "look cardinal",
        vec![
            enum_slot("direction", DIRECTIONS, "north"),
            ParamSlot::of_double("yawOffset", 0.0),
            ParamSlot::of_double("pitchOffset", 0.0),
            ParamSlot::of_int("ticks", 0),
        ]));
    v.push(stmt(BlockType::LookAtPos, BlockCategory::Look, "look position",
        vec![
            ParamSlot::of_pos("position"),
            ParamSlot::of_double("yawOffset", 0.0),
            ParamSlot::of_double("pitchOffset", 0.0),
            ParamSlot::of_int("ticks", 0),
        ]));
    v.push(stmt(BlockType::LookAtEntity, BlockCategory::Look, "look entity",
        vec![
            ParamSlot::of_uuid("target"),
            enum_slot("mode", LOOK_AT_MODES, "eyes"),
            ParamSlot::of_double("yawOffset", 0.0),
            ParamSlot::of_double("pitchOffset", 0.0),
            ParamSlot::of_int("ticks", 0),
        ]));
    v.push(stmt(BlockType::Turn, BlockCategory::Look, "turn",
        vec![
            ParamSlot::of_double("dYaw", 0.0),
            ParamSlot::of_double("dPitch", 0.0),
            ParamSlot::of_double("yawOffset", 0.0),
            ParamSlot::of_double("pitchOffset", 0.0),
            ParamSlot::of_int("ticks", 0),
        ]));

    v.push(stmt(BlockType::GotoPos, BlockCategory::Path, "path pos",
        vec![ParamSlot::of_pos("position")]));
    v.push(stmt(BlockType::GotoEntity, BlockCategory::Path, "path entity",
        vec![ParamSlot::of_uuid("target")]));
    v.push(stmt(BlockType::StopPath, BlockCategory::Path, "stop pathing", vec![]));
    v.push(stmt(BlockType::PathSetting, BlockCategory::Path, "path setting",
        vec![enum_slot("key", PATH_SETTINGS, "horizontalMoveCost"), ParamSlot::of_string("value", "1.0")]));
    v.push(stmt(BlockType::PathMoveType, BlockCategory::Path, "path move type",
        vec![enum_slot("type", MOVE_TYPES, "WALK")]));
    v.push(bool_block(BlockType::InPath, BlockCategory::Path, "in path", vec![]));
    v.push(bool_block(BlockType::PathReached, BlockCategory::Path, "reached end", vec![]));
    v.push(bool_block(BlockType::PathFailed, BlockCategory::Path, "failed path", vec![]));

    v.push(stmt(BlockType::PlaySound, BlockCategory::Voice, "play sound",
        vec![ParamSlot::of_string("sound", "hello.wav"), ParamSlot::of_bool("loop", false)]));
    v.push(stmt(BlockType::StopSound, BlockCategory::Voice, "stop sound", vec![]));
    v.push(bool_block(BlockType::IsSpeaking, BlockCategory::Voice, "is speaking", vec![]));
    v.push(stmt(BlockType::Bluetooth, BlockCategory::Voice, "bluetooth",
        vec![ParamSlot::of_uuid("source")]));
    v.push(stmt(BlockType::StopBluetooth, BlockCategory::Voice, "stop bluetooth", vec![]));
    v.push(bool_block(BlockType::IsBluetoothed, BlockCategory::Voice, "is bluetoothed", vec![]));

    v.push(stmt(BlockType::SelectHotbar, BlockCategory::Inventory, "hotbar",
        vec![enum_slot("slot", HOTBAR_SLOTS, "1")]));
    v.push(stmt(BlockType::OpenInventory, BlockCategory::Inventory, "open inventory", vec![]));
    v.push(stmt(BlockType::CloseScreen, BlockCategory::Inventory, "close screen", vec![]));
    v.push(stmt(BlockType::Handedness, BlockCategory::Look, "handedness",
        vec![enum_slot("side", HANDEDNESS, "right")]));
    v.push(stmt(BlockType::InvClick, BlockCategory::Inventory, "menu click",
        vec![
            enum_slot("menu", MENUS, "inventory"),
            enum_slot("mode", CLICK_MODES, "click"),
            ParamSlot::of_int("slot", 0),
        ]));
    v.push(stmt(BlockType::InvSwapHotbar, BlockCategory::Inventory, "swap slot",
        vec![
            enum_slot("menu", MENUS, "inventory"),
            ParamSlot::of_int("slot", 0),
            enum_slot("with", SWAP_TARGETS, "1"),
        ]));
    v.push(stmt(BlockType::InvHeldThrow, BlockCategory::Inventory, "throw held",
        vec![enum_slot("menu", MENUS, "inventory")]));
    v.push(stmt(BlockType::InvHeldDrag, BlockCategory::Inventory, "drag held",
        vec![
            enum_slot("menu", MENUS, "inventory"),
            enum_slot("button", DRAG_BUTTONS, "left"),
            ParamSlot::of_string("slots", "1,2,3"),
        ]));
    v.push(stmt(BlockType::QuickLoot, BlockCategory::Inventory, "quick loot",
        vec![enum_slot("from", LOOT_SOURCES, "container"), ParamSlot::of_int("slot", 0)]));
    v.push(stmt(BlockType::RecipeBook, BlockCategory::Inventory, "place recipe",
        vec![
            ParamSlot::of_item("item", "minecraft:crafting_table"),
            enum_slot("mode", RECIPE_MODES, "single"),
        ]));
    v.push(reporter(BlockType::InventoryOpen, BlockCategory::Inventory, "menu open", vec![]));
    v.push(reporter(BlockType::ContainerSize, BlockCategory::Inventory, "container size", vec![]));
    v.push(stmt(BlockType::TradeSelect, BlockCategory::Inventory, "trade select",
        vec![ParamSlot::of_int("index", 1)]));
    v.push(stmt(BlockType::TradeRestock, BlockCategory::Inventory, "trade restock", vec![]));
    v.push(reporter(BlockType::TradeCheck, BlockCategory::Inventory, "trade check",
        vec![ParamSlot::of_int("index", 0)]));

    v.push(def(BlockType::If, BlockCategory::Control, BlockShape::Statement, "if", 2,
        vec![ParamSlot::of_bool("condition", false)]));
    v.push(def(BlockType::ElseIf, BlockCategory::Control, BlockShape::Statement, "else (if)", 2,
        vec![ParamSlot::of_bool("condition", true)]));
    v.push(def(BlockType::For, BlockCategory::Control, BlockShape::Statement, "for", 1,
        vec![ParamSlot::of_int("count", 10)]));
    v.push(def(BlockType::While, BlockCategory::Control, BlockShape::Statement, "while", 1,
        vec![ParamSlot::of_bool("condition", true)]));
    v.push(reporter(BlockType::LoopIter, BlockCategory::Control, "i", vec![]));
    v.push(def(BlockType::BlockEnd, BlockCategory::Control, BlockShape::CEnd, "end", 1, vec![]));
    v.push(def(BlockType::Break, BlockCategory::Control, BlockShape::Statement, "break", 0,
        vec![ParamSlot::of_int("count", 1)]));
    v.push(stmt(BlockType::Wait, BlockCategory::Control, "wait",
        vec![ParamSlot::of_int("ticks", 20)]));
    v.push(stmt(BlockType::WaitUntil, BlockCategory::Control, "wait until",
        vec![ParamSlot::of_bool("condition", false)]));
    v.push(def(BlockType::StopScript, BlockCategory::Event, BlockShape::Statement, "stop", 0, vec![]));
    v.push(stmt(BlockType::PauseScript, BlockCategory::Event, "pause", vec![]));
    v.push(def(BlockType::SetScript, BlockCategory::Event, BlockShape::Statement, "run script", 0,
        vec![ParamSlot::of_string("script", "")]));

    v.push(stmt(BlockType::SetVar, BlockCategory::Variable, "set",
        vec![ParamSlot::of_var_ref("name", ""), ParamSlot::of_double("value", 0.0)]));
    v.push(stmt(BlockType::ChangeVar, BlockCategory::Variable, "change",
        vec![ParamSlot::of_var_ref("name", ""), ParamSlot::of_double("delta", 1.0)]));
    v.push(reporter(BlockType::ReadVar, BlockCategory::Variable, "var",
        vec![ParamSlot::of_var_ref("name", "")]));

    v.push(reporter(BlockType::Scoreboard, BlockCategory::Sensor, "scoreboard",
        vec![ParamSlot::of_string("objective", "score"), uuid_slot("target", "@s")]));
    v.push(bool_block(BlockType::HasTag, BlockCategory::Sensor, "has tag",
        vec![uuid_slot("target", "@s"), ParamSlot::of_string("tag", "tag")]));
    v.push(reporter(BlockType::Health, BlockCategory::Sensor, "health", vec![]));
    v.push(reporter(BlockType::Position, BlockCategory::Sensor, "position", vec![]));
    v.push(reporter(BlockType::Yaw, BlockCategory::Sensor, "",
        vec![enum_slot("axis", AXES, "yaw")]));
    v.push(reporter(BlockType::BlockAt, BlockCategory::Sensor, "block at",
        vec![ParamSlot::of_pos("position")]));
    v.push(bool_block(BlockType::IsTouchingBlock, BlockCategory::Sensor, "is touching block",
        vec![ParamSlot::of_string("block", "minecraft:water")]));
    v.push(reporter(BlockType::DistanceTo, BlockCategory::Sensor, "distance from",
        vec![
            enum_slot("fromShape", DISTANCE_SHAPES, "position"),
            enum_slot("toShape", DISTANCE_SHAPES, "position"),
            ParamSlot::of_uuid("target"),
            enum_slot("mode", DISTANCE_MODES, "normal"),
        ]));
    v.push(reporter(BlockType::TimeOfDay, BlockCategory::Sensor, "time of day", vec![]));

    v.push(bool_block(BlockType::EveryXTicks, BlockCategory::Sensor, "every",
        vec![ParamSlot::of_int("ticks", 1)]));
    v.push(bool_block(BlockType::OnDamage, BlockCategory::Sensor, "on damage", vec![]));
    v.push(reporter(BlockType::AttackCooldown, BlockCategory::Sensor, "attack cooldown",
        vec![enum_slot("kind", COOLDOWN_KINDS, "fraction")]));
    v.push(reporter(BlockType::HurtTime, BlockCategory::Sensor, "hurt time", vec![]));
    v.push(reporter(BlockType::Ping, BlockCategory::Sensor, "ping", vec![]));

    v.push(reporter(BlockType::ItemInSlot, BlockCategory::Inventory, "inventorySlot",
        vec![ParamSlot::of_int("slot", 0)]));
    v.push(reporter(BlockType::Equipment, BlockCategory::Inventory, "equipment",
        vec![uuid_slot("target", "@s"), enum_slot("slot", EQUIP_SLOTS, "weapon.mainhand")]));
    v.push(reporter(BlockType::GetCount, BlockCategory::Inventory, "get",
        vec![
            enum_slot("kind", COUNT_KINDS, "count"),
            enum_slot("slot", &ITEM_SLOT_NAMES, "weapon.mainhand"),
        ]));
    v.push(reporter(BlockType::MaxCount, BlockCategory::Inventory, "max",
        vec![
            enum_slot("kind", COUNT_KINDS, "count"),
            ParamSlot::of_item("item", "minecraft:diamond_sword"),
        ]));
    v.push(reporter(BlockType::CommandResult, BlockCategory::Sensor, "command result",
        vec![ParamSlot::of_string("command", "data get entity @s Health")]));

    v.push(reporter(BlockType::NumCalc, BlockCategory::DataManipulation, "num calc",
        vec![ParamSlot::of_string("expression", "0")]));
    v.push(reporter(BlockType::StringCalc, BlockCategory::DataManipulation, "String calc",
        vec![ParamSlot::of_string("expression", "\"\"")]));
    v.push(bool_block(BlockType::BoolCalc, BlockCategory::DataManipulation, "bool calc",
        vec![ParamSlot::of_string("expression", "true")]));
    v.push(reporter(BlockType::PosCalc, BlockCategory::DataManipulation, "Pos calc",
        vec![ParamSlot::of_string("expression", "pos(0,0,0)")]));
    v.push(reporter(BlockType::DirCalc, BlockCategory::DataManipulation, "Dir calc",
        vec![ParamSlot::of_string("expression", "dir(0,0)")]));
    v.push(reporter(BlockType::Ternary, BlockCategory::Operator, "if",
        vec![
            ParamSlot::of_bool("condition", false),
            ParamSlot::of_int("trueValue", 0),
            ParamSlot::of_int("falseValue", 0),
        ]));
    v.push(bool_block(BlockType::Compare, BlockCategory::Operator, "compare",
        vec![
            ParamSlot::of_double("a", 0.0),
            enum_slot("op", COMPARATORS, "<"),
            ParamSlot::of_double("b", 0.0),
        ]));
    v.push(bool_block(BlockType::Equality, BlockCategory::Operator, "equals",
        vec![
            ParamSlot::of_double("a", 0.0),
            enum_slot("op", EQUALITY_OPS, "="),
            ParamSlot::of_double("b", 0.0),
        ]));
    v.push(bool_block(BlockType::Logic, BlockCategory::Operator, "logic",
        vec![
            ParamSlot::of_bool("a", false),
            enum_slot("op", LOGIC_OPS, "and"),
            ParamSlot::of_bool("b", false),
        ]));
    v.push(bool_block(BlockType::Not, BlockCategory::Operator, "not",
        vec![ParamSlot::of_bool("a", false)]));
    v.push(bool_block(BlockType::Contains, BlockCategory::Operator, "contains",
        vec![
            ParamSlot::of_string("a", ""),
            ParamSlot::of_string("b", ""),
            ParamSlot::of_bool("checkCase", false),
        ]));
    v.push(bool_block(BlockType::And, BlockCategory::Operator, "and",
        vec![ParamSlot::of_bool("a", false), ParamSlot::of_bool("b", false)]));
    v.push(bool_block(BlockType::Or, BlockCategory::Operator, "or",
        vec![ParamSlot::of_bool("a", false), ParamSlot::of_bool("b", false)]));
    v.push(reporter(BlockType::RandomInt, BlockCategory::Operator, "random int",
        vec![ParamSlot::of_int("min", 0), ParamSlot::of_int("max", 10)]));
    v.push(reporter(BlockType::RandomDouble, BlockCategory::Operator, "random double",
        vec![ParamSlot::of_double("min", 0.0), ParamSlot::of_double("max", 1.0)]));

    v.push(reporter(BlockType::Vec3, BlockCategory::DataManipulation, "make position",
        vec![
            ParamSlot::of_double("x", 0.0),
            ParamSlot::of_double("y", 0.0),
            ParamSlot::of_double("z", 0.0),
        ]));
    v.push(reporter(BlockType::Rot, BlockCategory::DataManipulation, "make direction",
        vec![ParamSlot::of_double("yaw", 0.0), ParamSlot::of_double("pitch", 0.0)]));
    v.push(reporter(BlockType::ToString, BlockCategory::DataManipulation, "toString",
        vec![ParamSlot::of_string("value", "")]));

    v.push(def(BlockType::FuncDefine, BlockCategory::Functions, BlockShape::Hat, "define", 1,
        vec![enum_slot("name", &[], "")]));
    v.push(stmt(BlockType::FuncCall, BlockCategory::Functions, "call",
        vec![enum_slot("name", &[], "")]));
    v.push(reporter(BlockType::FuncParam, BlockCategory::Functions, "input",
        vec![ParamSlot::of_string("func", ""), ParamSlot::of_int("index", 0)]));

    let mut map: HashMap<BlockType, BlockDef> = HashMap::new();
    for d in v {
        map.insert(d.ty, d);
    }
    map
}

/// Upstream BlockDefRegistry.DEFS.
pub fn defs() -> &'static HashMap<BlockType, BlockDef> {
    static DEFS: LazyLock<HashMap<BlockType, BlockDef>> = LazyLock::new(register_all);
    &DEFS
}

/// Upstream BlockDefRegistry.all(): unmodifiable list in enum order.
pub fn all() -> Vec<&'static BlockDef> {
    BlockType::ALL.iter().filter_map(|t| defs().get(t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::block_registry::find_block_type;

    #[test]
    fn every_block_type_has_a_def() {
        assert_eq!(defs().len(), 113, "registry size matches upstream's 113 block types");
        for t in BlockType::ALL {
            assert!(defs().contains_key(t), "missing BlockDef for {}", t.json_name());
        }
    }

    #[test]
    fn spot_checks_match_upstream() {
        let wait = defs().get(&find_block_type("WAIT").unwrap()).unwrap();
        assert_eq!(wait.label, "wait");
        assert_eq!(wait.category, BlockCategory::Control);
        assert_eq!(wait.out_ports, 1);
        let ticks = wait.param_slot("ticks").unwrap();
        assert_eq!(ticks.ty, ParamType::Int);
        assert_eq!(ticks.default_value.as_deref(), Some("20"));

        let yaw = defs().get(&find_block_type("YAW").unwrap()).unwrap();
        assert_eq!(yaw.label, "");
        assert_eq!(yaw.shape, BlockShape::Reporter);

        let scoreboard = defs().get(&find_block_type("SCOREBOARD").unwrap()).unwrap();
        let target = scoreboard.param_slot("target").unwrap();
        assert_eq!(target.ty, ParamType::Uuid);
        assert_eq!(target.default_value.as_deref(), Some("@s"));

        let get_count = defs().get(&find_block_type("GET_COUNT").unwrap()).unwrap();
        let slot = get_count.param_slot("slot").unwrap();
        assert_eq!(slot.enum_choices, ITEM_SLOT_NAMES.to_vec());
        assert_eq!(slot.default_value.as_deref(), Some("weapon.mainhand"));

        let if_def = defs().get(&find_block_type("IF").unwrap()).unwrap();
        assert_eq!(if_def.out_ports, 2);
        assert_eq!(if_def.param_slot("condition").unwrap().default_value.as_deref(), Some("false"));

        let string_calc = defs().get(&find_block_type("STRING_CALC").unwrap()).unwrap();
        assert_eq!(string_calc.param_slot("expression").unwrap().default_value.as_deref(), Some("\"\""));

        let use_def = defs().get(&find_block_type("USE").unwrap()).unwrap();
        assert_eq!(use_def.param_slot("mode").unwrap().enum_choices, TWICE_MODES.to_vec());
        assert_eq!(use_def.param_slot("ticks").unwrap().default_value.as_deref(), Some("0"));

        let func_define = defs().get(&find_block_type("FUNC_DEFINE").unwrap()).unwrap();
        assert_eq!(func_define.shape, BlockShape::Hat);
        assert!(func_define.param_slot("name").unwrap().enum_choices.is_empty());

        let faces = defs().get(&find_block_type("PLACE_BLOCK").unwrap()).unwrap();
        assert_eq!(faces.param_slot("face").unwrap().enum_choices, FACES.to_vec());
    }
}
}
pub mod bool_eval {
// HeroBot AI runtime bool-expression engine, ported 1:1 from
// herobot-common hero.bane.herobot.common.ai.expr.BoolEval (CC-BY-NC-4.0).
// Clean-room port of grammar and semantics from the upstream Java we read.
//
// Grammar (upstream): or/|| < xor < and/&& < unary (!, not(...)) < primary
// ((expr) | operand [cmpOp operand]*). Operands: quoted strings, numeric
// sub-expressions (the ExprEval grammar, so {var}[idx] and Input<n> refs
// and arithmetic work inside), {var} refs, true/false, and Input<n> names.
// A bare operand is truthy via as_bool; comparison chains conjoin left to
// right. A parse error compiles to FALSE and logs once, like upstream.

use super::expr::{
    canonical_input, is_input_ref, log_bad_expression, parse_numeric, Expr as NumExpr, ParamKind,
};

/// Upstream hands the vars function raw Java Objects; BoolVal stands in for
/// the shapes the ported runtime produces. Arr is upstream float[]/double[]
/// (our Rot), Other is any other non-null object carried by its
/// String.valueOf form (our Vec3).
#[derive(Clone, Debug)]
pub enum BoolVal {
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<f64>),
    Other(String),
}

pub type VarsFn<'a> = dyn FnMut(&str) -> Option<BoolVal> + 'a;

/// Upstream asBool: Boolean as is, Number non-zero, String parsed as a
/// boolean, any other non-null value true, null false.
pub fn as_bool(v: Option<&BoolVal>) -> bool {
    match v {
        None => false,
        Some(BoolVal::Bool(b)) => *b,
        Some(BoolVal::Num(n)) => *n != 0.0,
        Some(BoolVal::Str(s)) => s.eq_ignore_ascii_case("true"),
        Some(BoolVal::Arr(_)) | Some(BoolVal::Other(_)) => true,
    }
}

fn num_or_len(v: Option<&BoolVal>) -> Option<f64> {
    match v {
        Some(BoolVal::Num(n)) => Some(*n),
        Some(BoolVal::Str(s)) => Some(s.len() as f64),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

fn cmp_num(a: f64, b: f64, op: CmpOp) -> bool {
    match op {
        CmpOp::Gt => a > b,
        CmpOp::Lt => a < b,
        CmpOp::Ge => a >= b,
        CmpOp::Le => a <= b,
        _ => false,
    }
}

/// Upstream compare(): two strings compare textually (or by length for
/// ordering); when both sides yield a Number or a String length, compare
/// numerically; when either side is Boolean, fall back to as_bool
/// equality; otherwise fall back to String.valueOf equality.
pub fn compare(a: Option<BoolVal>, b: Option<BoolVal>, op: CmpOp) -> bool {
    let eq_op = op == CmpOp::Eq;
    let ne_op = op == CmpOp::Ne;
    if let (Some(BoolVal::Str(sa)), Some(BoolVal::Str(sb))) = (a.as_ref(), b.as_ref()) {
        if eq_op {
            return sa == sb;
        }
        if ne_op {
            return sa != sb;
        }
        return cmp_num(sa.len() as f64, sb.len() as f64, op);
    }
    if let (Some(x), Some(y)) = (num_or_len(a.as_ref()), num_or_len(b.as_ref())) {
        if eq_op {
            return x == y;
        }
        if ne_op {
            return x != y;
        }
        return cmp_num(x, y, op);
    }
    if matches!(a.as_ref(), Some(BoolVal::Bool(_)))
        || matches!(b.as_ref(), Some(BoolVal::Bool(_)))
    {
        let ba = as_bool(a.as_ref());
        let bb = as_bool(b.as_ref());
        if eq_op {
            return ba == bb;
        }
        if ne_op {
            return ba != bb;
        }
        return false;
    }
    let eq = values_equal(a.as_ref(), b.as_ref());
    if eq_op {
        eq
    } else if ne_op {
        !eq
    } else {
        false
    }
}

fn values_equal(a: Option<&BoolVal>, b: Option<&BoolVal>) -> bool {
    match (a, b) {
        // Upstream Objects.equals(null, null) is true.
        (None, None) => true,
        (Some(BoolVal::Arr(x)), Some(BoolVal::Arr(y))) => x == y,
        (Some(x), Some(y)) => value_string(x) == value_string(y),
        _ => false,
    }
}

fn value_string(v: &BoolVal) -> String {
    match v {
        BoolVal::Bool(b) => b.to_string(),
        BoolVal::Num(n) => n.to_string(),
        BoolVal::Str(s) => s.clone(),
        BoolVal::Arr(f) => f
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(" "),
        BoolVal::Other(s) => s.clone(),
    }
}

/// Upstream refToDouble: only array values index (our Rot); elsewhere a
/// Boolean is 1/0, a Number its value, a String its length, anything else 0.
pub fn ref_to_double(v: Option<BoolVal>, idx: i32) -> f64 {
    if idx >= 0 {
        return match v {
            Some(BoolVal::Arr(f)) => {
                if (idx as usize) < f.len() {
                    f[idx as usize]
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };
    }
    match v {
        Some(BoolVal::Bool(b)) => {
            if b {
                1.0
            } else {
                0.0
            }
        }
        Some(BoolVal::Num(n)) => n,
        Some(BoolVal::Str(s)) => s.len() as f64,
        _ => 0.0,
    }
}

#[derive(Clone, Debug)]
pub enum OperandKind {
    Var(String),
    Str(String),
    BoolConst(bool),
    Num(NumExpr),
}

#[derive(Clone, Debug)]
pub struct Operand {
    pub kind: OperandKind,
    pub ty: Option<ParamKind>,
}

#[derive(Clone, Debug)]
pub enum BoolExpr {
    False,
    Not(Box<BoolExpr>),
    Or(Box<BoolExpr>, Box<BoolExpr>),
    Xor(Box<BoolExpr>, Box<BoolExpr>),
    And(Box<BoolExpr>, Box<BoolExpr>),
    /// A bare operand, truthy via as_bool.
    Atom(Operand),
    /// compare(a, op, b).
    Cmp(Box<Operand>, CmpOp, Box<Operand>),
}

impl BoolExpr {
    pub fn eval(&self, vars: &mut VarsFn) -> bool {
        match self {
            BoolExpr::False => false,
            BoolExpr::Not(a) => !a.eval(&mut *vars),
            BoolExpr::Or(a, b) => a.eval(&mut *vars) || b.eval(&mut *vars),
            BoolExpr::Xor(a, b) => a.eval(&mut *vars) ^ b.eval(&mut *vars),
            BoolExpr::And(a, b) => a.eval(&mut *vars) && b.eval(&mut *vars),
            BoolExpr::Atom(op) => as_bool(operand_value(op, vars).as_ref()),
            BoolExpr::Cmp(a, o, b) => {
                compare(operand_value(a, vars), operand_value(b, vars), *o)
            }
        }
    }
}

fn operand_value(op: &Operand, vars: &mut VarsFn) -> Option<BoolVal> {
    match &op.kind {
        OperandKind::Var(name) => vars(name),
        OperandKind::Str(s) => Some(BoolVal::Str(s.clone())),
        OperandKind::BoolConst(b) => Some(BoolVal::Bool(*b)),
        OperandKind::Num(e) => {
            let mut sub = |n: &str, idx: i32| ref_to_double(vars(n), idx);
            Some(BoolVal::Num(e.eval(&mut sub)))
        }
    }
}

/// Upstream Parser (BoolEval).
pub struct Parser<'a> {
    src: &'a str,
    chars: Vec<char>,
    strict: bool,
    types: Option<&'a dyn Fn(&str) -> Option<ParamKind>>,
    pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(
        src: &'a str,
        strict: bool,
        types: Option<&'a dyn Fn(&str) -> Option<ParamKind>>,
    ) -> Self {
        Parser {
            src,
            chars: src.chars().collect(),
            strict,
            types,
            pos: 0,
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn lookahead(&self, tok: &str) -> bool {
        let n = tok.chars().count();
        if self.pos + n > self.chars.len() {
            return false;
        }
        self.chars[self.pos..self.pos + n]
            .iter()
            .copied()
            .eq(tok.chars())
    }

    /// Upstream matchWord: whitespace-skipping, case-insensitive word match
    /// that must not be followed by another word character.
    fn match_word(&mut self, w: &str) -> bool {
        self.skip_ws();
        let n = w.len();
        if self.pos + n > self.chars.len() {
            return false;
        }
        for (i, wc) in w.chars().enumerate() {
            let c = self.chars[self.pos + i];
            if !c.eq_ignore_ascii_case(&wc) {
                return false;
            }
        }
        let end = self.pos + n;
        if end < self.chars.len() {
            let c = self.chars[end];
            if c.is_alphanumeric() || c == '_' {
                return false;
            }
        }
        self.pos = end;
        true
    }

    fn match_symbol(&mut self, sym: &str) -> bool {
        self.skip_ws();
        if !self.lookahead(sym) {
            return false;
        }
        self.pos += sym.chars().count();
        true
    }

    pub fn or_expr(&mut self) -> Result<BoolExpr, String> {
        let mut v = self.xor_expr()?;
        loop {
            if self.match_word("or") || self.match_symbol("||") {
                let l = v;
                let r = self.xor_expr()?;
                v = BoolExpr::Or(Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn xor_expr(&mut self) -> Result<BoolExpr, String> {
        let mut v = self.and_expr()?;
        while self.match_word("xor") {
            let l = v;
            let r = self.and_expr()?;
            v = BoolExpr::Xor(Box::new(l), Box::new(r));
        }
        Ok(v)
    }

    fn and_expr(&mut self) -> Result<BoolExpr, String> {
        let mut v = self.unary()?;
        loop {
            if self.match_word("and") || self.match_symbol("&&") {
                let l = v;
                let r = self.unary()?;
                v = BoolExpr::And(Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn unary(&mut self) -> Result<BoolExpr, String> {
        self.skip_ws();
        if self.peek() == Some('!') && !self.lookahead("!=") {
            self.pos += 1;
            let a = self.unary()?;
            return Ok(BoolExpr::Not(Box::new(a)));
        }
        if self.match_word("not") {
            self.skip_ws();
            if self.peek() != Some('(') {
                return Err("not needs (...)".to_string());
            }
            self.pos += 1;
            let a = self.or_expr()?;
            self.skip_ws();
            if self.peek() == Some(')') {
                self.pos += 1;
            } else if self.strict {
                return Err("expected )".to_string());
            }
            return Ok(BoolExpr::Not(Box::new(a)));
        }
        self.primary()
    }

    fn cmp_op(&mut self) -> Option<CmpOp> {
        self.skip_ws();
        if self.lookahead("==") {
            self.pos += 2;
            return Some(CmpOp::Eq);
        }
        if self.lookahead("!=") {
            self.pos += 2;
            return Some(CmpOp::Ne);
        }
        if self.lookahead(">=") {
            self.pos += 2;
            return Some(CmpOp::Ge);
        }
        if self.lookahead("<=") {
            self.pos += 2;
            return Some(CmpOp::Le);
        }
        match self.peek() {
            Some('>') => {
                self.pos += 1;
                Some(CmpOp::Gt)
            }
            Some('<') => {
                self.pos += 1;
                Some(CmpOp::Lt)
            }
            Some('=') => {
                self.pos += 1;
                Some(CmpOp::Eq)
            }
            _ => None,
        }
    }

    fn primary(&mut self) -> Result<BoolExpr, String> {
        self.skip_ws();
        if self.peek() == Some('(') {
            self.pos += 1;
            let v = self.or_expr()?;
            self.skip_ws();
            if self.peek() == Some(')') {
                self.pos += 1;
            } else if self.strict {
                return Err("expected )".to_string());
            }
            return Ok(v);
        }
        let a = self.operand()?;
        let op = match self.cmp_op() {
            Some(o) => o,
            None => {
                if self.strict && self.types.is_some() {
                    if let Some(t) = a.ty {
                        if t != ParamKind::Bool {
                            return Err("not a boolean".to_string());
                        }
                    }
                }
                return Ok(BoolExpr::Atom(a));
            }
        };
        let b = self.operand()?;
        if self.strict && self.types.is_some() {
            self.check_comparable(a.ty, b.ty, op)?;
        }
        let mut chain = BoolExpr::Cmp(Box::new(a), op, Box::new(b.clone()));
        let mut prev = b;
        loop {
            let next_op = match self.cmp_op() {
                Some(o) => o,
                None => break,
            };
            let next = self.operand()?;
            if self.strict && self.types.is_some() {
                self.check_comparable(prev.ty, next.ty, next_op)?;
            }
            let link = BoolExpr::Cmp(Box::new(prev), next_op, Box::new(next.clone()));
            chain = BoolExpr::And(Box::new(chain), Box::new(link));
            prev = next;
        }
        Ok(chain)
    }

    fn check_comparable(
        &self,
        a: Option<ParamKind>,
        b: Option<ParamKind>,
        op: CmpOp,
    ) -> Result<(), String> {
        let (a, b) = match (a, b) {
            (Some(a), Some(b)) => (a, b),
            _ => return Ok(()),
        };
        let a_flex = matches!(a, ParamKind::Num | ParamKind::Str);
        let b_flex = matches!(b, ParamKind::Num | ParamKind::Str);
        if a_flex && b_flex {
            return Ok(());
        }
        let eq_op = matches!(op, CmpOp::Eq | CmpOp::Ne);
        if types_same(a, b) && eq_op {
            return Ok(());
        }
        Err(format!("cannot compare {:?} {:?} {:?}", a, op, b))
    }

    fn operand(&mut self) -> Result<Operand, String> {
        self.skip_ws();
        let c = match self.peek() {
            Some(c) => c,
            None => return Err("expected a value".to_string()),
        };
        if c == '"' {
            return self.quoted();
        }
        let start = self.pos;
        if c == '-' || c == '.' || c.is_ascii_digit() || self.function_call_ahead() {
            return self.numeric(start);
        }
        if c != '{' && !c.is_alphabetic() && c != '_' {
            return Err("expected a value".to_string());
        }
        let a = if c == '{' {
            self.var_ref()?
        } else {
            self.identifier()?
        };
        self.skip_ws();
        if self.arith_op_ahead() {
            self.numeric(start)
        } else {
            Ok(a)
        }
    }

    fn function_call_ahead(&self) -> bool {
        let mut p = self.pos;
        match self.chars.get(p) {
            Some(&c) if c.is_alphabetic() || c == '_' => {}
            _ => return false,
        }
        while let Some(&c) = self.chars.get(p) {
            if c.is_alphanumeric() || c == '_' {
                p += 1;
            } else {
                break;
            }
        }
        while let Some(&c) = self.chars.get(p) {
            if c.is_whitespace() {
                p += 1;
            } else {
                break;
            }
        }
        self.chars.get(p) == Some(&'(')
    }

    fn arith_op_ahead(&self) -> bool {
        matches!(
            self.peek(),
            Some('+') | Some('-') | Some('*') | Some('/') | Some('%') | Some('^')
        )
    }
    fn numeric(&mut self, start: usize) -> Result<Operand, String> {
        let sub = parse_numeric(self.src, start, self.strict, self.types)?;
        self.pos = sub.end;
        Ok(Operand {
            kind: OperandKind::Num(sub.expr),
            ty: Some(ParamKind::Num),
        })
    }

    fn var_ref(&mut self) -> Result<Operand, String> {
        self.pos += 1;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == '}' {
                break;
            }
            self.pos += 1;
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        let name = name.trim().to_string();
        if self.peek() == Some('}') {
            self.pos += 1;
        } else if self.strict {
            return Err("expected }".to_string());
        }
        if self.strict && name.is_empty() {
            return Err("empty variable name".to_string());
        }
        let ty = self.types.and_then(|t| t(&name));
        Ok(Operand {
            kind: OperandKind::Var(name),
            ty,
        })
    }

    fn quoted(&mut self) -> Result<Operand, String> {
        self.pos += 1;
        let mut sb = String::new();
        while let Some(c) = self.peek() {
            if c == '"' {
                break;
            }
            if c == '\\' && self.pos + 1 < self.chars.len() {
                self.pos += 1;
                sb.push(self.chars[self.pos]);
            } else {
                sb.push(c);
            }
            self.pos += 1;
        }
        if self.peek() == Some('"') {
            self.pos += 1;
        } else if self.strict {
            return Err("unclosed string".to_string());
        }
        Ok(Operand {
            kind: OperandKind::Str(sb),
            ty: Some(ParamKind::Str),
        })
    }

    fn identifier(&mut self) -> Result<Operand, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        if name.eq_ignore_ascii_case("true") {
            return Ok(Operand {
                kind: OperandKind::BoolConst(true),
                ty: Some(ParamKind::Bool),
            });
        }
        if name.eq_ignore_ascii_case("false") {
            return Ok(Operand {
                kind: OperandKind::BoolConst(false),
                ty: Some(ParamKind::Bool),
            });
        }
        if is_input_ref(&name) {
            let canonical = canonical_input(&name);
            let ty = self.types.and_then(|t| t(&canonical));
            return Ok(Operand {
                kind: OperandKind::Var(canonical),
                ty,
            });
        }
        Err(format!("unknown name: {}", name))
    }

    fn parse_full(&mut self) -> Result<BoolExpr, String> {
        let e = self.or_expr()?;
        self.skip_ws();
        if self.at_end() {
            Ok(e)
        } else {
            Err("unexpected trailing input".to_string())
        }
    }
}

fn types_same(a: ParamKind, b: ParamKind) -> bool {
    if a == b {
        return true;
    }
    matches!(a, ParamKind::Num) && matches!(b, ParamKind::Num)
}

/// Upstream eval(): compile (cached) then evaluate.
pub fn eval(expression: &str, vars: &mut VarsFn) -> bool {
    compile(expression).eval(vars)
}

/// Upstream compile(): blank strings and parse errors compile to FALSE and
/// the error is logged once; results are memoized (limit 256, cleared).
pub fn compile(expression: &str) -> BoolExpr {
    if expression.trim().is_empty() {
        return BoolExpr::False;
    }
    if let Some(hit) = CACHE.with(|c| c.borrow_mut().get(expression).cloned()) {
        return hit;
    }
    let compiled = match Parser::new(expression, true, None).parse_full() {
        Ok(e) => e,
        Err(reason) => {
            log_bad_expression(expression, &reason);
            BoolExpr::False
        }
    };
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(expression.to_string(), compiled.clone());
    });
    compiled
}

/// Upstream isValid: parses with types for editor-side validation.
pub fn is_valid(expression: &str, types: Option<&dyn Fn(&str) -> Option<ParamKind>>) -> bool {
    if expression.trim().is_empty() {
        return true;
    }
    let mut p = Parser::new(expression, true, types);
    match p.or_expr() {
        Ok(_) => {
            p.skip_ws();
            p.at_end()
        }
        Err(_) => false,
    }
}

thread_local! {
    static CACHE: std::cell::RefCell<std::collections::HashMap<String, BoolExpr>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}
}
pub mod expr {
// Upstream herobot-common hero.bane.herobot.common.ai.expr.{ExprEval,
// ExprLog}.java (CC-BY-NC-4.0), ported 1:1: the numeric expression
// engine behind NUM_CALC and the numeric sub-grammar that
// BOOL_CALC/STRING_CALC/POS_CALC/DIR_CALC embed through parse_numeric.
// Upstream catches every parse failure and evaluates to a constant 0
// after warning once per distinct (expression reason) pair, so the
// Results below feed that path and never escape eval(). The 256-entry
// compile cache and the same-bounded log set are thread-locals here
// (upstream: ConcurrentHashMap + synchronized set; the portable
// runtime evaluates on one thread).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

/// Upstream ExprEval.ParamType, as far as the numeric grammar checks
/// it: only POSITION/ROTATION decide indexability in checkRef; the
/// other kinds matter to the boolean/string/vector grammars and get
/// their variants as those ports land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    Position,
    Rotation,
    // The other grammars type their operands with these.
    Bool,
    Num,
    Str,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fn1Kind {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sqrt,
    Floor,
    Ceil,
    Abs,
}

/// Upstream ExprEval.Expr: a compiled numeric expression tree.
#[derive(Clone, Debug)]
pub enum Expr {
    Zero,
    Const(f64),
    Var(String, i32),
    Neg(Box<Expr>),
    /// Upstream ++ / -- postfix: the value plus (true) or minus one.
    Post(Box<Expr>, bool),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Pi,
    E,
    Tau,
    Rand,
    Fn1(Fn1Kind, Box<Expr>),
    /// Upstream pow(a, b); with one argument it evaluates to a.
    Pow(Box<Expr>, Box<Expr>),
    Rand1(Box<Expr>),
    Rand2(Box<Expr>, Box<Expr>),
    /// randomint(a, b): floor(min)..=floor(max), inclusive.
    RandInt(Box<Expr>, Box<Expr>, bool),
}

impl Expr {
    pub fn eval(&self, vars: &mut dyn FnMut(&str, i32) -> f64) -> f64 {
        match self {
            Expr::Zero => 0.0,
            Expr::Const(v) => *v,
            Expr::Var(name, idx) => vars(name, *idx),
            Expr::Neg(a) => -a.eval(vars),
            Expr::Post(a, up) => a.eval(vars) + if *up { 1.0 } else { -1.0 },
            Expr::Bin(op, l, r) => match op {
                BinOp::Add => l.eval(vars) + r.eval(vars),
                BinOp::Sub => l.eval(vars) - r.eval(vars),
                BinOp::Mul => l.eval(vars) * r.eval(vars),
                // Upstream divides and mods guard the zero divisor with 0.
                BinOp::Div => {
                    let d = r.eval(vars);
                    if d == 0.0 { 0.0 } else { l.eval(vars) / d }
                }
                BinOp::Mod => {
                    let d = r.eval(vars);
                    if d == 0.0 { 0.0 } else { l.eval(vars) % d }
                }
                BinOp::Pow => l.eval(vars).powf(r.eval(vars)),
            },
            Expr::Pi => std::f64::consts::PI,
            Expr::E => std::f64::consts::E,
            Expr::Tau => std::f64::consts::PI * 2.0,
            Expr::Rand => rand_unit(),
            Expr::Fn1(kind, a) => {
                let x = a.eval(vars);
                match kind {
                    Fn1Kind::Sin => x.sin(),
                    Fn1Kind::Cos => x.cos(),
                    Fn1Kind::Tan => x.tan(),
                    Fn1Kind::Asin => x.asin(),
                    Fn1Kind::Acos => x.acos(),
                    Fn1Kind::Atan => x.atan(),
                    Fn1Kind::Sqrt => x.sqrt(),
                    Fn1Kind::Floor => x.floor(),
                    Fn1Kind::Ceil => x.ceil(),
                    Fn1Kind::Abs => x.abs(),
                }
            }
            Expr::Pow(a, b) => a.eval(vars).powf(b.eval(vars)),
            Expr::Rand1(a) => rand_unit() * a.eval(vars),
            // Upstream evaluates the lower bound twice here.
            Expr::Rand2(a, b) => {
                let lo = a.eval(vars);
                lo + rand_unit() * (b.eval(vars) - a.eval(vars))
            }
            Expr::RandInt(a, b, two) => {
                let x = a.eval(vars);
                let y = if *two { b.eval(vars) } else { 0.0 };
                let lo = x.min(y).floor() as i64;
                let hi = x.max(y).floor() as i64;
                (lo + (rand_unit() * (hi - lo + 1) as f64) as i64) as f64
            }
        }
    }
}

thread_local! {
    static RNG_STATE: Cell<u64> = Cell::new(0);
    static CACHE: RefCell<HashMap<String, Expr>> = RefCell::new(HashMap::new());
    static REPORTED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Thread-local xorshift* standing in for Math.random(), seeded from
/// the clock like the runtime's RANDOM_* port.
pub fn rand_unit() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    RNG_STATE.with(|st| {
        let mut x = st.get();
        if x == 0 {
            x = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E37_79B9_7F4A_7C15)
                | 1;
        }
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        st.set(x);
        let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (r >> 11) as f64 / (1u64 << 53) as f64
    })
}

/// Upstream ExprLog.badExpression.
pub fn log_bad_expression(expression: &str, reason: &str) {
    REPORTED.with(|rep| {
        let mut set = rep.borrow_mut();
        if set.len() >= 256 {
            set.clear();
        }
        if set.insert(format!("{} {}", expression, reason)) {
            eprintln!(
                "Invalid expression \"{}\" ({}) - it will evaluate to a constant",
                expression, reason
            );
        }
    });
}

/// Upstream ExprEval.isInputRef: "Input" followed by one or more digits.
pub fn is_input_ref(name: &str) -> bool {
    if name.len() < 6 {
        return false;
    }
    if !name[..5].eq_ignore_ascii_case("Input") {
        return false;
    }
    name[5..].chars().all(|c| c.is_ascii_digit())
}

/// Upstream ExprEval.canonicalInput.
pub fn canonical_input(name: &str) -> String {
    format!("Input{}", &name[5..])
}

fn cache_get(key: &str) -> Option<Expr> {
    CACHE.with(|c| c.borrow().get(key).cloned())
}

fn cache_put(key: &str, e: Expr) {
    CACHE.with(|c| {
        let mut m = c.borrow_mut();
        if m.len() >= 256 {
            m.clear();
        }
        m.insert(key.to_string(), e);
    });
}

/// Upstream ExprEval.compile: parse (strict), cache, fall back to the
/// zero constant on any parse failure.
pub fn compile(expression: &str) -> Expr {
    if expression.trim().is_empty() {
        return Expr::Zero;
    }
    if let Some(cached) = cache_get(expression) {
        return cached;
    }
    let compiled = match Parser::new(expression, true, None).parse_full() {
        Ok(e) => e,
        Err(reason) => {
            log_bad_expression(expression, &reason);
            Expr::Zero
        }
    };
    cache_put(expression, compiled.clone());
    compiled
}

/// Upstream ExprEval.eval(String, IndexedVars).
pub fn eval(expression: &str, vars: &mut dyn FnMut(&str, i32) -> f64) -> f64 {
    compile(expression).eval(vars)
}

/// Upstream ExprEval.isValid: full strict parse with no trailing input.
pub fn is_valid(expression: &str) -> bool {
    if expression.trim().is_empty() {
        return true;
    }
    let mut p = Parser::new(expression, true, None);
    match p.expr() {
        Ok(_) => {
            p.skip_ws();
            p.at_end()
        }
        Err(_) => false,
    }
}

/// Upstream ExprEval.Sub: a parsed sub-expression plus the end offset.
pub struct Sub {
    pub expr: Expr,
    pub end: usize,
}

/// Upstream ExprEval.parseNumeric, used by the other grammars to embed
/// numeric sub-expressions.
pub fn parse_numeric(
    s: &str,
    from: usize,
    strict: bool,
    types: Option<&dyn Fn(&str) -> Option<ParamKind>>,
) -> Result<Sub, String> {
    let mut p = Parser::new(s, strict, types);
    p.pos = from;
    let e = p.expr()?;
    Ok(Sub { expr: e, end: p.pos })
}

pub struct Parser<'a> {
    input: Vec<char>,
    strict: bool,
    types: Option<&'a dyn Fn(&str) -> Option<ParamKind>>,
    pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(
        input: &str,
        strict: bool,
        types: Option<&'a dyn Fn(&str) -> Option<ParamKind>>,
    ) -> Parser<'a> {
        Parser {
            input: input.chars().collect(),
            strict,
            types,
            pos: 0,
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    pub fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn lookahead(&self, tok: &str) -> bool {
        let mut i = self.pos;
        for c in tok.chars() {
            if self.input.get(i).copied() != Some(c) {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Upstream: expr() then skipWs() then atEnd() from compile().
    pub fn parse_full(&mut self) -> Result<Expr, String> {
        let e = self.expr()?;
        self.skip_ws();
        if self.at_end() {
            Ok(e)
        } else {
            Err("unexpected trailing input".to_string())
        }
    }

    pub fn expr(&mut self) -> Result<Expr, String> {
        let mut v = self.term()?;
        loop {
            self.skip_ws();
            let c = match self.peek() {
                Some(c) => c,
                None => break,
            };
            if c == '+' && !self.lookahead("++") {
                self.pos += 1;
                let l = v;
                let r = self.term()?;
                v = Expr::Bin(BinOp::Add, Box::new(l), Box::new(r));
            } else if c == '-' && !self.lookahead("--") {
                self.pos += 1;
                let l = v;
                let r = self.term()?;
                v = Expr::Bin(BinOp::Sub, Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn term(&mut self) -> Result<Expr, String> {
        let mut v = self.power()?;
        loop {
            self.skip_ws();
            let c = match self.peek() {
                Some(c) => c,
                None => break,
            };
            if c == '*' {
                self.pos += 1;
                let l = v;
                let r = self.power()?;
                v = Expr::Bin(BinOp::Mul, Box::new(l), Box::new(r));
            } else if c == '/' {
                self.pos += 1;
                let l = v;
                let r = self.power()?;
                v = Expr::Bin(BinOp::Div, Box::new(l), Box::new(r));
            } else if c == '%' {
                self.pos += 1;
                let l = v;
                let r = self.power()?;
                v = Expr::Bin(BinOp::Mod, Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn power(&mut self) -> Result<Expr, String> {
        let base = self.unary()?;
        self.skip_ws();
        if self.peek() == Some('^') {
            self.pos += 1;
            let e = self.power()?;
            return Ok(Expr::Bin(BinOp::Pow, Box::new(base), Box::new(e)));
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        if self.peek() == Some('-') && !self.lookahead("--") {
            self.pos += 1;
            let a = self.unary()?;
            return Ok(Expr::Neg(Box::new(a)));
        }
        if self.peek() == Some('+') && !self.lookahead("++") {
            self.pos += 1;
            return self.unary();
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut v = self.primary()?;
        loop {
            self.skip_ws();
            if self.lookahead("++") {
                self.pos += 2;
                v = Expr::Post(Box::new(v), true);
            } else if self.lookahead("--") {
                self.pos += 2;
                v = Expr::Post(Box::new(v), false);
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        let c = match self.peek() {
            Some(c) => c,
            None => return Err("unexpected input end".to_string()),
        };
        if c == '(' {
            self.pos += 1;
            let v = self.expr()?;
            self.skip_ws();
            if self.peek() == Some(')') {
                self.pos += 1;
            } else if self.strict {
                return Err("expected )".to_string());
            }
            return Ok(v);
        }
        if c == '{' {
            return self.variable();
        }
        if c == '.' || c.is_ascii_digit() {
            return self.number();
        }
        if c.is_alphabetic() || c == '_' {
            return self.identifier();
        }
        Err(format!("unexpected '{}'", c))
    }

    fn variable(&mut self) -> Result<Expr, String> {
        self.pos += 1;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == '}' {
                break;
            }
            self.pos += 1;
        }
        let name: String = self.input[start..self.pos].iter().collect::<String>().trim().to_string();
        if self.peek() == Some('}') {
            self.pos += 1;
        } else if self.strict {
            return Err("expected }".to_string());
        }
        let idx = self.parse_index()?;
        self.check_ref(&name, idx)?;
        Ok(Expr::Var(name, idx))
    }

    fn parse_index(&mut self) -> Result<i32, String> {
        self.skip_ws();
        if self.peek() != Some('[') {
            return Ok(-1);
        }
        self.pos += 1;
        self.skip_ws();
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err("expected index".to_string());
        }
        let text: String = self.input[start..self.pos].iter().collect();
        let idx: i32 = text.parse().map_err(|_| "bad index".to_string())?;
        self.skip_ws();
        if self.peek() == Some(']') {
            self.pos += 1;
        } else if self.strict {
            return Err("expected ]".to_string());
        }
        Ok(idx)
    }

    fn check_ref(&self, name: &str, idx: i32) -> Result<(), String> {
        if !self.strict {
            return Ok(());
        }
        let types = match self.types {
            Some(t) => t,
            None => return Ok(()),
        };
        let t = match types(name) {
            Some(t) => t,
            None => return Ok(()),
        };
        let vec = t == ParamKind::Position || t == ParamKind::Rotation;
        if idx < 0 {
            if vec {
                return Err(format!("{} needs an index like [0]", name));
            }
            return Ok(());
        }
        if !vec {
            return Err(format!("{} can't be indexed", name));
        }
        let max = if t == ParamKind::Position { 2 } else { 1 };
        if idx > max {
            return Err("index out of range".to_string());
        }
        Ok(())
    }

    fn number(&mut self) -> Result<Expr, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text: String = self.input[start..self.pos].iter().collect();
        let val: f64 = text.parse().map_err(|_| "invalid number".to_string())?;
        Ok(Expr::Const(val))
    }

    fn identifier(&mut self) -> Result<Expr, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let name: String = self.input[start..self.pos].iter().collect();
        self.skip_ws();
        if self.peek() == Some('(') {
            return self.call_function(&name);
        }
        self.constant(&name)
    }

    fn constant(&mut self, name: &str) -> Result<Expr, String> {
        if is_input_ref(name) {
            let canonical = canonical_input(name);
            let idx = self.parse_index()?;
            self.check_ref(&canonical, idx)?;
            return Ok(Expr::Var(canonical, idx));
        }
        match name.to_lowercase().as_str() {
            "pi" => Ok(Expr::Pi),
            "e" => Ok(Expr::E),
            "tau" => Ok(Expr::Tau),
            "rand" | "random" => Ok(Expr::Rand),
            _ => {
                if self.strict {
                    Err(format!("unknown name: {}", name))
                } else {
                    Ok(Expr::Zero)
                }
            }
        }
    }

    fn call_function(&mut self, name: &str) -> Result<Expr, String> {
        self.pos += 1;
        let arg = self.expr()?;
        let arg2 = Expr::Zero;
        self.skip_ws();
        let mut two_args = false;
        let mut arg2 = arg2;
        if self.peek() == Some(',') {
            self.pos += 1;
            arg2 = self.expr()?;
            two_args = true;
            self.skip_ws();
        }
        if self.peek() == Some(')') {
            self.pos += 1;
        } else if self.strict {
            return Err("expected )".to_string());
        }
        match name {
            "sin" => Ok(Expr::Fn1(Fn1Kind::Sin, Box::new(arg))),
            "cos" => Ok(Expr::Fn1(Fn1Kind::Cos, Box::new(arg))),
            "tan" => Ok(Expr::Fn1(Fn1Kind::Tan, Box::new(arg))),
            "asin" | "arcsin" => Ok(Expr::Fn1(Fn1Kind::Asin, Box::new(arg))),
            "acos" | "arccos" => Ok(Expr::Fn1(Fn1Kind::Acos, Box::new(arg))),
            "atan" | "arctan" => Ok(Expr::Fn1(Fn1Kind::Atan, Box::new(arg))),
            "sqrt" => Ok(Expr::Fn1(Fn1Kind::Sqrt, Box::new(arg))),
            "floor" => Ok(Expr::Fn1(Fn1Kind::Floor, Box::new(arg))),
            "ceil" => Ok(Expr::Fn1(Fn1Kind::Ceil, Box::new(arg))),
            "abs" => Ok(Expr::Fn1(Fn1Kind::Abs, Box::new(arg))),
            "pow" => {
                if two_args {
                    Ok(Expr::Pow(Box::new(arg), Box::new(arg2)))
                } else {
                    Ok(arg)
                }
            }
            "rand" | "random" => {
                if two_args {
                    Ok(Expr::Rand2(Box::new(arg), Box::new(arg2)))
                } else {
                    Ok(Expr::Rand1(Box::new(arg)))
                }
            }
            "randomint" | "randint" => Ok(Expr::RandInt(Box::new(arg), Box::new(arg2), two_args)),
            _ => {
                if self.strict {
                    Err(format!("unknown function: {}", name))
                } else {
                    Ok(Expr::Zero)
                }
            }
        }
    }
}
}
pub mod hex_codec {
// HeroBot AI hex codec ported 1:1 from HerobaneNair/HeRoBot
// (herobot-common/.../ai/AiHexCodec.java), CC-BY-NC-4.0.
// Upstream takes an AiScript and calls AiScriptCodec.toJson; the port
// splits that: callers pass the JSON string. Everything else matches:
// raw-deflate (Java nowrap, flate2's deflate is raw) at best
// compression, one version byte (0x01) framed in front, hex-encoded.
// JSON codec: script_codec.rs, ports next.

pub const VERSION: u8 = 0x01;

use flate2::write::DeflateEncoder;
use flate2::read::DeflateDecoder;
use flate2::Compression;
use std::io::{Read, Write};

pub fn encode_json(json: &str) -> String {
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    enc.write_all(json.as_bytes()).expect("in-memory write");
    let compressed = enc.finish().expect("in-memory finish");
    let mut framed = Vec::with_capacity(compressed.len() + 1);
    framed.push(VERSION);
    framed.extend_from_slice(&compressed);
    hex_encode(&framed)
}

pub fn decode_json(hex: &str, fallback_name: &str) -> Result<String, String> {
    let trimmed = hex.trim();
    if trimmed.is_empty() {
        return Err("hex is empty".into());
    }
    let framed = hex_decode(trimmed).map_err(|_| "not a valid hex string".to_string())?;
    if framed.is_empty() {
        return Err("hex is empty".into());
    }
    if framed[0] != VERSION {
        return Err(format!("unsupported AI hex version: 0x{:02x}", framed[0]));
    }
    let mut dec = DeflateDecoder::new(&framed[1..]);
    let mut json = Vec::new();
    dec.read_to_end(&mut json).map_err(|_| "corrupt AI hex payload".to_string())?;
    let _ = fallback_name;
    String::from_utf8(json).map_err(|_| "payload is not a valid AI script".to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
    let bytes = s.as_bytes();
    if bytes.len() % 2 != 0 { return Err(()); }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16).ok_or(())?;
        let lo = (bytes[i + 1] as char).to_digit(16).ok_or(())?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Ok(out)
}
}
pub mod script_codec {
// HeroBot AI script JSON codec ported 1:1 from HerobaneNair/HeRoBot
// (herobot-common/.../ai/AiScriptCodec.java), CC-BY-NC-4.0.
// Field layout matches upstream Gson output exactly: version, name,
// optional sorted, variables, varFolders, functions, blocks, wires,
// comments. Style runs are RLE pairs (len, flag). Legacy block names
// migrate like upstream blockFromJson.

use super::ai::{AiScript, BlockInstance, Comment, FuncDecl, ParamValue, VarDecl, VarType, Wire};
use super::block_registry::{find_block_type, BlockType};
use serde_json::{json, Map, Value};

pub fn to_json(script: &AiScript, omit_positions: bool) -> String {
    let mut root = Map::new();
    root.insert("version".into(), json!(script.version));
    root.insert("name".into(), json!(script.name));
    if omit_positions {
        root.insert("sorted".into(), json!(true));
    }

    let vars: Vec<Value> = script
        .variables
        .iter()
        .map(|v: &VarDecl| {
            let mut o = Map::new();
            o.insert("name".into(), json!(v.name));
            o.insert("type".into(), json!(var_type_name(v.ty)));
            o.insert("default".into(), match &v.default_value {
                Some(d) => default_to_json(v.ty, d),
                None => Value::Null,
            });
            if !v.folder.is_empty() {
                o.insert("folder".into(), json!(v.folder));
            }
            Value::Object(o)
        })
        .collect();
    root.insert("variables".into(), Value::Array(vars));

    root.insert("varFolders".into(), json!(script.var_folders));

    let funcs: Vec<Value> = script
        .functions
        .iter()
        .map(|f: &FuncDecl| {
            let mut o = Map::new();
            o.insert("name".into(), json!(f.name));
            if !f.folder.is_empty() {
                o.insert("folder".into(), json!(f.folder));
            }
            o.insert(
                "params".into(),
                Value::Array(f.params.iter().map(|t| json!(var_type_name(*t))).collect()),
            );
            Value::Object(o)
        })
        .collect();
    root.insert("functions".into(), Value::Array(funcs));

    let blocks: Vec<Value> = script
        .blocks
        .iter()
        .map(|b| block_to_json(b, omit_positions))
        .collect();
    root.insert("blocks".into(), Value::Array(blocks));

    let wires: Vec<Value> = script
        .wires
        .iter()
        .map(|w: &Wire| {
            let mut o = Map::new();
            o.insert("from".into(), json!(w.from_block_id));
            o.insert("out".into(), json!(w.out_port));
            o.insert("to".into(), json!(w.to_block_id));
            if w.to_port != 0 {
                o.insert("toPort".into(), json!(w.to_port));
            }
            Value::Object(o)
        })
        .collect();
    root.insert("wires".into(), Value::Array(wires));

    let comments: Vec<Value> = script
        .comments
        .iter()
        .map(|c: &Comment| {
            let mut o = Map::new();
            o.insert("id".into(), json!(c.id));
            o.insert("x".into(), json!(c.x));
            o.insert("y".into(), json!(c.y));
            o.insert("text".into(), json!(c.text));
            if let Some(styles) = encode_styles(c) {
                o.insert("styles".into(), styles);
            }
            if c.attached_to >= 0 {
                o.insert("attachedTo".into(), json!(c.attached_to));
                o.insert("offX".into(), json!(c.off_x));
                o.insert("offY".into(), json!(c.off_y));
            }
            Value::Object(o)
        })
        .collect();
    root.insert("comments".into(), Value::Array(comments));

    serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default()
}

fn var_type_name(t: VarType) -> &'static str {
    match t {
        VarType::Int => "INT",
        VarType::Double => "DOUBLE",
        VarType::Bool => "BOOL",
        VarType::Position => "POSITION",
        VarType::Rotation => "ROTATION",
        VarType::String => "STRING",
        VarType::Uuid => "UUID",
        VarType::Item => "ITEM",
    }
}

fn var_type_from_name(s: &str) -> Option<VarType> {
    Some(match s {
        "INT" => VarType::Int,
        "DOUBLE" => VarType::Double,
        "BOOL" => VarType::Bool,
        "POSITION" => VarType::Position,
        "ROTATION" => VarType::Rotation,
        "STRING" => VarType::String,
        "UUID" => VarType::Uuid,
        "ITEM" => VarType::Item,
        _ => return None,
    })
}

/// Upstream paramToJson: bool stays bool, numbers stay numbers,
/// everything else becomes its string.
fn default_to_json(t: VarType, d: &str) -> Value {
    match t {
        VarType::Bool => json!(d == "true"),
        VarType::Int => d.parse::<i64>().map(|v| json!(v)).unwrap_or_else(|_| json!(d)),
        VarType::Double => d.parse::<f64>().map(|v| json!(v)).unwrap_or_else(|_| json!(d)),
        _ => json!(d),
    }
}

fn encode_styles(c: &Comment) -> Option<Value> {
    if c.styles.iter().all(|s| *s == 0) {
        return None;
    }
    let mut arr = Vec::new();
    let mut i = 0usize;
    while i < c.styles.len() {
        let flag = c.styles[i];
        let mut j = i + 1;
        while j < c.styles.len() && c.styles[j] == flag {
            j += 1;
        }
        arr.push(json!((j - i) as i64));
        arr.push(json!(flag as i64));
        i = j;
    }
    Some(Value::Array(arr))
}

fn decode_styles(arr: &[Value], c: &mut Comment) {
    let mut styles = vec![0u8; c.text.len()];
    let mut pos = 0usize;
    let mut k = 0usize;
    while k + 1 < arr.len() {
        let len = arr[k].as_i64().unwrap_or(0) as usize;
        let flag = arr[k + 1].as_i64().unwrap_or(0) as u8;
        for _ in 0..len {
            if pos >= styles.len() {
                break;
            }
            styles[pos] = flag;
            pos += 1;
        }
        k += 2;
    }
    c.set_styles(styles);
}

fn block_to_json(b: &BlockInstance, omit_positions: bool) -> Value {
    let mut o = Map::new();
    o.insert("id".into(), json!(b.id));
    o.insert("type".into(), json!(BlockType::from_index(b.block_type).json_name()));
    if !omit_positions {
        o.insert("x".into(), json!(b.x));
        o.insert("y".into(), json!(b.y));
    }
    if b.paired_id >= 0 {
        o.insert("pairedId".into(), json!(b.paired_id));
    }
    let mut params = Map::new();
    for (k, v) in &b.params {
        params.insert(k.clone(), v.to_json());
    }
    o.insert("params".into(), Value::Object(params));
    if !b.reporter_params.is_empty() {
        let mut reps = Map::new();
        for (k, v) in &b.reporter_params {
            reps.insert(k.clone(), block_to_json(v, omit_positions));
        }
        o.insert("reporters".into(), Value::Object(reps));
    }
    Value::Object(o)
}

pub fn from_json(json_str: &str, fallback_name: &str) -> Result<AiScript, String> {
    let root: Value = serde_json::from_str(json_str).map_err(|e| format!("bad AI script json: {}", e))?;
    let name = root.get("name").and_then(Value::as_str).unwrap_or(fallback_name);
    let mut s = AiScript::new(name);
    s.version = root.get("version").and_then(Value::as_i64).unwrap_or(1) as i32;

    if let Some(vars) = root.get("variables").and_then(Value::as_array) {
        for vo in vars {
            let vname = vo.get("name").and_then(Value::as_str).unwrap_or("");
            let ty = var_type_from_name(vo.get("type").and_then(Value::as_str).unwrap_or(""))
                .unwrap_or(VarType::String);
            let def = vo.get("default").and_then(|d| json_to_var_default(d, ty));
            let folder = vo.get("folder").and_then(Value::as_str).unwrap_or("");
            s.variables.push(VarDecl {
                name: vname.to_string(),
                ty,
                default_value: def,
                folder: folder.to_string(),
            });
        }
    }

    if let Some(folders) = root.get("varFolders").and_then(Value::as_array) {
        for f in folders {
            if let Some(fs) = f.as_str() {
                s.var_folders.push(fs.to_string());
            }
        }
    }

    if let Some(funcs) = root.get("functions").and_then(Value::as_array) {
        for fo in funcs {
            let mut params = Vec::new();
            if let Some(ps) = fo.get("params").and_then(Value::as_array) {
                for pe in ps {
                    params.push(
                        var_type_from_name(pe.as_str().unwrap_or("")).unwrap_or(VarType::String),
                    );
                }
            }
            let ffolder = fo.get("folder").and_then(Value::as_str).unwrap_or("");
            s.functions.push(FuncDecl {
                name: fo.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                params,
                folder: ffolder.to_string(),
            });
        }
    }

    if let Some(blocks) = root.get("blocks").and_then(Value::as_array) {
        for be in blocks {
            let b = block_from_json(be).map_err(|e| e)?;
            s.put_block(b.clone());
            s.register_nested_ids(&b);
        }
    }

    if let Some(wires) = root.get("wires").and_then(Value::as_array) {
        for wo in wires {
            let (Some(from), Some(out), Some(to)) = (
                wo.get("from").and_then(Value::as_i64),
                wo.get("out").and_then(Value::as_i64),
                wo.get("to").and_then(Value::as_i64),
            ) else {
                continue; // upstream logs and skips a wire missing from/out/to
            };
            if s.block(from as i32).is_none() || s.block(to as i32).is_none() {
                continue; // upstream drops a wire to a missing block
            }
            let to_port = wo.get("toPort").and_then(Value::as_i64).unwrap_or(0);
            s.add_wire(Wire::with_to_port(from as i32, out as i32, to as i32, to_port as i32));
        }
    }

    let orphaned: Vec<i32> = s
        .blocks
        .iter()
        .filter(|b| b.paired_id >= 0 && s.block(b.paired_id).is_none())
        .map(|b| b.id)
        .collect();
    for bid in orphaned {
        if let Some(b) = s.blocks.iter_mut().find(|b| b.id == bid) {
            b.paired_id = -1;
        }
    }

    if let Some(comments) = root.get("comments").and_then(Value::as_array) {
        for co in comments {
            let id = co.get("id").and_then(Value::as_i64).map(|v| v as i32);
            let x = co.get("x").and_then(Value::as_f64).unwrap_or(0.0);
            let y = co.get("y").and_then(Value::as_f64).unwrap_or(0.0);
            let text = co.get("text").and_then(Value::as_str).unwrap_or("");
            let id = match id {
                Some(id) => id,
                None => s.fresh_id(),
            };
            let mut c = Comment::new(id, x, y, text);
            if let Some(styles) = co.get("styles").and_then(Value::as_array) {
                decode_styles(styles, &mut c);
            }
            if let Some(att) = co.get("attachedTo").and_then(Value::as_i64) {
                c.attached_to = att as i32;
                c.off_x = co.get("offX").and_then(Value::as_f64).unwrap_or(0.0);
                c.off_y = co.get("offY").and_then(Value::as_f64).unwrap_or(0.0);
            }
            s.comments.push(c);
            s.bump_next_id(id);
        }
    }
    Ok(s)
}

fn json_to_var_default(el: &Value, ty: VarType) -> Option<String> {
    if el.is_null() {
        return None;
    }
    Some(match ty {
        VarType::Bool => el.as_bool().map(|b| b.to_string()).unwrap_or_default(),
        VarType::Int => el.as_i64().map(|i| i.to_string()).unwrap_or_default(),
        VarType::Double => el.as_f64().map(|d| d.to_string()).unwrap_or_default(),
        _ => param_value_to_string(&ParamValue::from_json(el)),
    })
}

fn param_value_to_string(v: &ParamValue) -> String {
    match v {
        ParamValue::Bool(b) => b.to_string(),
        ParamValue::Int(i) => i.to_string(),
        ParamValue::Double(d) => d.to_string(),
        ParamValue::Str(s) => s.clone(),
    }
}

fn block_from_json(o: &Value) -> Result<BlockInstance, String> {
    let id = o.get("id").and_then(Value::as_i64).ok_or("block entry is missing id/type")? as i32;
    let type_name = o
        .get("type")
        .and_then(Value::as_str)
        .ok_or("block entry is missing id/type")?;
    let x = o.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = o.get("y").and_then(Value::as_f64).unwrap_or(0.0);

    // Legacy migrations, upstream blockFromJson verbatim.
    if let Some(op) = legacy_arith_op(type_name) {
        let mut calc = BlockInstance::new(id, BlockType::NumCalc.index(), x, y);
        calc.set_param("expression", ParamValue::Str(legacy_expr(o, op)));
        return Ok(calc);
    }
    if type_name == "TRADE_EXECUTE" {
        let mut click = BlockInstance::new(id, BlockType::InvClick.index(), x, y);
        click.set_param("menu", ParamValue::Str("container".into()));
        click.set_param("mode", ParamValue::Str(legacy_trade_click_mode(o).into()));
        click.set_param("slot", ParamValue::Int(2));
        return Ok(click);
    }

    let legacy_pitch = type_name == "PITCH";
    let mut type_name = if legacy_pitch { "YAW" } else { type_name };
    let legacy_offhand = type_name == "INV_SWAP_OFFHAND";
    if legacy_offhand {
        type_name = "INV_SWAP_HOTBAR";
    }
    if type_name == "LOOK_YAW_PITCH" {
        type_name = "LOOK_DIRECTION";
    } else if type_name == "LOOK_DIRECTION" && is_legacy_cardinal(o) {
        type_name = "LOOK_CARDINAL";
    }
    if type_name == "OPEN_CONTAINER" {
        type_name = "OPEN_INVENTORY";
    }
    if type_name == "CONTAINER_OPEN" {
        type_name = "INVENTORY_OPEN";
    }

    let legacy_drop_stack = type_name == "DROP_STACK";
    let block_type = find_block_type(if legacy_drop_stack { "DROP_ITEM" } else { type_name })
        .ok_or_else(|| format!("unknown block type {}", type_name))?;
    let mut b = BlockInstance::new(id, block_type.index(), x, y);
    if legacy_drop_stack {
        b.set_param("amount", ParamValue::Str("stack".into()));
    }
    if legacy_pitch {
        b.set_param("axis", ParamValue::Str("pitch".into()));
    }
    if let Some(paired) = o.get("pairedId").and_then(Value::as_i64) {
        b.paired_id = paired as i32;
    }
    if let Some(params) = o.get("params").and_then(Value::as_object) {
        for (k, v) in params {
            b.set_param(k.clone(), ParamValue::from_json(v));
        }
    }
    if block_type == BlockType::LookDirection
        && b.get_param("direction").is_none()
        && (b.get_param("yaw").is_some() || b.get_param("pitch").is_some())
    {
        let yaw = b.get_param("yaw").map(num_of).unwrap_or_else(|| "0".into());
        let pitch = b.get_param("pitch").map(num_of).unwrap_or_else(|| "0".into());
        b.set_param("direction", ParamValue::Str(format!("{} {}", yaw, pitch)));
    }
    if block_type == BlockType::InvSwapHotbar {
        if legacy_offhand {
            b.set_param("with", ParamValue::Str("offhand".into()));
        } else if b.get_param("with").is_none() && b.get_param("hotbarSlot").is_some() {
            let hs = b.get_param("hotbarSlot").unwrap().clone();
            b.set_param("with", ParamValue::Str(param_value_to_string(&hs)));
        }
    }
    if block_type == BlockType::RecipeBook
        && b.get_param("mode").is_none()
        && b.get_param("max").is_some()
    {
        let is_max = matches!(b.get_param("max"), Some(ParamValue::Bool(true)))
            || param_value_to_string(b.get_param("max").unwrap()) == "true";
        b.set_param("mode", ParamValue::Str(if is_max { "max".into() } else { "single".into() }));
    }
    if block_type == BlockType::Contains
        && b.get_param("checkCase").is_none()
        && b.get_param("ci").is_some()
    {
        let insensitive = matches!(b.get_param("ci"), Some(ParamValue::Bool(true)))
            || param_value_to_string(b.get_param("ci").unwrap()) == "true";
        b.set_param("checkCase", ParamValue::Bool(!insensitive));
        b.params.remove("ci");
    }
    if let Some(reps) = o.get("reporters").and_then(Value::as_object) {
        for (k, v) in reps {
            b.set_reporter(k.clone(), block_from_json(v)?);
        }
    }
    Ok(b)
}

/// Upstream legacyArithOp.
fn legacy_arith_op(type_name: &str) -> Option<&'static str> {
    Some(match type_name {
        "ADD" => "+",
        "SUB" => "-",
        "MUL" => "*",
        "DIV" => "/",
        "MOD" => "%",
        _ => return None,
    })
}

fn legacy_expr(o: &Value, op: &str) -> String {
    let get = |k: &str| {
        o.get("params")
            .and_then(|p| p.get(k))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    format!("{} {} {}", num_of(&ParamValue::Double(get("a"))), op, num_of(&ParamValue::Double(get("b"))))
}

/// Upstream num(): whole doubles print without a decimal point.
fn num_of(v: &ParamValue) -> String {
    match v {
        ParamValue::Double(d) if *d == d.floor() && d.is_finite() => format!("{}", *d as i64),
        other => param_value_to_string(other),
    }
}

fn is_legacy_cardinal(o: &Value) -> bool {
    matches!(
        o.get("params").and_then(|p| p.get("direction")).and_then(Value::as_str),
        Some("north" | "south" | "east" | "west" | "up" | "down")
    )
}

fn legacy_trade_click_mode(o: &Value) -> &'static str {
    let mode = o
        .get("params")
        .and_then(|p| p.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or("once");
    match mode {
        "shiftClick" => "shiftClick",
        "drop" => "throw",
        "dropMax" => "throwAll",
        _ => "click",
    }
}

/// Upstream wasSorted.
pub fn was_sorted(json_str: &str) -> bool {
    serde_json::from_str::<Value>(json_str)
        .ok()
        .and_then(|r| r.get("sorted").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// Upstream minCorner: [minX, minY] over blocks, None when empty.
pub fn min_corner(json_str: &str) -> Option<[f64; 2]> {
    let root: Value = serde_json::from_str(json_str).ok()?;
    let mut any = false;
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    for e in root.get("blocks")?.as_array()? {
        let x = e.get("x").and_then(Value::as_f64).unwrap_or(0.0);
        let y = e.get("y").and_then(Value::as_f64).unwrap_or(0.0);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        any = true;
    }
    if any {
        Some([min_x, min_y])
    } else {
        None
    }
}
}
pub mod sensor {
// HeroBot SensorExecutor ported from HerobaneNair/HeRoBot
// (herobot-mod/.../ai/runtime/executors/SensorExecutor.java, 265 lines),
// CC-BY-NC-4.0, plus the portable halves of the pieces it leans on:
// ItemSlots (herobot-common/.../ai/block/ItemSlots.java, 40 lines),
// SelectorExecutor.resolveFirstEntity's port-facing contract
// (herobot-mod/.../executors/SelectorExecutor.java, 64 lines) and
// DistanceCalculator's closest-point math
// (herobot-mod/.../util/DistanceCalculator.java, 86 lines).
//
// Sensor reporters are world-bound: they read entity state, block
// states, scoreboards and inventory from the live server. The port
// keeps every piece upstream computes from plain data (slot parsing,
// equipment-slot mapping, hitbox closest-point math, distance modes,
// subject defaulting, count/durability arithmetic) 1:1 and pushes the
// server reads behind the SensorHook seam the game layer supplies,
// the same shape as the World seam in vec_eval.rs. With no hook (the
// harness, or the game layer before the fake-player work lands) every
// reporter falls back to the value upstream produces with no live
// world: 0, false, an empty stack id string, or positive infinity for
// DISTANCE_TO with no target.

use super::ai::BlockInstance;
use super::block_registry::BlockType;
use super::runtime::{ParamEval, RuntimeValue};

/// Upstream ItemSlots.NAMES: 9 hotbar, 27 inventory, 2 weapons, 4 armor.
pub const ITEM_SLOT_NAMES: [&str; 42] = [
    "hotbar.0", "hotbar.1", "hotbar.2", "hotbar.3", "hotbar.4", "hotbar.5",
    "hotbar.6", "hotbar.7", "hotbar.8", "inventory.0", "inventory.1",
    "inventory.2", "inventory.3", "inventory.4", "inventory.5",
    "inventory.6", "inventory.7", "inventory.8", "inventory.9",
    "inventory.10", "inventory.11", "inventory.12", "inventory.13",
    "inventory.14", "inventory.15", "inventory.16", "inventory.17",
    "inventory.18", "inventory.19", "inventory.20", "inventory.21",
    "inventory.22", "inventory.23", "inventory.24", "inventory.25",
    "inventory.26", "weapon.mainhand", "weapon.offhand", "armor.head",
    "armor.chest", "armor.legs", "armor.feet",
];

fn valid_slot(index: i32) -> i32 {
    if index >= 0 && (index as usize) < ITEM_SLOT_NAMES.len() {
        index
    } else {
        -1
    }
}

/// Upstream ItemSlots.index: a numeric value is taken as the slot
/// number (Number.intValue truncation), a string is looked up by name
/// first then parsed as a number, an empty string is -1, anything
/// unparseable is -1, and anything out of range is -1.
pub fn item_slot_index(value: &RuntimeValue) -> i32 {
    match value {
        RuntimeValue::Int(i) => valid_slot(*i as i32),
        RuntimeValue::Double(d) => valid_slot(*d as i64 as i32),
        other => {
            let s = value_string(other);
            if s.trim().is_empty() {
                return -1;
            }
            if let Some(idx) = ITEM_SLOT_NAMES.iter().position(|n| *n == s.trim()) {
                return idx as i32;
            }
            match s.trim().parse::<i32>() {
                Ok(n) => valid_slot(n),
                Err(_) => -1,
            }
        }
    }
}

/// String form of a value for upstream's String.valueOf calls.
fn value_string(v: &RuntimeValue) -> String {
    match v {
        RuntimeValue::Str(s) => s.clone(),
        RuntimeValue::Bool(b) => b.to_string(),
        RuntimeValue::Int(i) => i.to_string(),
        RuntimeValue::Double(d) => d.to_string(),
        RuntimeValue::Vec3(x, y, z) => format!("{} {} {}", x, y, z),
        RuntimeValue::Rot(y, p) => format!("{} {}", y, p),
    }
}

/// Upstream's EQUIP map keys mapped to equipment slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipSlot {
    Mainhand,
    Offhand,
    Head,
    Chest,
    Legs,
    Feet,
}

/// Upstream SensorExecutor.EQUIP: only those six exact keys resolve;
/// anything else is an empty stack upstream and None here.
pub fn equip_slot(name: &str) -> Option<EquipSlot> {
    match name {
        "weapon.mainhand" => Some(EquipSlot::Mainhand),
        "weapon.offhand" => Some(EquipSlot::Offhand),
        "armor.head" => Some(EquipSlot::Head),
        "armor.chest" => Some(EquipSlot::Chest),
        "armor.legs" => Some(EquipSlot::Legs),
        "armor.feet" => Some(EquipSlot::Feet),
        _ => None,
    }
}

/// Upstream net.minecraft.world.phys.AABB, the fields the port's math
/// needs (Mth.clamp bounds and getCenter).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

impl Aabb {
    /// Upstream AABB.inflate: grow (or shrink with a negative amount)
    /// by d on every side.
    pub fn inflate(&self, d: f64) -> Aabb {
        Aabb {
            min_x: self.min_x - d,
            min_y: self.min_y - d,
            min_z: self.min_z - d,
            max_x: self.max_x + d,
            max_y: self.max_y + d,
            max_z: self.max_z + d,
        }
    }

    /// Upstream AABB.getCenter.
    pub fn center(&self) -> [f64; 3] {
        [
            (self.min_x + self.max_x) / 2.0,
            (self.min_y + self.max_y) / 2.0,
            (self.min_z + self.max_z) / 2.0,
        ]
    }
}

/// Upstream DistanceCalculator.closestPointOnHitbox: clamp the point
/// into the box on every axis.
pub fn closest_point_on_hitbox(aabb: &Aabb, p: [f64; 3]) -> [f64; 3] {
    [
        p[0].clamp(aabb.min_x, aabb.max_x),
        p[1].clamp(aabb.min_y, aabb.max_y),
        p[2].clamp(aabb.min_z, aabb.max_z),
    ]
}

/// Upstream DistanceCalculator.closestPointsBetween: walk the closest
/// point back and forth between the two boxes until it settles.
pub fn closest_points_between(from: &Aabb, to: &Aabb) -> ([f64; 3], [f64; 3]) {
    let center_from = from.center();
    let on_to = closest_point_on_hitbox(to, center_from);
    let on_from = closest_point_on_hitbox(from, on_to);
    let on_to_final = closest_point_on_hitbox(to, on_from);
    (on_from, on_to_final)
}

/// DISTANCE_TO's mode switch, upstream SensorExecutor lines ~167-174:
/// horizontal is the ground-plane distance, vertical the height
/// difference, anything else (including the empty default) the full
/// 3D distance.
pub fn distance_mode_value(a: [f64; 3], c: [f64; 3], mode: &str) -> f64 {
    match mode.trim().to_lowercase().as_str() {
        "horizontal" => ((a[0] - c[0]) * (a[0] - c[0]) + (a[2] - c[2]) * (a[2] - c[2])).sqrt(),
        "vertical" => (a[1] - c[1]).abs(),
        _ => {
            let dx = a[0] - c[0];
            let dy = a[1] - c[1];
            let dz = a[2] - c[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        }
    }
}

/// One inventory or equipment stack as the port's value model sees it.
/// Upstream keeps live ItemStack objects; the game layer fills this in
/// from the real stacks and the port does the arithmetic on it.
#[derive(Clone, Debug, Default)]
pub struct StackData {
    pub item_id: String,
    pub count: i64,
    pub damage: i64,
    pub max_damage: i64,
    pub max_stack: i64,
    pub damageable: bool,
}

/// ATTACK_COOLDOWN's two reads off a Player: the raw attack-strength
/// delay in ticks and the charge scale at 0 and 0.5 partial ticks
/// (upstream getCurrentItemAttackStrengthDelay / getAttackStrengthScale).
#[derive(Clone, Copy, Debug)]
pub struct AttackCooldown {
    pub delay: f64,
    pub scale_zero: f64,
    pub scale_half: f64,
}

/// The world seam behind every world-bound sensor reporter. The game
/// layer supplies it (player level, entity lookups, inventory,
/// scoreboard, command dispatch); the harness runs hookless and every
/// method defaults to None, which each reporter maps to the neutral
/// value upstream produces with no live world.
pub trait SensorHook {
    fn player_entity(&self) -> i64 {
        0
    }
    /// Upstream SelectorExecutor.resolveFirstEntity: an entity value
    /// resolves to itself, a UUID string to the entity with that UUID,
    /// anything else to the first entity a /select-style expression
    /// matches (selectors run with all permissions upstream). None
    /// covers the empty and failed cases.
    fn resolve_first_entity(&self, _sel: &RuntimeValue) -> Option<i64> {
        None
    }
    fn entity_position(&self, _entity: i64) -> Option<[f64; 3]> {
        None
    }
    fn entity_yaw_pitch(&self, _entity: i64) -> Option<(f32, f32)> {
        None
    }
    /// Health of a living entity; None is the non-living case
    /// (upstream HEALTH reports 0.0 for those).
    fn entity_health(&self, _entity: i64) -> Option<f64> {
        None
    }
    /// hurtTime of a living entity; None is the non-living case.
    fn entity_hurt_time(&self, _entity: i64) -> Option<i64> {
        None
    }
    /// Upstream ON_DAMAGE's other-entity half: hurtTime > 0 and
    /// hurtTime == hurtDuration, i.e. the hurt flash just started.
    fn entity_on_damage(&self, _entity: i64) -> Option<bool> {
        None
    }
    fn attack_cooldown(&self, _entity: i64) -> Option<AttackCooldown> {
        None
    }
    fn ping(&self, _entity: i64) -> Option<i64> {
        None
    }
    /// Upstream DOING_ACTION: the action pack reads for a bot player
    /// (forward/strafing/active actions), the entity flags otherwise.
    /// The action string is already validated by the port.
    fn doing_action(&self, _entity: i64, _action: &str) -> Option<bool> {
        None
    }
    fn day_time(&self) -> Option<i64> {
        None
    }
    fn game_time(&self) -> Option<i64> {
        None
    }
    /// Upstream IN_PATH: the bot's path follower still active, or the
    /// remote-path state for a real player.
    fn in_path(&self) -> Option<bool> {
        None
    }
    fn block_at(&self, _pos: [f64; 3]) -> Option<String> {
        None
    }
    fn entity_aabb(&self, _entity: i64) -> Option<Aabb> {
        None
    }
    /// IS_TOUCHING_BLOCK's scan: the port inflates the box itself and
    /// hands over the inflated area plus the raw block id string; the
    /// hook returns false for an unparseable id like upstream.
    fn any_block_in(&self, _area: &Aabb, _target: &str) -> Option<bool> {
        None
    }
    fn entity_has_tag(&self, _entity: i64, _tag: &str) -> Option<bool> {
        None
    }
    fn scoreboard_score(&self, _objective: &str, _entity: i64) -> Option<f64> {
        None
    }
    /// The acting player's stack in slot index 0..41 (upstream
    /// slotStack), None for an empty slot.
    fn slot_stack(&self, _index: i32) -> Option<StackData> {
        None
    }
    /// A living entity's equipment stack, None when empty.
    fn equipment(&self, _entity: i64, _slot: EquipSlot) -> Option<StackData> {
        None
    }
    /// Upstream DataExecutor.asItemStack for MAX_COUNT's item param.
    fn item_stack(&self, _item: &RuntimeValue) -> Option<StackData> {
        None
    }
    /// COMMAND_RESULT: run the command with all permissions and
    /// suppressed output, return its callback value.
    fn command_result(&self, _command: &str) -> Option<f64> {
        None
    }
}

pub struct NoSensorHook;
impl SensorHook for NoSensorHook {}

static NO_SENSOR: NoSensorHook = NoSensorHook;

/// The sensor reporters as a set, so the runtime dispatch can fall
/// through to sensor_report without naming them one by one.
pub fn is_sensor_type(ty: BlockType) -> bool {
    matches!(
        ty,
        BlockType::Scoreboard
            | BlockType::HasTag
            | BlockType::Health
            | BlockType::Position
            | BlockType::Yaw
            | BlockType::BlockAt
            | BlockType::IsTouchingBlock
            | BlockType::DistanceTo
            | BlockType::TimeOfDay
            | BlockType::EveryXTicks
            | BlockType::OnDamage
            | BlockType::AttackCooldown
            | BlockType::HurtTime
            | BlockType::Ping
            | BlockType::ItemInSlot
            | BlockType::Equipment
            | BlockType::GetCount
            | BlockType::MaxCount
            | BlockType::CommandResult
            | BlockType::InPath
            | BlockType::PathReached
            | BlockType::PathFailed
            | BlockType::DoingAction
    )
}

/// Upstream SensorExecutor.subject: with neither a literal param nor a
/// nested reporter in the slot the subject is the acting player;
/// otherwise the slot value resolves through the selector and a
/// failed resolve falls back to the player too.
fn subject(b: &BlockInstance, r: &super::runtime::ScriptRunner, br: &super::runtime::Branch, hook: &dyn SensorHook, slot: &str) -> i64 {
    if !b.params.contains_key(slot) && !b.reporter_params.contains_key(slot) {
        return hook.player_entity();
    }
    let raw = ParamEval::raw(b, slot, r, br);
    hook.resolve_first_entity(&raw).unwrap_or_else(|| hook.player_entity())
}

/// Upstream SensorExecutor.distanceShape: "hitbox" (any case, after
/// trim) takes the entity's bounding box, anything else a zero-size
/// box at the entity's position.
fn distance_shape(hook: &dyn SensorHook, entity: i64, shape: &str) -> Option<Aabb> {
    if shape.trim().eq_ignore_ascii_case("hitbox") {
        hook.entity_aabb(entity)
    } else {
        hook.entity_position(entity).map(|p| Aabb {
            min_x: p[0],
            min_y: p[1],
            min_z: p[2],
            max_x: p[0],
            max_y: p[1],
            max_z: p[2],
        })
    }
}

/// The sensor reporter dispatch, mirroring upstream's register() table
/// entry by entry. Game-bound reads go through the hook; everything
/// computed from plain data stays in the port.
pub fn sensor_report(
    ty: BlockType,
    b: &BlockInstance,
    br: &super::runtime::Branch,
    r: &super::runtime::ScriptRunner,
) -> Option<RuntimeValue> {
    let hook: &dyn SensorHook = match r.sensor.as_deref() {
        Some(h) => h,
        None => &NO_SENSOR,
    };
    match ty {
        BlockType::Health => {
            let e = subject(b, r, br, hook, "target");
            Some(RuntimeValue::Double(hook.entity_health(e).unwrap_or(0.0)))
        }
        BlockType::Position => {
            let e = subject(b, r, br, hook, "target");
            let p = hook.entity_position(e).unwrap_or([0.0, 0.0, 0.0]);
            Some(RuntimeValue::Vec3(p[0], p[1], p[2]))
        }
        BlockType::Yaw => {
            let e = subject(b, r, br, hook, "target");
            let axis = ParamEval::eval_string(b, "axis", r, br);
            let (yaw, pitch) = hook.entity_yaw_pitch(e).unwrap_or((0.0, 0.0));
            Some(RuntimeValue::Double(if axis == "pitch" {
                pitch as f64
            } else {
                yaw as f64
            }))
        }
        BlockType::TimeOfDay => Some(RuntimeValue::Double(
            (hook.day_time().unwrap_or(0) % 24000) as f64,
        )),
        BlockType::EveryXTicks => {
            let n = ParamEval::eval_int(b, "ticks", r, br).max(1);
            Some(RuntimeValue::Bool(match hook.game_time() {
                Some(t) => t % n == 0,
                None => true,
            }))
        }
        BlockType::DoingAction => {
            let action = ParamEval::eval_string(b, "action", r, br);
            let e = subject(b, r, br, hook, "target");
            Some(RuntimeValue::Bool(match action.as_str() {
                "walking" | "strafing" | "sprinting" | "sneaking" | "using" | "attacking"
                | "swinging" | "jumping" | "dropping" | "swapping" => {
                    hook.doing_action(e, &action).unwrap_or(false)
                }
                _ => false,
            }))
        }
        BlockType::OnDamage => {
            let e = subject(b, r, br, hook, "target");
            if e == hook.player_entity() {
                Some(RuntimeValue::Bool(r.was_damaged()))
            } else {
                Some(RuntimeValue::Bool(hook.entity_on_damage(e).unwrap_or(false)))
            }
        }
        BlockType::AttackCooldown => {
            let e = subject(b, r, br, hook, "target");
            let ticks = ParamEval::eval_string(b, "kind", r, br) == "ticksLeft";
            match hook.attack_cooldown(e) {
                Some(cd) => {
                    if ticks {
                        let v = (cd.delay * (1.0 - cd.scale_zero)).ceil() as i64;
                        Some(RuntimeValue::Double(v.max(0) as f64))
                    } else {
                        Some(RuntimeValue::Double(cd.scale_half))
                    }
                }
                None => Some(RuntimeValue::Double(if ticks { 0.0 } else { 1.0 })),
            }
        }
        BlockType::HurtTime => {
            let e = subject(b, r, br, hook, "target");
            Some(RuntimeValue::Double(
                hook.entity_hurt_time(e).unwrap_or(0) as f64,
            ))
        }
        BlockType::Ping => {
            let e = subject(b, r, br, hook, "target");
            Some(RuntimeValue::Double(hook.ping(e).unwrap_or(0) as f64))
        }
        BlockType::InPath => Some(RuntimeValue::Bool(hook.in_path().unwrap_or(false))),
        BlockType::PathReached => Some(RuntimeValue::Bool(r.path_reached())),
        BlockType::PathFailed => Some(RuntimeValue::Bool(r.path_failed())),
        BlockType::BlockAt => {
            let e = subject(b, r, br, hook, "target");
            let base = hook.entity_position(e).unwrap_or([0.0, 0.0, 0.0]);
            let raw = ParamEval::raw(b, "position", r, br);
            let pos = match raw {
                RuntimeValue::Vec3(x, y, z) => [x, y, z],
                other => {
                    let (x, y, z) = ParamEval::parse_vec3(&value_string(&other), (base[0], base[1], base[2]));
                    [x, y, z]
                }
            };
            Some(RuntimeValue::Str(hook.block_at(pos).unwrap_or_default()))
        }
        BlockType::IsTouchingBlock => {
            let block_id = ParamEval::eval_string(b, "block", r, br);
            let e = subject(b, r, br, hook, "target");
            match hook.entity_aabb(e) {
                Some(bb) => Some(RuntimeValue::Bool(
                    hook.any_block_in(&bb.inflate(0.001), &block_id).unwrap_or(false),
                )),
                None => Some(RuntimeValue::Bool(false)),
            }
        }
        BlockType::HasTag => {
            let sel = ParamEval::raw(b, "target", r, br);
            let tag = ParamEval::eval_string(b, "tag", r, br);
            Some(RuntimeValue::Bool(match hook.resolve_first_entity(&sel) {
                Some(e) => hook.entity_has_tag(e, &tag).unwrap_or(false),
                None => false,
            }))
        }
        BlockType::DistanceTo => {
            let sel = ParamEval::raw(b, "target", r, br);
            let e = match hook.resolve_first_entity(&sel) {
                Some(e) => e,
                None => return Some(RuntimeValue::Double(f64::INFINITY)),
            };
            let from = subject(b, r, br, hook, "from");
            let from_shape = ParamEval::eval_string(b, "fromShape", r, br);
            let to_shape = ParamEval::eval_string(b, "toShape", r, br);
            let (a, c) = match (
                distance_shape(hook, from, &from_shape),
                distance_shape(hook, e, &to_shape),
            ) {
                (Some(fa), Some(ta)) => closest_points_between(&fa, &ta),
                _ => return Some(RuntimeValue::Double(f64::INFINITY)),
            };
            let mode = ParamEval::eval_string(b, "mode", r, br);
            Some(RuntimeValue::Double(distance_mode_value(a, c, &mode)))
        }
        BlockType::Scoreboard => {
            let obj = ParamEval::eval_string(b, "objective", r, br);
            let sel = ParamEval::raw(b, "target", r, br);
            Some(RuntimeValue::Double(match hook.resolve_first_entity(&sel) {
                Some(e) => hook.scoreboard_score(&obj, e).unwrap_or(0.0),
                None => 0.0,
            }))
        }
        BlockType::ItemInSlot => {
            let slot = item_slot_index(&ParamEval::raw(b, "slot", r, br));
            let stack = if slot < 0 { None } else { hook.slot_stack(slot) };
            Some(RuntimeValue::Str(match stack {
                Some(sd) => sd.item_id,
                None => String::new(),
            }))
        }
        BlockType::Equipment => {
            let sel = ParamEval::raw(b, "target", r, br);
            let slot = ParamEval::eval_string(b, "slot", r, br);
            let stack = match (hook.resolve_first_entity(&sel), equip_slot(&slot)) {
                (Some(e), Some(es)) => hook.equipment(e, es),
                _ => None,
            };
            Some(RuntimeValue::Str(match stack {
                Some(sd) => sd.item_id,
                None => String::new(),
            }))
        }
        BlockType::GetCount => {
            let slot = item_slot_index(&ParamEval::raw(b, "slot", r, br));
            let kind = ParamEval::eval_string(b, "kind", r, br);
            let stack = if slot < 0 { None } else { hook.slot_stack(slot) };
            Some(RuntimeValue::Double(match stack {
                Some(sd) => {
                    if kind == "durability" {
                        if sd.damageable {
                            (sd.max_damage - sd.damage) as f64
                        } else {
                            0.0
                        }
                    } else {
                        sd.count as f64
                    }
                }
                None => 0.0,
            }))
        }
        BlockType::MaxCount => {
            let raw = ParamEval::raw(b, "item", r, br);
            let kind = ParamEval::eval_string(b, "kind", r, br);
            Some(RuntimeValue::Double(match hook.item_stack(&raw) {
                Some(sd) => {
                    if kind == "durability" {
                        if sd.damageable {
                            sd.max_damage as f64
                        } else {
                            0.0
                        }
                    } else {
                        sd.max_stack as f64
                    }
                }
                None => 0.0,
            }))
        }
        BlockType::CommandResult => {
            let mut cmd = ParamEval::eval_string(b, "command", r, br).trim().to_string();
            if cmd.starts_with('/') {
                cmd.remove(0);
            }
            if cmd.is_empty() {
                return Some(RuntimeValue::Double(0.0));
            }
            Some(RuntimeValue::Double(hook.command_result(&cmd).unwrap_or(0.0)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_slots_index_matches_upstream() {
        // hotbar.0..8 are 0..8, inventory.0..26 are 9..35, weapons 36/37,
        // armor 38..41; unknown names, empty strings and out-of-range
        // numbers are -1.
        assert_eq!(item_slot_index(&RuntimeValue::Str("hotbar.0".into())), 0);
        assert_eq!(item_slot_index(&RuntimeValue::Str("inventory.0".into())), 9);
        assert_eq!(item_slot_index(&RuntimeValue::Str("inventory.26".into())), 35);
        assert_eq!(item_slot_index(&RuntimeValue::Str("weapon.mainhand".into())), 36);
        assert_eq!(item_slot_index(&RuntimeValue::Str("weapon.offhand".into())), 37);
        assert_eq!(item_slot_index(&RuntimeValue::Str("armor.head".into())), 38);
        assert_eq!(item_slot_index(&RuntimeValue::Str("armor.feet".into())), 41);
        assert_eq!(item_slot_index(&RuntimeValue::Int(5)), 5);
        assert_eq!(item_slot_index(&RuntimeValue::Int(41)), 41);
        assert_eq!(item_slot_index(&RuntimeValue::Int(42)), -1);
        assert_eq!(item_slot_index(&RuntimeValue::Int(-1)), -1);
        assert_eq!(item_slot_index(&RuntimeValue::Double(7.9)), 7);
        assert_eq!(item_slot_index(&RuntimeValue::Str("  12  ".into())), 12);
        assert_eq!(item_slot_index(&RuntimeValue::Str("".into())), -1);
        assert_eq!(item_slot_index(&RuntimeValue::Str("nope".into())), -1);
    }

    #[test]
    fn equip_slots_map_exactly() {
        assert_eq!(equip_slot("weapon.mainhand"), Some(EquipSlot::Mainhand));
        assert_eq!(equip_slot("armor.head"), Some(EquipSlot::Head));
        assert_eq!(equip_slot("mainhand"), None);
        assert_eq!(equip_slot(""), None);
    }

    #[test]
    fn closest_points_between_matches_upstream() {
        let from = Aabb { min_x: 0.0, min_y: 0.0, min_z: 0.0, max_x: 2.0, max_y: 2.0, max_z: 2.0 };
        let to = Aabb { min_x: 10.0, min_y: 0.0, min_z: 10.0, max_x: 12.0, max_y: 2.0, max_z: 12.0 };
        let (a, c) = closest_points_between(&from, &to);
        assert_eq!(a, [2.0, 1.0, 2.0]);
        assert_eq!(c, [10.0, 1.0, 10.0]);
        // Overlapping boxes clamp the walk-through point inside both.
        let (a, c) = closest_points_between(&from, &from);
        assert_eq!(a, c);
    }

    #[test]
    fn distance_modes_match_upstream() {
        let a = [0.0, 5.0, 0.0];
        let c = [3.0, 1.0, 4.0];
        assert!((distance_mode_value(a, c, "horizontal") - 5.0).abs() < 1e-9);
        assert_eq!(distance_mode_value(a, c, "vertical"), 4.0);
        assert!((distance_mode_value(a, c, "") - (41.0f64).sqrt()).abs() < 1e-9);
        assert!((distance_mode_value(a, c, "NORMAL") - (41.0f64).sqrt()).abs() < 1e-9);
    }
}
}
pub mod settings {
// HeroBot settings/rules ported 1:1 from HerobaneNair/HeRoBot
// (herobot-common/.../rule/HeroBotSettings.java, CC-BY-NC-4.0).
// Java @Rule annotations become a static registry table: same names,
// defaults, bounds and descriptions, ordered as upstream lists them.
// The /herobot command reads this table via native_register_commands
// (server hooks 8): /herobot <setting> shows value+description,
// /herobot <setting> <value> sets for the session,
// /herobot <setting> <value> <perm> sets and saves.

pub enum RuleType { Bool, Int, Float, Enum }

pub struct Rule {
    pub name: &'static str,
    pub desc: &'static str,
    pub ty: RuleType,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

// Values live in a separate mutable table so the command layer can read
// and write them generically (Java used static fields + reflection).
#[derive(Clone, Copy, PartialEq)]
pub enum RuleValue {
    Bool(bool),
    Int(i64),
    Float(f64),
}

#[derive(Clone, Copy, PartialEq)]
pub enum ExplosionNoDmgMode { True, False, Most }

impl ExplosionNoDmgMode {
    pub fn enabled(self) -> bool { self != ExplosionNoDmgMode::False }
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "TRUE" => Some(Self::True),
            "FALSE" => Some(Self::False),
            "MOST" => Some(Self::Most),
            _ => None,
        }
    }
}

pub struct Settings {
    pub creative_no_clip: bool,
    pub creative_fly_speed: f64,
    pub creative_fly_drag: f64,
    pub allow_spawning_offline_players: bool,
    pub allow_listing_bot_players: bool,
    pub bot_ping_to_ticks: i64,
    pub bot_leave_on_death: bool,
    pub shield_stunning: bool,
    pub shield_stunning_window: i64,
    pub shield_delay_ticks: i64,
    pub kb_scaling: bool,
    pub damage_ticks: i64,
    pub explosion_no_block_damage: ExplosionNoDmgMode,
    pub explosion_no_fire: bool,
    pub wind_charge_no_trigger: bool,
    pub editable_player_nbt: bool,
    pub clients_ignore_slow_tick_rate: bool,
    pub xp_no_cooldown: bool,
    pub no_projectile_random: bool,
    pub delete_chunk_entities: bool,
    pub disable_experimental_screen: bool,
    pub server_has_herobot: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            creative_no_clip: false,
            creative_fly_speed: 1.0,
            creative_fly_drag: 0.09,
            allow_spawning_offline_players: true,
            allow_listing_bot_players: true,
            bot_ping_to_ticks: 25,
            bot_leave_on_death: false,
            shield_stunning: false,
            shield_stunning_window: 3,
            shield_delay_ticks: 5,
            kb_scaling: true,
            damage_ticks: 10,
            explosion_no_block_damage: ExplosionNoDmgMode::False,
            explosion_no_fire: false,
            wind_charge_no_trigger: false,
            editable_player_nbt: false,
            clients_ignore_slow_tick_rate: false,
            xp_no_cooldown: false,
            no_projectile_random: false,
            delete_chunk_entities: false,
            disable_experimental_screen: false,
            server_has_herobot: true,
        }
    }
}

impl Settings {
    // Upstream: damageInvulnerableTicks() = damageTicks * 2
    pub fn damage_invulnerable_ticks(&self) -> i64 { self.damage_ticks * 2 }

    pub fn rules() -> &'static [Rule] {
        &RULES
    }
}

pub static RULES: &[Rule] = &[
    Rule { name: "creativeNoClip", desc: "Creative No Clip, allows to client player to phase through blocks", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "creativeFlySpeed", desc: "Changes creative flying speed multiplier (Default 1.0), how quickly the client flies", ty: RuleType::Float, min: Some(0.0), max: None },
    Rule { name: "creativeFlyDrag", desc: "Changes creative air drag (Default 0.09), how quickly the air stops the client while flying", ty: RuleType::Float, min: Some(0.0), max: Some(1.0) },
    Rule { name: "allowSpawningOfflinePlayers", desc: "Spawn offline players in online mode if online-mode player with specified name does not exist", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "allowListingBotPlayers", desc: "Allows listing bot players on the multiplayer screen", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "botPingToTicks", desc: "Change the ping to tick conversion for Bot Players (default 25)", ty: RuleType::Int, min: Some(1.0), max: None },
    Rule { name: "botLeaveOnDeath", desc: "Bots disconnect on death rather than respawning", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "shieldStunning", desc: "Enables shield stunning, where the shielding player can be damaged immediately after the shield is disabled", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "shieldStunningWindow", desc: "Makes shield stuns use decreased [paper] kb during the window, default 5", ty: RuleType::Int, min: Some(0.0), max: None },
    Rule { name: "shieldDelayTicks", desc: "Change the delay of bringing up the shield", ty: RuleType::Int, min: Some(0.0), max: None },
    Rule { name: "kbScaling", desc: "Disable knockback scaling, as in entities will take the same vertical knockback regardless of their previous vertical velocity", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "damageTicks", desc: "Change how many ticks the hurt animation/red flash and damage cooldown last when any living entity is damaged (default 10)", ty: RuleType::Int, min: Some(1.0), max: None },
    Rule { name: "explosionNoBlockDamage", desc: "Explosions won't destroy blocks", ty: RuleType::Enum, min: None, max: None },
    Rule { name: "explosionNoFire", desc: "Allows intentional game design explosions (from beds and respawn anchors) to not explode with fire", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "windChargeNoTrigger", desc: "Wind Charges won't activate redstone blocks", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "editablePlayerNbt", desc: "Enables editing player nbt, so you can directly edit values within a player's data", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "clientsIgnoreSlowTickRate", desc: "If true, makes client players ignore slower tick rates", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "xpNoCooldown", desc: "Players absorb XP instantly, without delay", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "noProjectileRandom", desc: "Removes randomness from projectiles while true", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "deleteChunkEntities", desc: "Chunk Resetting deletes entities within that chunk", ty: RuleType::Bool, min: None, max: None },
    Rule { name: "disableExperimentalScreen", desc: "Remove the experimental world setting (if on client disables all, if on world, only disables it on this world)", ty: RuleType::Bool, min: None, max: None },
];
}
pub mod str_eval {
// Port of herobot-common StrEval.java (CC-BY-NC-4.0, HerobaneNair/HeRoBot).
// String expression engine: "text" + {var} + Input refs, python-style
// slicing [start:stop:step] and single-char indexing [i], and .strip().
// Compile failures and trailing input yield the empty expression; missing
// variables stringify to "". Upstream indexes UTF-16 code units; this port
// indexes Unicode scalar values (identical for the ASCII bot scripts use).

use super::expr::{canonical_input, is_input_ref};
use std::cell::RefCell;
use std::collections::HashMap;

pub const OPS_LEGEND: &str = "\"text\"  +  {var}  Input1  ' \\,\" '";
pub const OPS_LEGEND_2: &str = "[start:stop:step]  [::-1]  .strip()";

const CACHE_LIMIT: usize = 256;

#[derive(Clone, Debug)]
pub enum StrExpr {
    Empty,
    Lit(String),
    Concat(Box<StrExpr>, Box<StrExpr>),
    Var(String),
    Input(String),
    Index(Box<StrExpr>, i32),
    Slice(Box<StrExpr>, Option<i32>, Option<i32>, Option<i32>),
    Strip(Box<StrExpr>),
}

pub type StrVarsFn<'a> = dyn FnMut(&str) -> Option<String> + 'a;

impl StrExpr {
    pub fn eval(&self, vars: &mut StrVarsFn) -> String {
        match self {
            StrExpr::Empty => String::new(),
            StrExpr::Lit(s) => s.clone(),
            StrExpr::Concat(l, r) => format!("{}{}", l.eval(vars), r.eval(vars)),
            StrExpr::Var(n) | StrExpr::Input(n) => vars(n).unwrap_or_default(),
            StrExpr::Index(b, i) => str_index(&b.eval(vars), *i),
            StrExpr::Slice(b, s, e, st) => str_slice(&b.eval(vars), *s, *e, *st),
            StrExpr::Strip(b) => b.eval(vars).trim().to_string(),
        }
    }
}

thread_local! {
    static CACHE: RefCell<HashMap<String, StrExpr>> = RefCell::new(HashMap::new());
}

pub fn eval(expression: &str, vars: &mut StrVarsFn) -> String {
    compile(expression).eval(vars)
}

pub fn is_valid(expression: &str) -> bool {
    if expression.trim().is_empty() {
        return true;
    }
    let mut p = Parser::new(expression, true);
    match p.expr() {
        Ok(_) => {
            p.skip_ws();
            p.at_end()
        }
        Err(_) => false,
    }
}

pub fn compile(expression: &str) -> StrExpr {
    if expression.trim().is_empty() {
        return StrExpr::Empty;
    }
    let mut compiled = StrExpr::Empty;
    let mut p = Parser::new(expression, false);
    if let Ok(e) = p.expr() {
        p.skip_ws();
        if p.at_end() {
            compiled = e;
        }
    }
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= CACHE_LIMIT {
            c.clear();
        }
        c.insert(expression.to_string(), compiled.clone());
    });
    compiled
}

struct Parser {
    input: Vec<char>,
    pos: usize,
    strict: bool,
}

impl Parser {
    fn new(src: &str, strict: bool) -> Parser {
        Parser {
            input: src.chars().collect(),
            pos: 0,
            strict,
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek(&self) -> char {
        if self.at_end() { '\0' } else { self.input[self.pos] }
    }

    fn skip_ws(&mut self) {
        while !self.at_end() && self.input[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn expr(&mut self) -> Result<StrExpr, String> {
        let mut v = self.term()?;
        loop {
            self.skip_ws();
            if self.peek() == '+' {
                self.pos += 1;
                let r = self.term()?;
                v = StrExpr::Concat(Box::new(v), Box::new(r));
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn term(&mut self) -> Result<StrExpr, String> {
        let mut v = self.atom()?;
        loop {
            self.skip_ws();
            let c = self.peek();
            if c == '[' {
                v = self.slice_of(v)?;
            } else if c == '.' {
                v = self.method_of(v)?;
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn method_of(&mut self, base: StrExpr) -> Result<StrExpr, String> {
        self.pos += 1; // '.'
        let start = self.pos;
        while !self.at_end() && self.input[self.pos].is_alphabetic() {
            self.pos += 1;
        }
        let name: String = self.input[start..self.pos].iter().collect();
        self.skip_ws();
        if self.peek() == '(' {
            self.pos += 1;
        } else if self.strict {
            return Err(format!("expected ( after .{}", name));
        }
        self.skip_ws();
        if self.peek() == ')' {
            self.pos += 1;
        } else if self.strict {
            return Err(format!("expected ) after .{}(", name));
        }
        if name == "strip" {
            return Ok(StrExpr::Strip(Box::new(base)));
        }
        if self.strict {
            return Err(format!("unknown method: {}", name));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<StrExpr, String> {
        self.skip_ws();
        let c = self.peek();
        if c == '"' {
            self.quoted()
        } else if c == '{' {
            self.var_ref()
        } else if c.is_alphabetic() {
            self.input_ref()
        } else {
            Err("expected \"string\", {var} or Input#".to_string())
        }
    }

    fn quoted(&mut self) -> Result<StrExpr, String> {
        self.pos += 1;
        let mut sb = String::new();
        while !self.at_end() && self.input[self.pos] != '"' {
            let c = self.input[self.pos];
            let next = if self.pos + 1 < self.input.len() {
                self.input[self.pos + 1]
            } else {
                '\0'
            };
            if c == '\\' && (next == '"' || next == '\\') {
                self.pos += 1;
                sb.push(next);
            } else {
                sb.push(c);
            }
            self.pos += 1;
        }
        if self.peek() == '"' {
            self.pos += 1;
        } else {
            // Upstream StrEval throws on an unclosed string; compile() then
            // yields the EMPTY expression, in both strict and lenient modes.
            return Err("unclosed string".to_string());
        }
        Ok(StrExpr::Lit(sb))
    }

    fn var_ref(&mut self) -> Result<StrExpr, String> {
        self.pos += 1;
        let start = self.pos;
        while !self.at_end() && self.input[self.pos] != '}' {
            self.pos += 1;
        }
        let name: String = self.input[start..self.pos].iter().collect::<String>().trim().to_string();
        if self.peek() == '}' {
            self.pos += 1;
        } else if self.strict {
            return Err("expected }".to_string());
        }
        if self.strict && name.is_empty() {
            return Err("empty variable name".to_string());
        }
        Ok(StrExpr::Var(name))
    }

    fn input_ref(&mut self) -> Result<StrExpr, String> {
        let start = self.pos;
        while !self.at_end()
            && (self.input[self.pos].is_alphanumeric() || self.input[self.pos] == '_')
        {
            self.pos += 1;
        }
        let name: String = self.input[start..self.pos].iter().collect();
        if !is_input_ref(&name) {
            return Err(format!("unknown name: {}", name));
        }
        Ok(StrExpr::Input(canonical_input(&name)))
    }

    fn slice_of(&mut self, base: StrExpr) -> Result<StrExpr, String> {
        self.pos += 1; // '['
        let start = self.opt_int();
        self.skip_ws();
        if self.peek() == ']' {
            self.pos += 1;
            if start.is_none() {
                if self.strict {
                    return Err("empty index".to_string());
                }
                return Ok(base);
            }
            return Ok(StrExpr::Index(Box::new(base), start.unwrap()));
        }
        if self.peek() != ':' {
            if self.strict {
                return Err("expected : or ] in slice".to_string());
            }
            return Ok(base);
        }
        self.pos += 1;
        let stop = self.opt_int();
        let mut c: Option<i32> = None;
        self.skip_ws();
        if self.peek() == ':' {
            self.pos += 1;
            c = self.opt_int();
            self.skip_ws();
        }
        if self.peek() == ']' {
            self.pos += 1;
        } else if self.strict {
            return Err("expected ] in slice".to_string());
        }
        Ok(StrExpr::Slice(Box::new(base), start, stop, c))
    }

    // Upstream uses Integer.parseInt here; an overflow digit string would
    // throw and compile to EMPTY. This port yields None instead (treated
    // as a missing bound), a divergence only for absurd inputs.
    fn opt_int(&mut self) -> Option<i32> {
        self.skip_ws();
        let start = self.pos;
        if self.peek() == '-' {
            self.pos += 1;
        }
        while !self.at_end() && self.input[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos == start || (self.pos == start + 1 && self.input[start] == '-') {
            self.pos = start;
            return None;
        }
        let text: String = self.input[start..self.pos].iter().collect();
        text.parse::<i32>().ok()
    }
}

fn str_slice(s: &str, start: Option<i32>, stop: Option<i32>, step: Option<i32>) -> String {
    let st = step.unwrap_or(1);
    if st == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i32;
    let mut out = String::new();
    if st > 0 {
        let b = norm(start, 0, len, false);
        let e = norm(stop, len, len, false);
        let mut i = b;
        while i < e {
            out.push(chars[i as usize]);
            i += st;
        }
    } else {
        let b = norm(start, len - 1, len, true);
        let e = norm(stop, -1, len, true);
        let mut i = b;
        while i > e {
            out.push(chars[i as usize]);
            i += st;
        }
    }
    out
}

fn norm(v: Option<i32>, def: i32, len: i32, neg_step: bool) -> i32 {
    let mut i = match v {
        None => return def,
        Some(x) => x,
    };
    if i < 0 {
        i += len;
    }
    if neg_step {
        if i < 0 {
            return -1;
        }
        if i >= len {
            return len - 1;
        }
    } else {
        if i < 0 {
            return 0;
        }
        if i > len {
            return len;
        }
    }
    i
}

fn str_index(s: &str, i: i32) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut i = i;
    if i < 0 {
        i += chars.len() as i32;
    }
    if i < 0 || i as usize >= chars.len() {
        return String::new();
    }
    chars[i as usize].to_string()
}
}
pub mod vec_eval {
// Port of herobot-common VecEval.java (CC-BY-NC-4.0, HerobaneNair/HeRoBot).
// Vector expression engine behind POS_CALC and DIR_CALC: pos(x,y,z) and
// dir(y,p) relative constructors with ~ offsets and ^ local coordinates,
// element-wise + - * / % with scalar broadcast, per-component math calls,
// climb/fall terrain scans, and {var}/Input<n> refs. Compile errors fall
// back to the ZERO expression; runtime arity mismatches fall back to a zero
// vector of the required length (upstream MISMATCH catch semantics).

use super::expr::{canonical_input, is_input_ref, rand_unit};
use std::cell::RefCell;
use std::collections::HashMap;

pub const OPS_LEGEND: &str = "pos(x,y,z)  dir(y,p)  +  -  *  /  %";
pub const OPS_LEGEND_2: &str =
    "climb(pos)  fall(pos)  floor(vector)  {var}  Input1  ~  ^";

const CACHE_LIMIT: usize = 256;

/// World seam for terrain scans and ~ / ^ bases; the game layer supplies it
/// (upstream: the player's Level, plus the player position and rotation for
/// the base values).
pub trait World {
    fn is_air(&self, x: i32, y: i32, z: i32) -> bool;
    fn min_y(&self) -> i32;
    fn max_y(&self) -> i32;
    /// Upstream default: no base information (all zeros).
    fn base(&self, axis: i32, rotation: bool) -> f64 {
        let _ = (axis, rotation);
        0.0
    }
}

/// What a {var} or Input<n> ref yields before coercion (upstream Object).
#[derive(Clone, Debug)]
pub enum VecVal {
    /// double[]: a Vec3 variable as [x, y, z].
    Arr(Vec<f64>),
    /// float[]: a Rot variable as [yaw, pitch].
    Rot([f64; 2]),
    Num(f64),
    Bool(bool),
    /// Anything else coerces to [0].
    Other,
}

/// Upstream coerce: double[] keeps its shape, float[] widens, Number and
/// Boolean become 1-length arrays, everything else is [0].
pub fn coerce(v: VecVal) -> Vec<f64> {
    match v {
        VecVal::Arr(a) => a,
        VecVal::Rot(r) => r.to_vec(),
        VecVal::Num(n) => vec![n],
        VecVal::Bool(b) => vec![if b { 1.0 } else { 0.0 }],
        VecVal::Other => vec![0.0],
    }
}

/// Ref type callback for is_valid (upstream Function<String, ParamType>).
pub enum RefType {
    Position,
    Rotation,
    Number,
    /// Any other parameter type is rejected in strict mode.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinOp { Add, Sub, Mul, Div, Mod, Pow }

#[derive(Clone, Copy, Debug)]
pub enum MapOp { Floor, Ceil, Abs, Sqrt, Sin, Cos, Tan, Asin, Acos, Atan, Neg }

fn map_apply(op: MapOp, x: f64) -> f64 {
    match op {
        MapOp::Floor => x.floor(),
        MapOp::Ceil => x.ceil(),
        MapOp::Abs => x.abs(),
        MapOp::Sqrt => x.sqrt(),
        MapOp::Sin => x.sin(),
        MapOp::Cos => x.cos(),
        MapOp::Tan => x.tan(),
        MapOp::Asin => x.asin(),
        MapOp::Acos => x.acos(),
        MapOp::Atan => x.atan(),
        MapOp::Neg => -x,
    }
}

/// Element-wise apply with 1-vs-n scalar broadcast; two different multi
/// component shapes throw (upstream MISMATCH).
fn apply_op(a: &[f64], b: &[f64], op: BinOp) -> Result<Vec<f64>, ()> {
    let f = |x: f64, y: f64| match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x * y,
        BinOp::Div => {
            if y == 0.0 { 0.0 } else { x / y }
        }
        BinOp::Mod => {
            if y == 0.0 { 0.0 } else { x % y }
        }
        BinOp::Pow => x.powf(y),
    };
    if a.len() == b.len() {
        return Ok(a.iter().zip(b.iter()).map(|(x, y)| f(*x, *y)).collect());
    }
    if a.len() == 1 {
        return Ok(b.iter().map(|y| f(a[0], *y)).collect());
    }
    if b.len() == 1 {
        return Ok(a.iter().map(|x| f(*x, b[0])).collect());
    }
    Err(())
}

/// One pos(...) / dir(...) argument: an expression, a ~ offset, or a ^ local
/// offset. Upstream keeps three parallel lists (parts, offsets, carrots);
/// exactly one of the three is set per slot.
#[derive(Clone, Debug)]
pub enum RelArg {
    Expr(VecNode),
    Tilde(f64),
    Carrot(f64),
}

#[derive(Clone, Debug)]
pub enum VecNode {
    /// Upstream ZERO: [0], arity 1.
    Zero,
    Const(Vec<f64>, usize),
    /// rand / random evaluate per call (upstream Math.random()).
    Rand,
    Ref(String, usize),
    Bin(Box<VecNode>, Box<VecNode>, BinOp, usize),
    Map(Box<VecNode>, MapOp, usize),
    Rel(usize, Vec<RelArg>),
    /// climb / fall terrain scan; the bool is up.
    Terrain(bool, Box<VecNode>),
}

impl VecNode {
    fn num_args(&self) -> usize {
        match self {
            VecNode::Zero => 1,
            VecNode::Const(_, n) => *n,
            VecNode::Rand => 1,
            VecNode::Ref(_, n) => *n,
            VecNode::Bin(_, _, _, n) => *n,
            VecNode::Map(_, _, n) => *n,
            VecNode::Rel(n, _) => *n,
            VecNode::Terrain(..) => 3,
        }
    }

    /// Upstream Expr::eval; Err is the MISMATCH path.
    pub fn eval(
        &self,
        vars: &mut dyn FnMut(&str) -> Option<VecVal>,
        world: Option<&dyn World>,
    ) -> Result<Vec<f64>, ()> {
        match self {
            VecNode::Zero => Ok(vec![0.0]),
            VecNode::Const(v, _) => Ok(v.clone()),
            VecNode::Rand => Ok(vec![rand_unit()]),
            VecNode::Ref(name, _) => Ok(vars(name).map(coerce).unwrap_or_else(|| vec![0.0])),
            VecNode::Bin(l, r, op, _) => {
                let a = l.eval(vars, world)?;
                let b = r.eval(vars, world)?;
                apply_op(&a, &b, *op)
            }
            VecNode::Map(n, op, _) => {
                let v = n.eval(vars, world)?;
                Ok(v.iter().map(|x| map_apply(*op, *x)).collect())
            }
            VecNode::Rel(n, args) => {
                let rotation = *n == 2;
                let carrot_count = args.iter().filter(|a| matches!(a, RelArg::Carrot(_))).count();
                let use_carrot = carrot_count == *n && *n == 3;
                if use_carrot {
                    let get = |i: usize| -> f64 {
                        match args.get(i) {
                            Some(RelArg::Carrot(x)) => *x,
                            _ => 0.0,
                        }
                    };
                    let (left, up, forwards) = (get(0), get(1), get(2));
                    return match world {
                        None => Ok(vec![left, up, forwards]),
                        Some(w) => {
                            let yaw = w.base(0, true);
                            let pitch = w.base(1, true);
                            let local = local_offset(yaw, pitch, left, up, forwards);
                            Ok(vec![
                                w.base(0, false) + local[0],
                                w.base(1, false) + local[1],
                                w.base(2, false) + local[2],
                            ])
                        }
                    };
                }
                let mut out = vec![0.0; *n];
                for i in 0..args.len().min(*n) {
                    match &args[i] {
                        RelArg::Tilde(off) => {
                            let basis = match world {
                                None => 0.0,
                                Some(w) => w.base(i as i32, rotation),
                            };
                            out[i] = basis + off;
                        }
                        RelArg::Expr(e) => {
                            let v = e.eval(vars, world)?;
                            out[i] = v[0];
                        }
                        // A ^ slot that never reached the useCarrot path is
                        // ignored by upstream (its lists are null): 0.
                        RelArg::Carrot(_) => {}
                    }
                }
                Ok(out)
            }
            VecNode::Terrain(up, arg) => {
                let p = arg.eval(vars, world)?;
                if p.len() != 3 {
                    return Err(());
                }
                let w = match world {
                    None => return Ok(p),
                    Some(w) => w,
                };
                let bx = p[0].floor() as i32;
                let bz = p[2].floor() as i32;
                let start_y = p[1].floor() as i32;
                let mut y = start_y;
                if *up {
                    if w.is_air(bx, y, bz) {
                        return Ok(p);
                    }
                    while y < w.max_y() && !w.is_air(bx, y, bz) {
                        y += 1;
                    }
                } else {
                    if !w.is_air(bx, y, bz) {
                        return Ok(p);
                    }
                    while y - 1 >= w.min_y() && w.is_air(bx, y - 1, bz) {
                        y -= 1;
                    }
                }
                if y == start_y {
                    Ok(p)
                } else {
                    Ok(vec![p[0], y as f64, p[2]])
                }
            }
        }
    }
}

/// Upstream localOffset: world-space offset from left / up / forwards in the
/// player's view basis.
pub fn local_offset(yaw_deg: f64, pitch_deg: f64, left: f64, up: f64, forwards: f64) -> [f64; 3] {
    let f = (yaw_deg + 90.0).to_radians().cos();
    let g = (yaw_deg + 90.0).to_radians().sin();
    let h = (-pitch_deg).to_radians().cos();
    let i = (-pitch_deg).to_radians().sin();
    let j = (-pitch_deg + 90.0).to_radians().cos();
    let k = (-pitch_deg + 90.0).to_radians().sin();
    let fwd_basis = [f * h, i, g * h];
    let up_basis = [f * j, k, g * j];
    let l = cross(&fwd_basis, &up_basis);
    let left_basis = [-l[0], -l[1], -l[2]];
    [
        fwd_basis[0] * forwards + up_basis[0] * up + left_basis[0] * left,
        fwd_basis[1] * forwards + up_basis[1] * up + left_basis[1] * left,
        fwd_basis[2] * forwards + up_basis[2] * up + left_basis[2] * left,
    ]
}

fn cross(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn combine_num_args(a: usize, b: usize) -> Result<usize, String> {
    if a == b {
        return Ok(a);
    }
    if a == 0 {
        return Ok(if b == 1 { 0 } else { b });
    }
    if b == 0 {
        return Ok(if a == 1 { 0 } else { a });
    }
    if a == 1 {
        return Ok(b);
    }
    if b == 1 {
        return Ok(a);
    }
    Err("cannot mix a position with a direction".to_string())
}

fn bin(l: VecNode, r: VecNode, op: BinOp) -> Result<VecNode, String> {
    let num_args = combine_num_args(l.num_args(), r.num_args())?;
    Ok(VecNode::Bin(Box::new(l), Box::new(r), op, num_args))
}

struct Parser<'a> {
    input: Vec<char>,
    pos: usize,
    strict: bool,
    required_num_args: usize,
    types: Option<&'a dyn Fn(&str) -> Option<RefType>>,
}

impl<'a> Parser<'a> {
    fn new(
        src: &str,
        strict: bool,
        required_num_args: usize,
        types: Option<&'a dyn Fn(&str) -> Option<RefType>>,
    ) -> Parser<'a> {
        Parser { input: src.chars().collect(), pos: 0, strict, required_num_args, types }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek(&self) -> char {
        if self.at_end() { '\0' } else { self.input[self.pos] }
    }

    fn skip_ws(&mut self) {
        while !self.at_end() && self.input[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn expr(&mut self) -> Result<VecNode, String> {
        let mut v = self.term()?;
        loop {
            self.skip_ws();
            let c = self.peek();
            if c == '+' {
                self.pos += 1;
                let r = self.term()?;
                v = bin(v, r, BinOp::Add)?;
            } else if c == '-' {
                self.pos += 1;
                let r = self.term()?;
                v = bin(v, r, BinOp::Sub)?;
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn term(&mut self) -> Result<VecNode, String> {
        let mut v = self.unary()?;
        loop {
            self.skip_ws();
            let c = self.peek();
            if c == '*' {
                self.pos += 1;
                let r = self.unary()?;
                v = bin(v, r, BinOp::Mul)?;
            } else if c == '/' {
                self.pos += 1;
                let r = self.unary()?;
                v = bin(v, r, BinOp::Div)?;
            } else if c == '%' {
                self.pos += 1;
                let r = self.unary()?;
                v = bin(v, r, BinOp::Mod)?;
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn unary(&mut self) -> Result<VecNode, String> {
        self.skip_ws();
        if self.peek() == '-' {
            self.pos += 1;
            let n = self.unary()?;
            let na = n.num_args();
            return Ok(VecNode::Map(Box::new(n), MapOp::Neg, na));
        }
        if self.peek() == '+' {
            self.pos += 1;
            return self.unary();
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<VecNode, String> {
        self.skip_ws();
        let c = self.peek();
        if c == '(' {
            self.pos += 1;
            let v = self.expr()?;
            self.skip_ws();
            if self.peek() == ')' {
                self.pos += 1;
            } else if self.strict {
                return Err("expected )".to_string());
            }
            return Ok(v);
        }
        if c == '{' {
            return self.variable();
        }
        if c == '.' || c.is_ascii_digit() {
            return self.number();
        }
        if c.is_alphabetic() || c == '_' {
            return self.identifier();
        }
        Err(format!("unexpected '{}'", c))
    }

    fn variable(&mut self) -> Result<VecNode, String> {
        self.pos += 1;
        let start = self.pos;
        while !self.at_end() && self.input[self.pos] != '}' {
            self.pos += 1;
        }
        let raw: String = self.input[start..self.pos].iter().collect();
        let name = raw.trim().to_string();
        if self.peek() == '}' {
            self.pos += 1;
        } else if self.strict {
            return Err("expected }".to_string());
        }
        self.ref_node(&name)
    }

    fn ref_node(&mut self, name: &str) -> Result<VecNode, String> {
        let num_args = match self.types {
            None => 0,
            Some(t) => match t(name) {
                None => 0,
                Some(RefType::Position) => 3,
                Some(RefType::Rotation) => 2,
                Some(RefType::Number) => 1,
                Some(RefType::Other) => {
                    return Err(format!("{} is not a number, position, or direction", name));
                }
            },
        };
        Ok(VecNode::Ref(name.to_string(), num_args))
    }

    fn number(&mut self) -> Result<VecNode, String> {
        let start = self.pos;
        while !self.at_end()
            && (self.input[self.pos].is_ascii_digit() || self.input[self.pos] == '.')
        {
            self.pos += 1;
        }
        let text: String = self.input[start..self.pos].iter().collect();
        let val = text.parse::<f64>().map_err(|_| "bad number".to_string())?;
        Ok(VecNode::Const(vec![val], 1))
    }

    fn identifier(&mut self) -> Result<VecNode, String> {
        let start = self.pos;
        while !self.at_end()
            && (self.input[self.pos].is_alphanumeric() || self.input[self.pos] == '_')
        {
            self.pos += 1;
        }
        let name: String = self.input[start..self.pos].iter().collect();
        self.skip_ws();
        if self.peek() == '(' {
            return self.call_function(&name);
        }
        self.constant(&name)
    }

    fn constant(&mut self, name: &str) -> Result<VecNode, String> {
        if is_input_ref(name) {
            return self.ref_node(&canonical_input(name));
        }
        match name.to_lowercase().as_str() {
            "pi" => Ok(VecNode::Const(vec![std::f64::consts::PI], 1)),
            "e" => Ok(VecNode::Const(vec![std::f64::consts::E], 1)),
            "tau" => Ok(VecNode::Const(vec![std::f64::consts::TAU], 1)),
            "rand" | "random" => Ok(VecNode::Rand),
            _ => {
                if self.strict {
                    return Err(format!("unknown name: {}", name));
                }
                Ok(VecNode::Zero)
            }
        }
    }

    fn call_function(&mut self, name: &str) -> Result<VecNode, String> {
        let lower = name.to_lowercase();
        if lower == "pos" {
            return self.rel_construct(3);
        }
        if lower == "dir" {
            return self.rel_construct(2);
        }
        let mut a = self.args()?;
        match lower.as_str() {
            "floor" => self.per_component(a, MapOp::Floor),
            "ceil" => self.per_component(a, MapOp::Ceil),
            "abs" => self.per_component(a, MapOp::Abs),
            "sqrt" => self.per_component(a, MapOp::Sqrt),
            "sin" => self.per_component(a, MapOp::Sin),
            "cos" => self.per_component(a, MapOp::Cos),
            "tan" => self.per_component(a, MapOp::Tan),
            "asin" | "arcsin" => self.per_component(a, MapOp::Asin),
            "acos" | "arccos" => self.per_component(a, MapOp::Acos),
            "atan" | "arctan" => self.per_component(a, MapOp::Atan),
            "pow" => {
                if self.strict && a.len() != 2 {
                    return Err("pow needs 2 arguments".to_string());
                }
                if a.is_empty() {
                    return Ok(VecNode::Zero);
                }
                if a.len() < 2 {
                    return Ok(a.into_iter().next().unwrap());
                }
                if self.strict && (a[0].num_args() > 1 || a[1].num_args() > 1) {
                    return Err("pow arguments must be numbers".to_string());
                }
                let l = a.remove(0);
                let r = a.remove(0);
                bin(l, r, BinOp::Pow)
            }
            "climb" => self.terrain_node(a, true),
            "fall" => self.terrain_node(a, false),
            _ => {
                if self.strict {
                    return Err(format!("unknown function: {}", name));
                }
                Ok(VecNode::Zero)
            }
        }
    }

    fn args(&mut self) -> Result<Vec<VecNode>, String> {
        self.pos += 1; // '('
        let mut list = Vec::new();
        self.skip_ws();
        if self.peek() == ')' {
            self.pos += 1;
            return Ok(list);
        }
        list.push(self.expr()?);
        self.skip_ws();
        while self.peek() == ',' {
            self.pos += 1;
            list.push(self.expr()?);
            self.skip_ws();
        }
        if self.peek() == ')' {
            self.pos += 1;
        } else if self.strict {
            return Err("expected )".to_string());
        }
        Ok(list)
    }

    fn per_component(&mut self, a: Vec<VecNode>, op: MapOp) -> Result<VecNode, String> {
        if self.strict && a.len() != 1 {
            return Err("needs 1 argument".to_string());
        }
        if a.is_empty() {
            return Ok(VecNode::Zero);
        }
        let n = a.into_iter().next().unwrap();
        let na = n.num_args();
        Ok(VecNode::Map(Box::new(n), op, na))
    }

    fn terrain_node(&mut self, a: Vec<VecNode>, up: bool) -> Result<VecNode, String> {
        if self.strict {
            if self.required_num_args != 3 {
                return Err(format!(
                    "{} only works in pos calc",
                    if up { "climb" } else { "fall" }
                ));
            }
            if a.len() != 1 {
                return Err("needs 1 argument".to_string());
            }
            let na = a[0].num_args();
            if na != 0 && na != 3 {
                return Err("argument must be a position".to_string());
            }
        }
        if a.is_empty() {
            return Ok(VecNode::Zero);
        }
        Ok(VecNode::Terrain(up, Box::new(a.into_iter().next().unwrap())))
    }

    fn rel_construct(&mut self, n: usize) -> Result<VecNode, String> {
        self.pos += 1; // '('
        let mut parts: Vec<Option<VecNode>> = Vec::new();
        let mut offsets: Vec<Option<f64>> = Vec::new();
        let mut carrots: Vec<Option<f64>> = Vec::new();
        self.skip_ws();
        if self.peek() != ')' {
            self.parse_rel_arg(&mut parts, &mut offsets, &mut carrots)?;
            self.skip_ws();
            while self.peek() == ',' {
                self.pos += 1;
                self.parse_rel_arg(&mut parts, &mut offsets, &mut carrots)?;
                self.skip_ws();
            }
        }
        if self.peek() == ')' {
            self.pos += 1;
        } else if self.strict {
            return Err("expected )".to_string());
        }
        if self.strict && parts.len() != n {
            return Err(format!(
                "{} needs {} numbers",
                if n == 3 { "pos" } else { "dir" },
                n
            ));
        }
        let carrot_count = carrots.iter().filter(|c| c.is_some()).count();
        if carrot_count > 0 {
            if self.strict && n != 3 {
                return Err("^ local coordinates only work in pos calc".to_string());
            }
            if self.strict && carrot_count != n {
                return Err("cannot mix ^ with ~ or absolute coordinates".to_string());
            }
        }
        let mut args = Vec::with_capacity(parts.len());
        for i in 0..parts.len() {
            if let Some(o) = offsets[i] {
                args.push(RelArg::Tilde(o));
            } else if let Some(p) = parts[i].take() {
                args.push(RelArg::Expr(p));
            } else {
                args.push(RelArg::Carrot(carrots[i].unwrap_or(0.0)));
            }
        }
        Ok(VecNode::Rel(n, args))
    }

    #[allow(clippy::type_complexity)]
    fn parse_rel_arg(
        &mut self,
        parts: &mut Vec<Option<VecNode>>,
        offsets: &mut Vec<Option<f64>>,
        carrots: &mut Vec<Option<f64>>,
    ) -> Result<(), String> {
        self.skip_ws();
        let c = self.peek();
        if c == '^' || c == '~' {
            self.pos += 1;
            let mut offset = 0.0;
            let c2 = self.peek();
            if c2 == '-' || c2 == '+' || c2 == '.' || c2.is_ascii_digit() {
                let start = self.pos;
                if c2 == '-' || c2 == '+' {
                    self.pos += 1;
                }
                while !self.at_end()
                    && (self.input[self.pos].is_ascii_digit() || self.input[self.pos] == '.')
                {
                    self.pos += 1;
                }
                let text: String = self.input[start..self.pos].iter().collect();
                match text.parse::<f64>() {
                    Ok(v) => offset = v,
                    Err(_) => {
                        if self.strict {
                            return Err(if c == '^' {
                                "bad ^ offset".to_string()
                            } else {
                                "bad ~ offset".to_string()
                            });
                        }
                    }
                }
            }
            if c == '^' {
                parts.push(None);
                offsets.push(None);
                carrots.push(Some(offset));
            } else {
                parts.push(None);
                offsets.push(Some(offset));
                carrots.push(None);
            }
        } else {
            let arg = self.expr()?;
            if self.strict && arg.num_args() > 1 {
                return Err("arguments must be numbers".to_string());
            }
            parts.push(Some(arg));
            offsets.push(None);
            carrots.push(None);
        }
        Ok(())
    }
}

thread_local! {
    static CACHE: RefCell<HashMap<String, VecNode>> = RefCell::new(HashMap::new());
}

/// Upstream eval: compile (bad expressions become ZERO), run, and fall back
/// to a zero vector of the required length on any mismatch or error.
pub fn eval(
    expression: &str,
    required_num_args: usize,
    vars: &mut dyn FnMut(&str) -> Option<VecVal>,
    world: Option<&dyn World>,
) -> Vec<f64> {
    let v = compile(expression).eval(vars, world).ok();
    match v {
        Some(v) if v.len() == required_num_args => v,
        _ => vec![0.0; required_num_args],
    }
}

/// Upstream isValid: strict parse plus arity check against the parameter
/// types map.
pub fn is_valid(
    expression: &str,
    required_num_args: usize,
    types: &dyn Fn(&str) -> Option<RefType>,
) -> bool {
    if expression.trim().is_empty() {
        return true;
    }
    let mut p = Parser::new(expression, true, required_num_args, Some(types));
    match p.expr() {
        Ok(n) => {
            p.skip_ws();
            if !p.at_end() {
                return false;
            }
            n.num_args() == 0 || n.num_args() == required_num_args
        }
        Err(_) => false,
    }
}

/// Upstream compile: cache of 256 entries, cleared wholesale when full.
pub fn compile(expression: &str) -> VecNode {
    if expression.trim().is_empty() {
        return VecNode::Zero;
    }
    if let Some(cached) = CACHE.with(|c| c.borrow().get(expression).cloned()) {
        return cached;
    }
    let mut compiled = VecNode::Zero;
    let mut p = Parser::new(expression, false, 0, None);
    if let Ok(n) = p.expr() {
        p.skip_ws();
        if p.at_end() {
            compiled = n;
        }
    }
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= CACHE_LIMIT {
            c.clear();
        }
        c.insert(expression.to_string(), compiled.clone());
    });
    compiled
}
}
pub mod runtime {
// HeroBot AI runtime core ported 1:1 from HerobaneNair/HeRoBot
// (herobot-common/.../ai/runtime/{Branch,ControlFrame,JoinState,
// RuntimeVariable,StepResult}.java + herobot-mod/.../ai/runtime/
// {ScriptRunner,ParamEval,ScriptDispatch}.java), CC-BY-NC-4.0.
// Game-bound pieces (ServerPlayer/BotPlayer, PlayerControllers,
// PathExecutor, DataExecutor.asItemStack) sit behind the BotHook
// trait; the real hook is the Horizon fake-player work (issue drafted
// in README). The per-category executors port over coming runs; the
// dispatch table below carries the game-independent control blocks
// until then, exactly like upstream when an executor is missing.

use super::ai::{AiScript, BlockInstance, ParamValue, VarType, Wire};
use super::block_registry::{def_for, BlockCategory, BlockType};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// The dynamic value flowing through params, variables and reporters.
/// Upstream keeps ; bool/number/string mirror ParamValue, and
/// position/rotation/item arrive from the executors.
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeValue {
    Bool(bool),
    Int(i64),
    Double(f64),
    Str(String),
    Vec3(f64, f64, f64),
    Rot(f32, f32),
}

impl RuntimeValue {
    /// Upstream ParamEval.asBool.
    pub fn as_bool(&self) -> bool {
        match self {
            RuntimeValue::Bool(b) => *b,
            RuntimeValue::Int(i) => *i >= 1,
            RuntimeValue::Double(d) => *d >= 1.0,
            RuntimeValue::Str(s) => {
                let t = s.trim();
                match t.parse::<f64>() {
                    Ok(d) => d >= 1.0,
                    Err(_) => t.eq_ignore_ascii_case("true"),
                }
            }
            _ => true,
        }
    }
    /// Upstream ParamEval.asInt.
    pub fn as_int(&self) -> i64 {
        match self {
            RuntimeValue::Bool(b) => *b as i64,
            RuntimeValue::Int(i) => *i,
            RuntimeValue::Double(d) => *d as i64,
            RuntimeValue::Str(s) => s
                .trim()
                .parse::<i64>()
                .or_else(|_| s.trim().parse::<f64>().map(|d| d as i64))
                .unwrap_or(0),
            _ => 0,
        }
    }
    /// Upstream ParamEval.asDouble.
    pub fn as_double(&self) -> f64 {
        match self {
            RuntimeValue::Bool(b) => *b as i64 as f64,
            RuntimeValue::Int(i) => *i as f64,
            RuntimeValue::Double(d) => *d,
            RuntimeValue::Str(s) => s.trim().parse::<f64>().unwrap_or(0.0),
            _ => 0.0,
        }
    }
    /// Upstream ParamEval.asString.
    pub fn as_string(&self) -> String {
        match self {
            RuntimeValue::Str(s) => s.clone(),
            RuntimeValue::Bool(b) => b.to_string(),
            RuntimeValue::Int(i) => i.to_string(),
            RuntimeValue::Double(d) => d.to_string(),
            RuntimeValue::Vec3(x, y, z) => format!("{} {} {}", x, y, z),
            RuntimeValue::Rot(yaw, pitch) => format!("{} {}", yaw, pitch),
        }
    }
}

impl From<&ParamValue> for RuntimeValue {
    fn from(v: &ParamValue) -> Self {
        match v {
            ParamValue::Bool(b) => RuntimeValue::Bool(*b),
            ParamValue::Int(i) => RuntimeValue::Int(*i),
            ParamValue::Double(d) => RuntimeValue::Double(*d),
            ParamValue::Str(s) => RuntimeValue::Str(s.clone()),
        }
    }
}

/// Upstream StepResult.
#[derive(Clone, Debug, PartialEq)]
pub enum StepKind {
    Continue,
    Wait,
    Jump,
    End,
    Handled,
}

#[derive(Clone, Debug)]
pub struct StepResult {
    pub kind: StepKind,
    pub port: i32,
    pub wait_ticks: i32,
    pub jump_to: i32,
    pub push_frame: bool,
    pub pop_frame: bool,
    pub activation_id: i32,
    pub frame_data: Option<BTreeMap<String, RuntimeValue>>,
}

impl StepResult {
    pub fn continue_via(port: i32) -> Self {
        Self { kind: StepKind::Continue, port, wait_ticks: 0, jump_to: 0, push_frame: false, pop_frame: false, activation_id: -1, frame_data: None }
    }
    pub fn enter_body(port: i32, activation_id: i32) -> Self {
        Self { kind: StepKind::Continue, port, wait_ticks: 0, jump_to: 0, push_frame: true, pop_frame: false, activation_id, frame_data: None }
    }
    pub fn exit_body(port: i32) -> Self {
        Self { kind: StepKind::Continue, port, wait_ticks: 0, jump_to: 0, push_frame: false, pop_frame: true, activation_id: -1, frame_data: None }
    }
    pub fn push_and_jump(block_id: i32, activation_id: i32) -> Self {
        Self { kind: StepKind::Jump, port: 0, wait_ticks: 0, jump_to: block_id, push_frame: true, pop_frame: false, activation_id, frame_data: None }
    }
    pub fn push_and_jump_with(block_id: i32, activation_id: i32, data: BTreeMap<String, RuntimeValue>) -> Self {
        Self { kind: StepKind::Jump, port: 0, wait_ticks: 0, jump_to: block_id, push_frame: true, pop_frame: false, activation_id, frame_data: Some(data) }
    }
    pub fn pop_and_jump(block_id: i32) -> Self {
        Self { kind: StepKind::Jump, port: 0, wait_ticks: 0, jump_to: block_id, push_frame: false, pop_frame: true, activation_id: -1, frame_data: None }
    }
    pub fn wait(ticks: i32) -> Self {
        Self { kind: StepKind::Wait, port: 0, wait_ticks: ticks.max(0), jump_to: 0, push_frame: false, pop_frame: false, activation_id: -1, frame_data: None }
    }
    pub fn wait_then_continue(ticks: i32, port: i32) -> Self {
        Self { kind: StepKind::Continue, port, wait_ticks: ticks.max(0), jump_to: 0, push_frame: false, pop_frame: false, activation_id: -1, frame_data: None }
    }
    pub fn jump_to(block_id: i32) -> Self {
        Self { kind: StepKind::Jump, port: 0, wait_ticks: 0, jump_to: block_id, push_frame: false, pop_frame: false, activation_id: -1, frame_data: None }
    }
    pub fn end() -> Self {
        Self { kind: StepKind::End, port: 0, wait_ticks: 0, jump_to: 0, push_frame: false, pop_frame: false, activation_id: -1, frame_data: None }
    }
    pub fn handled() -> Self {
        Self { kind: StepKind::Handled, port: 0, wait_ticks: 0, jump_to: 0, push_frame: false, pop_frame: false, activation_id: -1, frame_data: None }
    }
}

/// Upstream ControlFrame.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlFrame {
    pub container_block_id: i32,
    pub activation_id: i32,
    pub data: BTreeMap<String, RuntimeValue>,
}

impl ControlFrame {
    pub fn new(container_block_id: i32, activation_id: i32) -> Self {
        Self { container_block_id, activation_id, data: BTreeMap::new() }
    }
    pub fn int_data(&self, key: &str, default: i64) -> i64 {
        self.data.get(key).map(RuntimeValue::as_int).unwrap_or(default)
    }
    pub fn set_data(&mut self, key: impl Into<String>, value: RuntimeValue) {
        self.data.insert(key.into(), value);
    }
}

/// Upstream JoinState.
#[derive(Clone, Debug)]
pub struct JoinState {
    pub start_type: BlockType,
    pub start_block_id: i32,
    pub end_block_id: i32,
    pub remaining: i32,
    pub iteration: i32,
}

/// Upstream RuntimeVariable.
#[derive(Clone, Debug)]
pub struct RuntimeVariable {
    pub ty: VarType,
    pub value: RuntimeValue,
}

/// Upstream Branch.
#[derive(Clone, Debug)]
pub struct Branch {
    pub current_block_id: i32,
    pub wait_ticks: i32,
    pub dead: bool,
    pub parked: bool,
    pub turns: i32,
    pub calls_this_tick: i32,
    pub origin_id: i32,
    pub path_wait_for: i32,
    pub event_message: Option<String>,
    pub frames: Vec<ControlFrame>,
}

impl Branch {
    pub fn new(start_block_id: i32) -> Self {
        Self { current_block_id: start_block_id, wait_ticks: 0, dead: false, parked: false, turns: 0, calls_this_tick: 0, origin_id: -1, path_wait_for: -1, event_message: None, frames: Vec::new() }
    }
    pub fn decrement_wait(&mut self) -> bool {
        if self.wait_ticks <= 0 {
            return false;
        }
        self.wait_ticks -= 1;
        true
    }
    pub fn in_activation(&self, activation_id: i32) -> bool {
        self.frames.iter().any(|f| f.activation_id == activation_id)
    }
    /// Upstream Branch.forkAt.
    pub fn fork_at(&self, block_id: i32) -> Branch {
        Branch {
            current_block_id: block_id,
            wait_ticks: 0,
            dead: false,
            parked: false,
            turns: self.turns,
            calls_this_tick: self.calls_this_tick,
            origin_id: self.origin_id,
            path_wait_for: self.path_wait_for,
            event_message: self.event_message.clone(),
            frames: self.frames.clone(),
        }
    }
}

/// The game hooks the runtime and executors call. Implemented by the
/// bot framework once the Horizon fake-player hook lands (see README).
pub trait BotHook {
    /// Position of the acting player (upstream player().position()).
    fn position(&self) -> (f64, f64, f64) {
        (0.0, 0.0, 0.0)
    }
}

/// Reporter evaluation hook (upstream ScriptRunner.evalReporter).
pub trait ReporterEval {
    fn eval_reporter(&self, reporter: &BlockInstance, branch: &Branch) -> Option<RuntimeValue>;
}

/// Upstream ParamEval.
pub struct ParamEval;

impl ParamEval {
    /// raw(): reporter wins over the literal param, then the slot
    /// default. Slot defaults are stored as strings in the ported
    /// registry; coercions below match upstream behavior on them.
    pub fn raw<H: ReporterEval + ?Sized>(
        block: &BlockInstance,
        name: &str,
        host: &H,
        branch: &Branch,
    ) -> RuntimeValue {
        let slot = def_for(BlockType::from_index(block.block_type))
            .and_then(|d| d.param_slot(name));
        let ty = BlockType::from_index(block.block_type);
        let reporter = block.reporter_params.get(name);
        let v = if let Some(rep) = reporter {
            host.eval_reporter(rep, branch).unwrap_or(RuntimeValue::Str(String::new()))
        } else if let Some(p) = block.params.get(name) {
            RuntimeValue::from(p)
        } else if let Some(s) = slot {
            RuntimeValue::Str(s.default_value.clone().unwrap_or_default())
        } else {
            RuntimeValue::Str(String::new())
        };
        Self::coerce_enum(&ty, name, v)
    }

    /// Upstream coerceEnum: numeric index into a non-numeric choice list
    /// resolves to that choice.
    fn coerce_enum(ty: &BlockType, name: &str, v: RuntimeValue) -> RuntimeValue {
        let slot = match def_for(*ty).and_then(|d| d.param_slot(name)) {
            Some(s) if s.ty == super::block_registry::ParamType::Enum => s,
            _ => return v,
        };
        let choices = &slot.enum_choices;
        if choices.is_empty() || choices.iter().all(|c| c.trim().parse::<i64>().is_ok()) {
            return v;
        }
        if let RuntimeValue::Str(s) = &v {
            if choices.contains(&s.trim()) {
                return v;
            }
        }
        let idx = v.as_int();
        if idx < 0 || idx >= choices.len() as i64 {
            return v;
        }
        RuntimeValue::Str(choices[idx as usize].to_string())
    }

    pub fn eval_bool<H: ReporterEval + ?Sized>(b: &BlockInstance, name: &str, h: &H, br: &Branch) -> bool {
        Self::raw(b, name, h, br).as_bool()
    }
    pub fn eval_int<H: ReporterEval + ?Sized>(b: &BlockInstance, name: &str, h: &H, br: &Branch) -> i64 {
        Self::raw(b, name, h, br).as_int()
    }
    pub fn eval_double<H: ReporterEval + ?Sized>(b: &BlockInstance, name: &str, h: &H, br: &Branch) -> f64 {
        Self::raw(b, name, h, br).as_double()
    }
    pub fn eval_string<H: ReporterEval + ?Sized>(b: &BlockInstance, name: &str, h: &H, br: &Branch) -> String {
        Self::raw(b, name, h, br).as_string()
    }

    /// Upstream ParamEval.parseVec3 with ~-relative tokens.
    pub fn parse_vec3(s: &str, base: (f64, f64, f64)) -> (f64, f64, f64) {
        let parts: Vec<&str> = s.trim().split_whitespace().collect();
        if parts.len() < 3 {
            return base;
        }
        let rel = |token: &str, b: f64| -> f64 {
            match token.strip_prefix('~') {
                Some(rest) if rest.is_empty() => b,
                Some(rest) => rest.parse::<f64>().map(|d| b + d).unwrap_or(b),
                None => token.parse::<f64>().unwrap_or(b),
            }
        };
        (rel(parts[0], base.0), rel(parts[1], base.1), rel(parts[2], base.2))
    }

    pub fn as_rotation(v: &RuntimeValue) -> (f32, f32) {
        if let RuntimeValue::Rot(y, p) = v {
            return (*y, *p);
        }
        if let RuntimeValue::Str(s) = v {
            let parts: Vec<&str> = s.trim().split_whitespace().collect();
            if parts.len() >= 2 {
                if let (Ok(y), Ok(p)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                    return (y, p);
                }
            }
        }
        (0.0, 0.0)
    }
}

/// Flow executor signature (upstream Executor); reporters use the
/// ReporterEval hook. Registered by the per-category executor ports.
pub type FlowExec = fn(&mut ScriptRunner, &BlockInstance, &mut Branch) -> StepResult;

fn flow_dispatch(ty: BlockType) -> Option<FlowExec> {
    // Partial until the executor files land; upstream logs and ends the
    // branch on a missing executor, and so does this.
    match ty {
        BlockType::Wait => Some(exec_wait),
        BlockType::WaitUntil => Some(exec_wait_until),
        BlockType::StopScript => Some(exec_stop_script),
        BlockType::StopAll => Some(exec_end),
        BlockType::PauseScript => Some(exec_pause_script),
        BlockType::SetScript => Some(exec_set_script),
        BlockType::SetVar => Some(exec_set_var),
        BlockType::ChangeVar => Some(exec_change_var),
        BlockType::Break => Some(exec_break),
        BlockType::If => Some(exec_if),
        BlockType::ElseIf => Some(exec_else_if),
        BlockType::For => Some(exec_for),
        BlockType::While => Some(exec_while),
        BlockType::BlockEnd => Some(exec_block_end),
        _ => None,
    }
}

/// Upstream ControlExecutor WAIT: waitThenContinue(max(1, ticks), 0).
fn exec_wait(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    let ticks = ParamEval::eval_int(b, "ticks", r, br) as i32;
    StepResult::wait_then_continue(ticks.max(1), 0)
}

fn exec_end(_: &mut ScriptRunner, _: &BlockInstance, _: &mut Branch) -> StepResult {
    StepResult::end()
}

// ===== Upstream ControlExecutor (executors/ControlExecutor.java) =====

/// Upstream LOOP_ITER reporter: iteration count of the active loop whose
/// container block is this reporter's paired block.
pub fn eval_control_reporter(b: &BlockInstance, r: &ScriptRunner, br: &Branch) -> Option<RuntimeValue> {
    if BlockType::from_index(b.block_type) != BlockType::LoopIter {
        return None;
    }
    for f in &br.frames {
        if f.container_block_id != b.paired_id {
            continue;
        }
        if let Some(js) = r.join_state(f.activation_id) {
            return Some(RuntimeValue::Int(js.iteration as i64));
        }
    }
    Some(RuntimeValue::Int(0))
}

/// Upstream FunctionExecutor.isCallFrame.
pub fn is_call_frame(f: &ControlFrame) -> bool {
    f.data.contains_key("fnCall")
}

fn skip_to_end(start: &BlockInstance, activation_id: i32) -> StepResult {
    if start.paired_id < 0 {
        return StepResult::end();
    }
    StepResult::push_and_jump(start.paired_id, activation_id)
}

/// Upstream ControlExecutor.sideTarget: the ELSE_IF chained off port 1.
fn side_target(r: &ScriptRunner, block_id: i32) -> Option<BlockInstance> {
    for w in r.outgoing(block_id, 1) {
        if let Some(t) = r.script.block(w.to_block_id) {
            if BlockType::from_index(t.block_type) == BlockType::ElseIf {
                return Some(t.clone());
            }
        }
    }
    None
}

/// Upstream ControlExecutor.chainRoot: walk the ELSE_IF side chain back
/// to its IF over port-1 incoming wires.
fn chain_root(r: &ScriptRunner, elif_id: i32) -> Option<BlockInstance> {
    let mut visited = std::collections::HashSet::new();
    let mut current = elif_id;
    while visited.insert(current) {
        let mut source: Option<BlockInstance> = None;
        for w in r.script.wires.iter().filter(|w| w.to_block_id == current) {
            if w.out_port != 1 {
                continue;
            }
            source = r.script.block(w.from_block_id).cloned();
            break;
        }
        let source = match source {
            Some(s) => s,
            None => return None,
        };
        match BlockType::from_index(source.block_type) {
            BlockType::If => return Some(source),
            BlockType::ElseIf => current = source.id,
            _ => return None,
        }
    }
    None
}

/// Upstream ControlExecutor WAIT_UNTIL.
fn exec_wait_until(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    if ParamEval::eval_bool(b, "condition", r, br) {
        return StepResult::continue_via(0);
    }
    StepResult::wait(1)
}

/// Upstream ControlExecutor BREAK: unwind to the nth enclosing loop,
/// killing every activation it passes, then jump past the loop end.
fn exec_break(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    let want = ParamEval::eval_int(b, "count", r, br).max(1) as usize;
    let mut target: Option<ControlFrame> = None;
    let mut end_id = -1;
    let mut found = 0;
    for f in br.frames.iter() {
        if is_call_frame(f) {
            break;
        }
        if let Some(js) = r.join_state(f.activation_id) {
            target = Some(f.clone());
            end_id = js.end_block_id;
            found += 1;
            if found >= want {
                break;
            }
        }
    }
    let target = match target {
        Some(t) => t,
        None => {
            return if r.return_from_function(br) {
                StepResult::handled()
            } else {
                StepResult::end()
            };
        }
    };
    while !br.frames.is_empty() {
        let f = br.frames.pop().expect("frames nonempty");
        if r.join_state(f.activation_id).is_some() {
            r.kill_activation(f.activation_id);
            r.remove_join(f.activation_id);
        }
        if f == target {
            break;
        }
    }
    for w in r.outgoing(end_id, 0) {
        return StepResult::jump_to(w.to_block_id);
    }
    if r.return_from_function(br) {
        StepResult::handled()
    } else {
        StepResult::end()
    }
}

/// Upstream STOP_SCRIPT clears the player's script registry; the
/// runner-level equivalent is stopping the runner (the registry layer
/// lands with the bot framework).
fn exec_stop_script(_: &mut ScriptRunner, _: &BlockInstance, _: &mut Branch) -> StepResult {
    // Upstream clears the player's script registry (game-bound, lands
    // with the bot framework), then ends the branch.
    StepResult::end()
}

/// Upstream PAUSE_SCRIPT.
fn exec_pause_script(r: &mut ScriptRunner, _: &BlockInstance, _: &mut Branch) -> StepResult {
    r.pause();
    StepResult::continue_via(0)
}

/// Upstream SET_SCRIPT swaps the running script through AiScriptRegistry
/// (load/assign/fireStart on the player). The registry is game-bound and
/// lands with the bot framework, so until then this ends the branch the
/// way an unknown script name would.
// ===== Upstream DataExecutor (executors/DataExecutor.java) =====
// UUID/ITEM coercion, entity name lookup and item-stack parsing are
// game-bound and land with the BotHook fake-player work.

/// Portable subset of upstream ParamEval.asVec3: literal triples.
fn as_vec3_value(v: &RuntimeValue) -> (f64, f64, f64) {
    match v {
        RuntimeValue::Vec3(x, y, z) => (*x, *y, *z),
        RuntimeValue::Str(s) => {
            let parts: Vec<f64> = s.split_whitespace().filter_map(|q| q.parse::<f64>().ok()).collect();
            if parts.len() >= 3 { (parts[0], parts[1], parts[2]) } else { (0.0, 0.0, 0.0) }
        }
        _ => (0.0, 0.0, 0.0),
    }
}

/// Upstream DataExecutor.coerce, portable subset.
fn data_coerce(ty: VarType, v: &RuntimeValue) -> RuntimeValue {
    match ty {
        VarType::Bool => RuntimeValue::Bool(v.as_bool()),
        VarType::Int => RuntimeValue::Int(v.as_int()),
        VarType::Double => RuntimeValue::Double(v.as_double()),
        VarType::String => RuntimeValue::Str(v.as_string()),
        VarType::Position => { let (x, y, z) = as_vec3_value(v); RuntimeValue::Vec3(x, y, z) }
        VarType::Rotation => { let (yaw, pitch) = ParamEval::as_rotation(v); RuntimeValue::Rot(yaw, pitch) }
        _ => v.clone(),
    }
}

/// Upstream DataExecutor.stringify, portable subset (item and entity
/// names resolve in the game layer).
fn stringify_value(v: &RuntimeValue) -> String {
    match v {
        RuntimeValue::Vec3(x, y, z) => format!("{} {} {}", x, y, z),
        RuntimeValue::Rot(yaw, pitch) => format!("{} {}", yaw, pitch),
        other => other.as_string(),
    }
}

/// Upstream DataExecutor SET_VAR.
fn exec_set_var(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    let name = ParamEval::eval_string(b, "name", r, br);
    let value = ParamEval::raw(b, "value", r, br);
    match r.variable(&name).map(|v| v.ty) {
        Some(ty) => {
            let nv = data_coerce(ty, &value);
            if let Some(v) = r.variable_mut(&name) { v.value = nv; }
        }
        None => {
            r.define_variable(name, RuntimeVariable { ty: VarType::String, value: RuntimeValue::Str(value.as_string()) });
        }
    }
    StepResult::continue_via(0)
}

/// Upstream DataExecutor CHANGE_VAR.
fn exec_change_var(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    let name = ParamEval::eval_string(b, "name", r, br);
    let existing = r.variable(&name).map(|v| (v.ty, v.value.clone()));
    match existing {
        None => {
            let d = ParamEval::eval_double(b, "delta", r, br);
            r.define_variable(name, RuntimeVariable { ty: VarType::Double, value: RuntimeValue::Double(d) });
        }
        Some((ty, cur)) => {
            let nv = match ty {
                VarType::Bool => RuntimeValue::Bool(!cur.as_bool()),
                VarType::Int => RuntimeValue::Int(cur.as_int() + ParamEval::eval_int(b, "delta", r, br)),
                VarType::Double => RuntimeValue::Double(cur.as_double() + ParamEval::eval_double(b, "delta", r, br)),
                VarType::Position => {
                    let (dx, dy, dz) = as_vec3_value(&ParamEval::raw(b, "delta", r, br));
                    let (cx, cy, cz) = as_vec3_value(&cur);
                    RuntimeValue::Vec3(cx + dx, cy + dy, cz + dz)
                }
                VarType::Rotation => {
                    let (dy, dp) = ParamEval::as_rotation(&ParamEval::raw(b, "delta", r, br));
                    let (cy, cp) = ParamEval::as_rotation(&cur);
                    RuntimeValue::Rot(cy + dy, cp + dp)
                }
                VarType::String => RuntimeValue::Str(cur.as_string() + &ParamEval::raw(b, "delta", r, br).as_string()),
                VarType::Uuid | VarType::Item => cur.clone(),
            };
            if let Some(v) = r.variable_mut(&name) { v.value = nv; }
        }
    }
    StepResult::continue_via(0)
}

fn exec_set_script(_: &mut ScriptRunner, _: &BlockInstance, _: &mut Branch) -> StepResult {
    StepResult::end()
}

/// Upstream ControlExecutor IF.
fn exec_if(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    let act = r.open_activation(BlockType::If, b.id, b.paired_id, 0);
    if ParamEval::eval_bool(b, "condition", r, br) {
        return StepResult::enter_body(0, act);
    }
    let elif = side_target(r, b.id);
    if elif.is_some() && b.paired_id >= 0 {
        return StepResult::push_and_jump(elif.expect("checked").id, act);
    }
    skip_to_end(b, act)
}

/// Upstream ControlExecutor ELSE_IF.
fn exec_else_if(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    if ParamEval::eval_bool(b, "condition", r, br) {
        let act = r.open_activation(BlockType::ElseIf, b.id, b.paired_id, 0);
        return StepResult::enter_body(0, act);
    }
    if let Some(next) = side_target(r, b.id) {
        return StepResult::jump_to(next.id);
    }
    if let Some(root) = chain_root(r, b.id) {
        if root.paired_id >= 0 {
            return StepResult::jump_to(root.paired_id);
        }
    }
    let act = r.open_activation(BlockType::ElseIf, b.id, b.paired_id, 0);
    skip_to_end(b, act)
}

/// Upstream ControlExecutor FOR.
fn exec_for(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    let count = ParamEval::eval_int(b, "count", r, br) as i32;
    let act = r.open_activation(BlockType::For, b.id, b.paired_id, count);
    if count <= 0 {
        return skip_to_end(b, act);
    }
    StepResult::enter_body(0, act)
}

/// Upstream ControlExecutor WHILE.
fn exec_while(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    let act = r.open_activation(BlockType::While, b.id, b.paired_id, 0);
    if !ParamEval::eval_bool(b, "condition", r, br) {
        return skip_to_end(b, act);
    }
    StepResult::enter_body(0, act)
}

/// Upstream ControlExecutor BLOCK_END / stepEnd.
fn exec_block_end(r: &mut ScriptRunner, b: &BlockInstance, br: &mut Branch) -> StepResult {
    let act = br.frames.last().map(|f| f.activation_id).unwrap_or(-1);
    let js = match r.join_state(act) {
        Some(js) => js.clone(),
        None => return StepResult::continue_via(0),
    };
    if r.running_count(act) != 0 {
        return StepResult::end();
    }
    r.kill_activation(act);
    let mut repeat = false;
    match js.start_type {
        BlockType::For => {
            let rem = js.remaining - 1;
            if let Some(j) = r.join_state_mut(act) {
                j.remaining = rem;
            }
            repeat = rem > 0;
        }
        BlockType::While => {
            let start = r.script.block(js.start_block_id).cloned();
            repeat = match start {
                Some(s) => ParamEval::eval_bool(&s, "condition", r, br),
                None => false,
            };
        }
        _ => {}
    }
    if repeat {
        if let Some(j) = r.join_state_mut(act) {
            j.iteration += 1;
        }
        let js_now = r.join_state(act).cloned().unwrap_or_else(|| js.clone());
        r.relaunch_body(&js_now, br);
        return StepResult::handled();
    }
    r.remove_join(act);
    if b.paired_id >= 0 {
        if let Some(start) = r.script.block(b.paired_id).cloned() {
            if BlockType::from_index(start.block_type) == BlockType::ElseIf {
                if let Some(root) = chain_root(r, start.id) {
                    if root.paired_id >= 0 {
                        return StepResult::pop_and_jump(root.paired_id);
                    }
                }
            }
        }
    }
    StepResult::exit_body(0)
}

const MAX_CHAT_QUEUE: usize = 256;
const STEP_BUDGET: usize = 1_000_000;
const MAX_TURNS_PER_TICK: i32 = 256;

/// Upstream ScriptRunner.
pub struct ScriptRunner {
    pub script: AiScript,
    pub branches: Vec<Branch>,
    joins: HashMap<i32, JoinState>,
    variables: HashMap<String, RuntimeVariable>,
    activation_seq: i32,
    call_seq: u64,
    out_index: HashMap<(i32, i32), Vec<Wire>>,
    in_index: HashMap<i32, Vec<Wire>>,
    hat_index: HashMap<i32, Vec<BlockInstance>>,
    paused: bool,
    damaged_this_tick: bool,
    path_reached_this_tick: bool,
    pub world: Option<Box<dyn super::vec_eval::World>>,
    pub sensor: Option<Box<dyn super::sensor::SensorHook>>,
    path_failed_this_tick: bool,
    chat_queue: VecDeque<String>,
}

// ===== Upstream OperatorExecutor, portable reporters =====
// NUM_CALC is live through the expr.rs port of upstream's ExprEval;
// BOOL_CALC/STRING_CALC through bool_eval.rs and str_eval.rs (upstream
// BoolEval/StrEval); POS_CALC/DIR_CALC through vec_eval.rs (upstream
// VecEval). VecEval's world seam is a World trait object the game layer
// supplies (player level, position and rotation); the harness runs worldless.
// RANDOM_* uses a thread-local xorshift* seeded from the clock,
// standing in for ThreadLocalRandom.

thread_local! {
    static RNG_STATE: std::cell::Cell<u64> = std::cell::Cell::new(0);
}

fn rng_next_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    RNG_STATE.with(|st| {
        let mut x = st.get();
        if x == 0 {
            x = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E3779B97F4A7C15);
        }
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        st.set(x);
        x.wrapping_mul(0x2545F4914F6CDD1D)
    })
}

/// Upstream OperatorExecutor.valuesEqual, portable subset (item stacks
/// and entity refs live in the game layer).
fn op_values_equal(a: &RuntimeValue, b: &RuntimeValue) -> bool {
    match (a, b) {
        (RuntimeValue::Int(x), RuntimeValue::Int(y)) => x == y,
        (RuntimeValue::Double(x), RuntimeValue::Double(y)) => x == y,
        (RuntimeValue::Int(x), RuntimeValue::Double(y)) | (RuntimeValue::Double(y), RuntimeValue::Int(x)) => (*x as f64) == *y,
        (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x == y,
        (RuntimeValue::Str(x), RuntimeValue::Str(y)) => x == y,
        (RuntimeValue::Vec3(x1, y1, z1), RuntimeValue::Vec3(x2, y2, z2)) => x1 == x2 && y1 == y2 && z1 == z2,
        (RuntimeValue::Rot(a1, a2), RuntimeValue::Rot(b1, b2)) => a1 == b1 && a2 == b2,
        _ => false,
    }
}

impl ReporterEval for ScriptRunner {
    fn eval_reporter(&self, reporter: &BlockInstance, _branch: &Branch) -> Option<RuntimeValue> {
        // Upstream ControlExecutor.register reporter table (LOOP_ITER).
        if let Some(v) = eval_control_reporter(reporter, self, _branch) {
            return Some(v);
        }
        let ty = BlockType::from_index(reporter.block_type);
        match ty {
            BlockType::ReadVar => {
                let name = ParamEval::eval_string(reporter, "name", self, _branch);
                return self.variable(&name).map(|v| v.value.clone());
            }
            BlockType::ToString => {
                let v = ParamEval::raw(reporter, "value", self, _branch);
                return Some(RuntimeValue::Str(stringify_value(&v)));
            }
            BlockType::Ternary => {
                let cond = ParamEval::eval_bool(reporter, "condition", self, _branch);
                let pick = if cond { "trueValue" } else { "falseValue" };
                return Some(ParamEval::raw(reporter, pick, self, _branch));
            }
            BlockType::Compare => {
                let a = ParamEval::eval_double(reporter, "a", self, _branch);
                let bv = ParamEval::eval_double(reporter, "b", self, _branch);
                let op = ParamEval::eval_string(reporter, "op", self, _branch);
                let res = match op.as_str() {
                    ">" => a > bv,
                    "=" => a == bv,
                    "\u{2260}" => a != bv,
                    "\u{2264}" => a <= bv,
                    "\u{2265}" => a >= bv,
                    _ => a < bv,
                };
                return Some(RuntimeValue::Bool(res));
            }
            BlockType::Equality => {
                let a = ParamEval::raw(reporter, "a", self, _branch);
                let b = ParamEval::raw(reporter, "b", self, _branch);
                let eq = op_values_equal(&a, &b);
                let neq = ParamEval::eval_string(reporter, "op", self, _branch) == "\u{2260}";
                return Some(RuntimeValue::Bool(neq != eq));
            }
            BlockType::Logic => {
                let op = ParamEval::eval_string(reporter, "op", self, _branch);
                let a = ParamEval::eval_bool(reporter, "a", self, _branch);
                let bv = ParamEval::eval_bool(reporter, "b", self, _branch);
                let res = match op.as_str() {
                    "or" => a || bv,
                    "xor" => a ^ bv,
                    _ => a && bv,
                };
                return Some(RuntimeValue::Bool(res));
            }
            BlockType::And => {
                let res = ParamEval::eval_bool(reporter, "a", self, _branch) && ParamEval::eval_bool(reporter, "b", self, _branch);
                return Some(RuntimeValue::Bool(res));
            }
            BlockType::Or => {
                let res = ParamEval::eval_bool(reporter, "a", self, _branch) || ParamEval::eval_bool(reporter, "b", self, _branch);
                return Some(RuntimeValue::Bool(res));
            }
            BlockType::Not => {
                let res = !ParamEval::eval_bool(reporter, "a", self, _branch);
                return Some(RuntimeValue::Bool(res));
            }
            BlockType::Contains => {
                let mut a = ParamEval::eval_string(reporter, "a", self, _branch);
                let mut needle = ParamEval::eval_string(reporter, "b", self, _branch);
                if !ParamEval::eval_bool(reporter, "checkCase", self, _branch) {
                    a = a.to_lowercase();
                    needle = needle.to_lowercase();
                }
                return Some(RuntimeValue::Bool(a.contains(&needle)));
            }
            BlockType::RandomInt => {
                let mut lo = ParamEval::eval_int(reporter, "min", self, _branch);
                let mut hi = ParamEval::eval_int(reporter, "max", self, _branch);
                if hi < lo { std::mem::swap(&mut lo, &mut hi); }
                let span = (hi as i128 - lo as i128 + 1).max(1);
                let val = lo as i128 + (rng_next_u64() as i128 % span);
                return Some(RuntimeValue::Int(val as i64));
            }
            BlockType::RandomDouble => {
                let lo = ParamEval::eval_double(reporter, "min", self, _branch);
                let hi = ParamEval::eval_double(reporter, "max", self, _branch);
                let (lo, hi) = if hi < lo { (hi, lo) } else { (lo, hi) };
                let u = (rng_next_u64() >> 11) as f64 / (1u64 << 53) as f64;
                return Some(RuntimeValue::Double(lo + (hi - lo) * u));
            }
            BlockType::Vec3 => {
                let x = ParamEval::eval_double(reporter, "x", self, _branch);
                let y = ParamEval::eval_double(reporter, "y", self, _branch);
                let z = ParamEval::eval_double(reporter, "z", self, _branch);
                return Some(RuntimeValue::Vec3(x, y, z));
            }
            BlockType::Rot => {
                let yaw = ParamEval::eval_double(reporter, "yaw", self, _branch) as f32;
                let pitch = ParamEval::eval_double(reporter, "pitch", self, _branch) as f32;
                return Some(RuntimeValue::Rot(yaw, pitch));
            }
            BlockType::NumCalc => {
                // Upstream OperatorExecutor NUM_CALC: {name} resolves
                // against the runner's variables, Input<n> against this
                // block's own params; [i] indexes positions, rotations
                // (upstream float[]), and nothing else.
                let expr_text = ParamEval::eval_string(reporter, "expression", self, _branch);
                let mut vars = |name: &str, idx: i32| -> f64 {
                    let raw = if super::expr::is_input_ref(name) {
                        ParamEval::raw(reporter, name, self, _branch)
                    } else {
                        match self.variable(name) {
                            Some(v) => v.value.clone(),
                            None => return 0.0,
                        }
                    };
                    if idx >= 0 {
                        match raw {
                            RuntimeValue::Vec3(x, y, z) => match idx {
                                0 => x,
                                1 => y,
                                2 => z,
                                _ => 0.0,
                            },
                            RuntimeValue::Rot(yaw, pitch) => match idx {
                                0 => yaw as f64,
                                1 => pitch as f64,
                                _ => 0.0,
                            },
                            _ => 0.0,
                        }
                    } else {
                        raw.as_double()
                    }
                };
                return Some(RuntimeValue::Double(super::expr::eval(
                    &expr_text,
                    &mut vars,
                )));
            }
            BlockType::BoolCalc => {
                // Upstream OperatorExecutor BOOL_CALC.
                let expr_text = ParamEval::eval_string(reporter, "expression", self, _branch);
                let mut vars = |name: &str| -> Option<super::bool_eval::BoolVal> {
                    let raw = if super::expr::is_input_ref(name) {
                        ParamEval::raw(reporter, name, self, _branch)
                    } else {
                        match self.variable(name) {
                            Some(v) => v.value.clone(),
                            None => return None,
                        }
                    };
                    Some(match &raw {
                        RuntimeValue::Bool(b) => super::bool_eval::BoolVal::Bool(*b),
                        RuntimeValue::Int(i) => super::bool_eval::BoolVal::Num(*i as f64),
                        RuntimeValue::Double(d) => super::bool_eval::BoolVal::Num(*d),
                        RuntimeValue::Str(s) => super::bool_eval::BoolVal::Str(s.clone()),
                        RuntimeValue::Rot(y, p) => {
                            super::bool_eval::BoolVal::Arr(vec![*y as f64, *p as f64])
                        }
                        RuntimeValue::Vec3(x, y, z) => super::bool_eval::BoolVal::Other(format!(
                            "{} {} {}",
                            x, y, z
                        )),
                    })
                };
                return Some(RuntimeValue::Bool(super::bool_eval::eval(
                    &expr_text,
                    &mut vars,
                )));
            }
            BlockType::StringCalc => {
                // Upstream OperatorExecutor STRING_CALC: {var} and Input<n>
                // resolve through DataExecutor.stringify; missing variables
                // stringify to the empty string.
                let expr_text = ParamEval::eval_string(reporter, "expression", self, _branch);
                let mut vars = |name: &str| -> Option<String> {
                    let raw = if super::expr::is_input_ref(name) {
                        ParamEval::raw(reporter, name, self, _branch)
                    } else {
                        match self.variable(name) {
                            Some(v) => v.value.clone(),
                            None => return None,
                        }
                    };
                    Some(stringify_value(&raw))
                };
                return Some(RuntimeValue::Str(super::str_eval::eval(
                    &expr_text,
                    &mut vars,
                )));
            }
            BlockType::PosCalc => {
                // Upstream OperatorExecutor POS_CALC: VecEval over the level
                // world. The world hook is a World trait object the game
                // layer attaches (upstream levelWorld: the player's level,
                // position and rotation); without it ~ bases resolve to 0,
                // ^ locals skip the world basis and climb/fall pass through.
                let expr_text = ParamEval::eval_string(reporter, "expression", self, _branch);
                let mut vars = |name: &str| -> Option<super::vec_eval::VecVal> {
                    let raw = if super::expr::is_input_ref(name) {
                        ParamEval::raw(reporter, name, self, _branch)
                    } else {
                        match self.variable(name) {
                            Some(v) => v.value.clone(),
                            None => return None,
                        }
                    };
                    Some(match raw {
                        RuntimeValue::Vec3(x, y, z) => super::vec_eval::VecVal::Arr(vec![x, y, z]),
                        RuntimeValue::Rot(y, p) => super::vec_eval::VecVal::Rot([y as f64, p as f64]),
                        RuntimeValue::Int(i) => super::vec_eval::VecVal::Num(i as f64),
                        RuntimeValue::Double(d) => super::vec_eval::VecVal::Num(d),
                        RuntimeValue::Bool(b) => super::vec_eval::VecVal::Bool(b),
                        RuntimeValue::Str(_) => super::vec_eval::VecVal::Other,
                    })
                };
                let world = self.world.as_deref();
                let v = super::vec_eval::eval(&expr_text, 3, &mut vars, world);
                return Some(RuntimeValue::Vec3(v[0], v[1], v[2]));
            }
            BlockType::DirCalc => {
                // Upstream OperatorExecutor DIR_CALC: same vars hook, required
                // arity 2, result cast to floats (upstream float[]).
                let expr_text = ParamEval::eval_string(reporter, "expression", self, _branch);
                let mut vars = |name: &str| -> Option<super::vec_eval::VecVal> {
                    let raw = if super::expr::is_input_ref(name) {
                        ParamEval::raw(reporter, name, self, _branch)
                    } else {
                        match self.variable(name) {
                            Some(v) => v.value.clone(),
                            None => return None,
                        }
                    };
                    Some(match raw {
                        RuntimeValue::Vec3(x, y, z) => super::vec_eval::VecVal::Arr(vec![x, y, z]),
                        RuntimeValue::Rot(y, p) => super::vec_eval::VecVal::Rot([y as f64, p as f64]),
                        RuntimeValue::Int(i) => super::vec_eval::VecVal::Num(i as f64),
                        RuntimeValue::Double(d) => super::vec_eval::VecVal::Num(d),
                        RuntimeValue::Bool(b) => super::vec_eval::VecVal::Bool(b),
                        RuntimeValue::Str(_) => super::vec_eval::VecVal::Other,
                    })
                };
                let world = self.world.as_deref();
                let v = super::vec_eval::eval(&expr_text, 2, &mut vars, world);
                return Some(RuntimeValue::Rot(v[0] as f32, v[1] as f32));
            }
            // The Sensor and Function reporters land with those executors.
            // Upstream returns null on a missing reporter.
            _ => {
                // Upstream SensorExecutor: the world-bound sensor reporters
                // dispatch to the sensor port; the hookless fallback matches
                // upstream with no live world.
                if super::sensor::is_sensor_type(ty) {
                    return super::sensor::sensor_report(ty, reporter, _branch, self);
                }
            }
        }
        None
    }
}

impl ScriptRunner {
    pub fn new(script: AiScript) -> Self {
        let mut r = Self {
            script,
            branches: Vec::new(),
            joins: HashMap::new(),
            variables: HashMap::new(),
            activation_seq: 0,
            call_seq: 0,
            out_index: HashMap::new(),
            in_index: HashMap::new(),
            hat_index: HashMap::new(),
            paused: false,
            damaged_this_tick: false,
            path_reached_this_tick: false,
            world: None,
            sensor: None,
            path_failed_this_tick: false,
            chat_queue: VecDeque::new(),
        };
        r.build_indexes();
        r.init_variables();
        r
    }

    fn build_indexes(&mut self) {
        let wires = self.script.wires.clone();
        for w in wires {
            self.out_index.entry((w.from_block_id, w.out_port)).or_default().push(w.clone());
            self.in_index.entry(w.to_block_id).or_default().push(w);
        }
        for bucket in self.out_index.values_mut() {
            if bucket.len() > 1 {
                let script = &self.script;
                bucket.sort_by(|a, b| {
                    let ba = script.block(a.to_block_id);
                    let bb = script.block(b.to_block_id);
                    match (ba, bb) {
                        (Some(ba), Some(bb)) => ba.x.partial_cmp(&bb.x).unwrap_or(std::cmp::Ordering::Equal).then(ba.y.partial_cmp(&bb.y).unwrap_or(std::cmp::Ordering::Equal)),
                        _ => std::cmp::Ordering::Equal,
                    }
                });
            }
        }
    }

    pub fn outgoing(&self, block_id: i32, out_port: i32) -> Vec<Wire> {
        self.out_index.get(&(block_id, out_port)).cloned().unwrap_or_default()
    }

    pub fn hats(&mut self, ty: BlockType) -> Vec<BlockInstance> {
        if let Some(v) = self.hat_index.get(&ty.index()) {
            return v.clone();
        }
        let v: Vec<BlockInstance> = self.script.hat_blocks(ty.index()).into_iter().cloned().collect();
        self.hat_index.insert(ty.index(), v.clone());
        v
    }

    fn init_variables(&mut self) {
        let vars = self.script.variables.clone();
        for v in vars {
            // ITEM defaults resolve through DataExecutor.asItemStack once
            // that executor lands; the raw string stands until then.
            self.variables.insert(
                v.qualified_name(),
                RuntimeVariable { ty: v.ty, value: RuntimeValue::Str(v.default_value.clone().unwrap_or_default()) },
            );
        }
    }

    pub fn variable(&self, name: &str) -> Option<&RuntimeVariable> {
        self.variables.get(name)
    }

    pub fn define_variable(&mut self, name: impl Into<String>, v: RuntimeVariable) {
        self.variables.insert(name.into(), v);
    }

    pub fn remove_variable(&mut self, name: &str) {
        self.variables.remove(name);
    }

    pub fn variable_mut(&mut self, name: &str) -> Option<&mut RuntimeVariable> {
        self.variables.get_mut(name)
    }

    pub fn next_call_seq(&mut self) -> u64 {
        self.call_seq += 1;
        self.call_seq
    }

    pub fn open_activation(&mut self, start_type: BlockType, start_id: i32, end_id: i32, remaining: i32) -> i32 {
        self.activation_seq += 1;
        let act = self.activation_seq;
        self.joins.insert(act, JoinState { start_type, start_block_id: start_id, end_block_id: end_id, remaining, iteration: 0 });
        act
    }

    pub fn join_state(&self, activation_id: i32) -> Option<&JoinState> {
        self.joins.get(&activation_id)
    }

    pub fn remove_join(&mut self, activation_id: i32) {
        self.joins.remove(&activation_id);
    }

    pub fn join_state_mut(&mut self, activation_id: i32) -> Option<&mut JoinState> {
        self.joins.get_mut(&activation_id)
    }

    /// Upstream runningCount over live branches still inside an activation.
    pub fn running_count(&self, activation_id: i32) -> i32 {
        self.branches
            .iter()
            .filter(|b| !b.dead && b.in_activation(activation_id))
            .count() as i32
    }

    pub fn kill_activation(&mut self, activation_id: i32) {
        for b in &mut self.branches {
            if !b.dead && b.in_activation(activation_id) {
                b.dead = true;
            }
        }
    }

    pub fn relaunch_body(&mut self, js: &JoinState, br: &mut Branch) {
        let start = js.start_block_id;
        self.navigate(br, start, 0, &mut Vec::new());
    }

    pub fn fire_start(&mut self) {
        for start in self.hats(BlockType::Start) {
            self.spawn_from_event(&start, None);
        }
    }

    pub fn fire_event(&mut self, ty: BlockType) {
        if self.paused {
            return;
        }
        for b in self.hats(ty) {
            if self.is_event_active(b.id) {
                continue;
            }
            self.spawn_from_event(&b, None);
        }
    }

    pub fn enqueue_chat_message(&mut self, message: impl Into<String>) {
        let message = message.into();
        while self.chat_queue.len() >= MAX_CHAT_QUEUE {
            self.chat_queue.pop_front();
        }
        self.chat_queue.push_back(message);
    }

    fn dispatch_message(&mut self, message: String) {
        for b in self.hats(BlockType::OnMessage) {
            self.spawn_from_event(&b, Some(message.clone()));
        }
    }

    fn fire_toggle_events(&mut self) {
        let toggles = self.hats(BlockType::OnToggle);
        if toggles.is_empty() {
            return;
        }
        for hat in toggles {
            if self.is_event_active(hat.id) {
                continue;
            }
            let tmp = Branch::new(hat.id);
            if ParamEval::eval_bool(&hat, "condition", self, &tmp) {
                self.spawn_from_event(&hat, None);
            }
        }
    }

    fn is_event_active(&self, event_id: i32) -> bool {
        self.branches.iter().any(|br| !br.dead && br.origin_id == event_id)
    }

    fn spawn_from_event(&mut self, event: &BlockInstance, payload: Option<String>) {
        let outs = self.outgoing(event.id, 0);
        for w in outs {
            let mut br = Branch::new(w.to_block_id);
            br.origin_id = event.id;
            br.event_message = payload.clone();
            self.branches.push(br);
        }
    }

    pub fn mark_damaged(&mut self) {
        self.damaged_this_tick = true;
    }
    pub fn was_damaged(&self) -> bool {
        self.damaged_this_tick
    }
    pub fn mark_path_reached(&mut self) {
        self.path_reached_this_tick = true;
    }
    pub fn mark_path_failed(&mut self) {
        self.path_failed_this_tick = true;
    }
    pub fn path_reached(&self) -> bool {
        self.path_reached_this_tick
    }
    pub fn path_failed(&self) -> bool {
        self.path_failed_this_tick
    }

    pub fn pause(&mut self) {
        if self.paused {
            return;
        }
        self.paused = true;
        for b in &mut self.branches {
            b.path_wait_for = -1;
        }
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn stop(&mut self) {
        for b in &mut self.branches {
            b.dead = true;
        }
        self.branches.clear();
        self.joins.clear();
        self.chat_queue.clear();
        self.paused = false;
    }

    /// Upstream tick.
    pub fn tick(&mut self) {
        if self.paused {
            self.damaged_this_tick = false;
            self.path_reached_this_tick = false;
            self.path_failed_this_tick = false;
            return;
        }
        self.fire_toggle_events();
        while let Some(chat) = self.chat_queue.pop_front() {
            self.dispatch_message(chat);
        }
        if self.branches.is_empty() {
            self.damaged_this_tick = false;
            self.path_reached_this_tick = false;
            self.path_failed_this_tick = false;
            return;
        }

        for br in &mut self.branches {
            if br.dead {
                continue;
            }
            br.turns = 0;
            br.calls_this_tick = 0;
            br.parked = br.decrement_wait();
        }

        let mut any_ran = true;
        while any_ran {
            any_ran = false;
            let count = self.branches.len();
            for i in 0..count {
                let (dead, parked, over) = {
                    let br = &self.branches[i];
                    (br.dead, br.parked, br.turns >= MAX_TURNS_PER_TICK)
                };
                if dead || parked || over {
                    continue;
                }
                self.branches[i].turns += 1;
                let mut br = std::mem::replace(&mut self.branches[i], Branch::new(-1));
                let mut spawned: Vec<Branch> = Vec::new();
                self.run_slice(&mut br, &mut spawned);
                // Always write the branch back: a dead branch must mark its
                // slot dead too, or the placeholder re-runs forever.
                self.branches[i] = br;
                self.branches.extend(spawned);
                any_ran = true;
            }
        }
        self.branches.retain(|b| !b.dead);
        self.damaged_this_tick = false;
        self.path_reached_this_tick = false;
        self.path_failed_this_tick = false;
    }

    fn run_slice(&mut self, br: &mut Branch, spawned: &mut Vec<Branch>) {
        for _budget in 0..STEP_BUDGET {
            let b = match self.script.block(br.current_block_id) {
                Some(b) => b.clone(),
                None => {
                    br.dead = true;
                    return;
                }
            };
            let exec = match flow_dispatch(BlockType::from_index(b.block_type)) {
                Some(e) => e,
                None => {
                    // Upstream: no flow executor, end the branch.
                    br.dead = true;
                    return;
                }
            };
            let r = exec(self, &b, br);
            self.apply_result(br, &b, r, spawned);
            if br.dead {
                return;
            }
            if self.paused || br.wait_ticks > 0 {
                br.parked = true;
                return;
            }
            if Self::yields_after(BlockType::from_index(b.block_type)) {
                return;
            }
        }
    }

    fn yields_after(ty: BlockType) -> bool {
        match ty {
            BlockType::Break => false,
            BlockType::GotoPos | BlockType::GotoEntity | BlockType::FuncCall => true,
            _ => def_for(ty).map(|d| d.category == BlockCategory::Control).unwrap_or(false),
        }
    }

    fn apply_result(&mut self, br: &mut Branch, current: &BlockInstance, r: StepResult, spawned: &mut Vec<Branch>) {
        match r.kind {
            StepKind::End => br.dead = true,
            StepKind::Wait => br.wait_ticks = r.wait_ticks,
            StepKind::Handled => {}
            StepKind::Jump => {
                self.apply_frames(br, current, &r);
                br.current_block_id = r.jump_to;
                if r.wait_ticks > 0 {
                    br.wait_ticks = r.wait_ticks;
                }
            }
            StepKind::Continue => {
                self.apply_frames(br, current, &r);
                let from = current.id;
                let port = r.port;
                self.navigate(br, from, port, spawned);
                if r.wait_ticks > 0 {
                    br.wait_ticks = r.wait_ticks;
                }
            }
        }
    }

    fn apply_frames(&self, br: &mut Branch, current: &BlockInstance, r: &StepResult) {
        if r.pop_frame && !br.frames.is_empty() {
            br.frames.pop();
        }
        if r.push_frame {
            let mut f = ControlFrame::new(current.id, r.activation_id);
            if let Some(data) = &r.frame_data {
                f.data.extend(data.clone());
            }
            br.frames.push(f);
        }
    }

    /// Upstream navigate: first wire continues the branch, further
    /// wires fork.
    pub fn navigate(&mut self, br: &mut Branch, from_block_id: i32, out_port: i32, spawned: &mut Vec<Branch>) {
        let wires = self.outgoing(from_block_id, out_port);
        if wires.is_empty() {
            if !self.return_from_function(br) {
                br.dead = true;
            }
            return;
        }
        br.current_block_id = wires[0].to_block_id;
        for w in &wires[1..] {
            spawned.push(br.fork_at(w.to_block_id));
        }
    }

    /// Upstream returnFromFunction: call frames carry the FuncCall id.
    pub fn return_from_function(&mut self, br: &mut Branch) -> bool {
        let top = match br.frames.last() {
            Some(f) => f.clone(),
            None => return false,
        };
        // isCallFrame: container is a FuncCall block.
        if BlockType::from_index(
            self.script.block(top.container_block_id).map(|b| b.block_type).unwrap_or(-1),
        ) != BlockType::FuncCall
        {
            return false;
        }
        br.frames.pop();
        let call_id = top.container_block_id;
        if self.script.block(call_id).is_none() {
            return false;
        }
        self.navigate(br, call_id, 0, &mut Vec::new());
        true
    }
}
}
