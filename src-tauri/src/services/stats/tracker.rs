//! Daily deltas per resource, read off consecutive inventory payloads. Pure: the caller supplies
//! the day key, so rollover and restarts are testable without a clock.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where a resource's amount lives in the raw DE inventory payload.
enum Source {
    Field(&'static str),
    Misc(&'static str),
}

macro_rules! misc {
    ($name:literal) => {
        Source::Misc(concat!("/Lotus/Types/Items/MiscItems/", $name))
    };
}

/// Recording covers this whole list; only display is a user choice. Traps that must not be
/// "fixed": PrimeBucks is Ducats, top-level PrimeTokens is Regal Aya, Aya is SchismKey, Vitus
/// Essence is Elitium.
const RESOURCES: &[(&str, Source)] = &[
    ("plat", Source::Field("PremiumCredits")),
    ("ducats", misc!("PrimeBucks")),
    ("aya", misc!("SchismKey")),
    ("credits", Source::Field("RegularCredits")),
    ("endo", Source::Field("FusionPoints")),
    ("vitus", misc!("Elitium")),
    ("kuva", misc!("Kuva")),
    ("regalAya", Source::Field("PrimeTokens")),
    ("voidTraces", misc!("VoidTearDrop")),
    ("steelEssence", misc!("SteelEssence")),
    ("rivenSliver", misc!("RivenFragment")),
    ("vosfor", misc!("DistillPoints")),
    ("nitain", misc!("Alertium")),
    ("forma", misc!("Forma")),
    ("argonCrystal", misc!("ArgonCrystal")),
    ("orokinCell", misc!("OrokinCell")),
    ("tellurium", misc!("Tellurium")),
    ("somaticFibers", misc!("MemoryCryptoFragment")),
    ("hexenon", misc!("ConcentratedGas")),
    ("narmerIsoplast", misc!("NarmerBountyResource")),
    // Resources that live outside the MiscItems namespace.
    (
        "cetusWisp",
        Source::Misc("/Lotus/Types/Gameplay/Eidolon/Resources/CetusWispItem"),
    ),
    (
        "pathosClamp",
        Source::Misc("/Lotus/Types/Gameplay/Duviri/Resource/DuviriDragonDropItem"),
    ),
];

pub fn is_known(id: &str) -> bool {
    RESOURCES.iter().any(|(known, _)| *known == id)
}

pub fn resource_ids() -> impl Iterator<Item = &'static str> {
    RESOURCES.iter().map(|(id, _)| *id)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StatResourceDay {
    pub delta: f64,
    #[serde(default)]
    pub abs: Option<f64>,
}

/// One local day. Resource ids are the catalog's; ids this build does not record never get here.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatEntry {
    pub date: String,
    pub relics_opened: u32,
    pub days_played: u32,
    pub daily_trades: u32,
    pub resources: HashMap<String, StatResourceDay>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StatSessionResource {
    pub delta: f64,
    pub current: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub resources: HashMap<String, StatSessionResource>,
    pub relics_opened: u32,
    pub daily_trades: u32,
    pub has_data: bool,
}

#[derive(Default)]
pub struct Tracker {
    day: String,
    /// First reading of the day, per resource.
    baselines: HashMap<&'static str, f64>,
    /// Last known amount; a payload that cannot report a resource leaves it in place.
    currents: HashMap<&'static str, f64>,
    /// Deltas of the day before a restart, so fresh baselines cannot overwrite them.
    resumed: HashMap<&'static str, f64>,
    last_relic_total: Option<u32>,
    relics_opened: u32,
    trades: u32,
}

impl Tracker {
    /// Resumes today's counters from the stored entry after a restart.
    pub fn restore(&mut self, entry: &DailyStatEntry) {
        self.day.clone_from(&entry.date);
        self.relics_opened = entry.relics_opened;
        self.trades = entry.daily_trades;
        self.resumed.clear();
        for id in resource_ids() {
            if let Some(d) = entry.resources.get(id).filter(|d| d.delta != 0.0) {
                self.resumed.insert(id, d.delta);
            }
        }
    }

    fn roll(&mut self, today: &str) {
        if self.day == today {
            return;
        }
        self.day = today.to_owned();
        self.relics_opened = 0;
        self.trades = 0;
        // No spurious spike across midnight.
        self.last_relic_total = None;
        self.resumed.clear();
        self.baselines.clear();
    }

    pub fn on_inventory(&mut self, data: &Value, today: &str) -> DailyStatEntry {
        self.roll(today);
        let total = relic_total(data);
        if let Some(last) = self.last_relic_total.filter(|last| total < *last) {
            self.relics_opened += last - total;
        }
        self.last_relic_total = Some(total);

        let misc: Option<HashMap<&str, f64>> = data["MiscItems"].as_array().map(|rows| {
            let mut seen = HashMap::new();
            for row in rows {
                if let (Some(t), Some(n)) = (row["ItemType"].as_str(), row["ItemCount"].as_f64()) {
                    // First entry wins when a payload repeats an ItemType.
                    seen.entry(t).or_insert(n);
                }
            }
            seen
        });
        for (id, source) in RESOURCES {
            // DE drops a MiscItems row at zero, so an absent row in a payload that HAS the array
            // reads as 0; without the array nothing was reported.
            let value = match source {
                Source::Field(f) => data[*f].as_f64(),
                Source::Misc(name) => misc.as_ref().map(|m| m.get(name).copied().unwrap_or(0.0)),
            };
            if let Some(v) = value {
                self.currents.insert(id, v);
            }
        }
        // A baseline survives a payload that omits its resource: a partial read cannot rebaseline.
        for (id, v) in &self.currents {
            self.baselines.entry(id).or_insert(*v);
        }
        self.entry()
    }

    pub fn add_trade(&mut self, today: &str) -> DailyStatEntry {
        self.roll(today);
        self.trades += 1;
        self.entry()
    }

    fn delta(&self, id: &str) -> f64 {
        let resumed = self.resumed.get(id).copied().unwrap_or(0.0);
        match (self.currents.get(id), self.baselines.get(id)) {
            (Some(c), Some(b)) => resumed + (c - b),
            _ => resumed,
        }
    }

    fn entry(&self) -> DailyStatEntry {
        let resources = resource_ids()
            .filter_map(|id| {
                let (delta, abs) = (self.delta(id), self.currents.get(id).copied());
                (abs.is_some() || delta != 0.0)
                    .then(|| (id.to_string(), StatResourceDay { delta, abs }))
            })
            .collect();
        DailyStatEntry {
            date: self.day.clone(),
            relics_opened: self.relics_opened,
            days_played: 1,
            daily_trades: self.trades,
            resources,
        }
    }

    pub fn session(&self) -> SessionStats {
        SessionStats {
            resources: resource_ids()
                .map(|id| {
                    let r = StatSessionResource {
                        delta: self.delta(id),
                        current: self.currents.get(id).copied(),
                    };
                    (id.to_string(), r)
                })
                .collect(),
            relics_opened: self.relics_opened,
            daily_trades: self.trades,
            has_data: !self.currents.is_empty(),
        }
    }
}

/// Relics owned. The collections can list the same relic, so each is summed per ItemType and the
/// largest wins; only when none of them has a relic is every array scanned.
fn relic_total(data: &Value) -> u32 {
    fn is_relic(t: &str) -> bool {
        let t = t.to_ascii_lowercase();
        t.contains("voidprojection") || t.contains("/lotus/relics/")
    }
    fn add(counts: &mut HashMap<String, u32>, rows: &Value) {
        let mut per: HashMap<&str, u32> = HashMap::new();
        for row in rows.as_array().into_iter().flatten() {
            let Some(t) = row["ItemType"].as_str().filter(|t| is_relic(t)) else {
                continue;
            };
            let n = match &row["ItemCount"] {
                Value::Null => 1,
                v => v
                    .as_f64()
                    .filter(|n| *n > 0.0)
                    .map_or(0, |n| n.floor() as u32),
            };
            *per.entry(t).or_default() += n;
        }
        for (t, n) in per {
            let best = counts.entry(t.to_owned()).or_default();
            *best = (*best).max(n);
        }
    }
    let mut counts = HashMap::new();
    for key in ["LevelKeys", "MiscItems", "Recipes"] {
        add(&mut counts, &data[key]);
    }
    if counts.is_empty() {
        data.as_object()
            .into_iter()
            .flat_map(|o| o.values())
            .for_each(|rows| add(&mut counts, rows));
    }
    counts.values().sum()
}
