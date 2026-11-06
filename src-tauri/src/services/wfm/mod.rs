//! warframe.market client core: HTTP client, error classification and sign-in session.

pub mod catalog;
pub mod client;
pub mod contracts;
pub mod errors;
pub mod legacy_v1;
pub mod live;
pub mod my_orders;
pub mod orders;
pub mod presence;
pub mod prices;
pub mod profile_slug;
pub mod reviews;
pub mod riven_items;
pub mod riven_search;
pub mod scheduler;
pub mod session;
pub mod stats_history;
pub mod stats_v1;
pub mod types;
pub mod validators;
pub mod webview_login;
pub mod ws;

#[cfg(test)]
pub(crate) mod mock;
