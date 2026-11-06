//! Order books a `MarketLoad` job fetched, kept in memory so a modal opened (or reopened) later can
//! read them. Raw orders: the frontend filters them by rank and subtype as it does for any book.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::services::wfm::orders::BookOrder;

/// Prices move; a book older than this is fetched again.
const TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarketBook {
    pub slug: String,
    pub orders: Vec<BookOrder>,
}

#[derive(Default)]
pub struct MarketBooks(Mutex<HashMap<String, (Instant, Vec<BookOrder>)>>);

impl MarketBooks {
    pub fn put(&self, slug: &str, orders: Vec<BookOrder>) {
        let mut m = self.0.lock().unwrap();
        m.retain(|_, (at, _)| at.elapsed() < TTL);
        m.insert(slug.to_owned(), (Instant::now(), orders));
    }

    /// The fresh books among `slugs`.
    pub fn get(&self, slugs: &[String]) -> Vec<MarketBook> {
        let m = self.0.lock().unwrap();
        slugs
            .iter()
            .filter_map(|s| {
                let (at, orders) = m.get(s)?;
                (at.elapsed() < TTL).then(|| MarketBook {
                    slug: s.clone(),
                    orders: orders.clone(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_only_what_was_put_and_not_expired() {
        let b = MarketBooks::default();
        b.put("serration", vec![]);
        let got = b.get(&["serration".into(), "other".into()]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].slug, "serration");
        b.0.lock().unwrap().get_mut("serration").unwrap().0 = Instant::now() - TTL;
        assert!(b.get(&["serration".into()]).is_empty());
    }
}
