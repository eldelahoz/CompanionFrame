//! Steel Path Honors rotation, computed locally from a fixed epoch (no API needed).

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};

use super::{SteelPath, SteelPathReward, SteelPathUpcoming};

const EPOCH_MS: i64 = 1_605_484_800_000; // 2020-11-16T00:00:00Z
const EIGHT_WEEKS_S: i64 = 4_838_400;
const SEVEN_DAYS_S: i64 = 604_800;

const ROTATION: [(&str, u32); 8] = [
    ("Umbra Forma Blueprint", 150),
    ("50,000 Kuva", 55),
    ("Kitgun Riven Mod", 75),
    ("3x Forma", 75),
    ("Zaw Riven Mod", 75),
    ("30,000 Endo", 150),
    ("Rifle Riven Mod", 75),
    ("Shotgun Riven Mod", 75),
];

const EVERGREENS: [(&str, u32); 10] = [
    ("Veiled Riven Cipher", 20),
    ("Bishamo Pauldrons Blueprint", 15),
    ("Bishamo Cuirass Blueprint", 25),
    ("Bishamo Helmet Blueprint", 20),
    ("Bishamo Greaves Blueprint", 25),
    ("10k Kuva", 15),
    ("Primary Arcane Adapter", 15),
    ("Secondary Arcane Adapter", 15),
    ("Relic Pack", 15),
    ("Stance Forma Blueprint", 10),
];

fn reward((name, cost): (&str, u32)) -> SteelPathReward {
    SteelPathReward {
        name: name.into(),
        cost,
    }
}

fn iso(d: DateTime<Utc>) -> String {
    d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// The week ends Sunday 23:59:59 UTC, six days after its Monday 00:00 start.
fn week_end(start: DateTime<Utc>) -> DateTime<Utc> {
    start + Duration::days(6) + Duration::hours(23) + Duration::minutes(59) + Duration::seconds(59)
}

pub fn compute(now_ms: i64) -> SteelPath {
    let since_start_s = (now_ms - EPOCH_MS).div_euclid(1000);
    let ind = ((since_start_s % EIGHT_WEEKS_S) / SEVEN_DAYS_S) as usize;

    let now = Utc.timestamp_millis_opt(now_ms).unwrap();
    let midnight = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let activation = midnight - Duration::days(now.weekday().num_days_from_monday() as i64);

    let upcoming = [1usize, 2]
        .map(|week_offset| {
            let start = activation + Duration::days(7 * week_offset as i64);
            SteelPathUpcoming {
                name: ROTATION[(ind + week_offset) % ROTATION.len()].0.into(),
                cost: ROTATION[(ind + week_offset) % ROTATION.len()].1,
                activation: iso(start),
                expiry: iso(week_end(start)),
                week_offset: week_offset as u32,
            }
        })
        .to_vec();

    SteelPath {
        current_reward: reward(ROTATION[ind]),
        activation: Some(iso(activation)),
        expiry: Some(iso(week_end(activation))),
        rotation: ROTATION.map(reward).to_vec(),
        upcoming,
        evergreens: EVERGREENS.map(reward).to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_runs_monday_to_sunday_and_rotation_advances() {
        // 2026-10-25 is a Sunday: its week started Monday 2026-10-19.
        let now = Utc
            .with_ymd_and_hms(2026, 10, 25, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let sp = compute(now);
        assert_eq!(sp.activation.as_deref(), Some("2026-10-19T00:00:00.000Z"));
        assert_eq!(sp.expiry.as_deref(), Some("2026-10-25T23:59:59.000Z"));
        assert_eq!(sp.upcoming[0].activation, "2026-10-26T00:00:00.000Z");
        let cur = ROTATION.iter().position(|r| r.0 == sp.current_reward.name);
        assert_eq!(sp.upcoming[0].name, ROTATION[(cur.unwrap() + 1) % 8].0);
        assert_eq!(sp.evergreens.len(), 10);
    }
}
