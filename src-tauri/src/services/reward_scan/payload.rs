//! What the reward overlay renders. `RewardOverlayPayload` mirrors
//! `config/shared/rewardPresentation.ts`; `RewardOverlayView` is the `reward-overlay` event.
use serde::Serialize;

/// `reward-overlay` event name; carries a [`RewardOverlayView`].
pub const EVENT: &str = "reward-overlay";

/// One part of the reward's set, with how many the player owns.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RewardOverlayPart {
    pub name: String,
    pub image_url: Option<String>,
    pub owned_count: u32,
    pub required_count: u32,
    /// The part this reward is (bow grips and the like are hard to spot).
    pub is_reward: bool,
    /// Its blueprint is already in the foundry.
    pub building: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RewardOverlayItem {
    /// English name (the join key), whatever language the card is printed in.
    pub name: String,
    pub rarity: String,
    /// 0 when unknown.
    pub ducats: u32,
    pub part_owned_count: u32,
    pub part_required_count: u32,
    /// `None`: not a masterable item (Forma, Kuva...), so no chip.
    pub mastered: Option<bool>,
    /// `None` for Forma.
    pub vaulted: Option<bool>,
    pub building: bool,
    pub set_owned_count: u32,
    pub set_required_count: u32,
    /// WFM slug of the set, when WFM lists one.
    pub set_url_name: Option<String>,
    pub set_parts: Vec<RewardOverlayPart>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RewardOverlaySlot {
    pub item: RewardOverlayItem,
    /// WFM platinum price of the part; `None` while unknown.
    pub price: Option<u32>,
    pub set_price: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RewardOverlayPayload {
    /// Cards on the game's screen (1-4); exactly that many `slots` are `Some`.
    pub count: u8,
    /// Always four entries, left to right as on the game's screen.
    pub slots: Vec<Option<RewardOverlaySlot>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RewardFailure {
    /// No card read, or the reads did not match the counted cards.
    NothingRead,
    /// The trigger fired but the screen is not a reward screen.
    NotRewardScreen,
    GameNotOpen,
    CaptureUnavailable,
    /// A hotkey scan read nothing after the last reward screen had already closed.
    ScreenClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RewardOverlayPhase {
    /// A new round started: clear the previous cards.
    Scanning,
    Ready,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RewardOverlayView {
    pub phase: RewardOverlayPhase,
    /// Set when `phase` is `ready`.
    pub presentation: Option<RewardOverlayPayload>,
    /// Set when `phase` is `failed`.
    pub failure: Option<RewardFailure>,
    /// The backend closes the window this long after the event; 0 while scanning.
    pub auto_close_ms: u32,
    /// Identifies the round; a close quoting an older one is ignored.
    #[specta(type = f64)]
    pub token: u64,
}
