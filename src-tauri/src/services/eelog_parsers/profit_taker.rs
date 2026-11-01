//! Profit-Taker (Orb Fight, phase 4 bounty) run parser.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::line_ts;
use super::sanitize::strip_platform_glyphs;

/// Only BountyFour is the orb fight; One/Two/Three are the earlier heist phases.
const JOB_BOUNTY_FOUR: &str = "/Lotus/Types/Gameplay/Venus/Jobs/Heists/HeistProfitTakerBountyFour";
const ACTIVE_JOB: &str = "ThemedSquadOverlay.lua: Active jobId set to ";
const PT_MISSION_NAME: &str =
    "ThemedSquadOverlay.lua: Mission name: Fortuna (Venus) - PROFIT-TAKER - PHASE 4";
const SESSION_MAP: &str = "EidolonMP.lua: Session map string: ";
const ELEVATOR_EXIT: &str = "EidolonMP.lua: EIDOLONMP: Avatar left the zone";
const BACK_TO_TOWN: &str = "EidolonMP.lua: EIDOLONMP: TryTownTransition";
const JOB_START: &str = "EidolonMissions.lua - Starting job PROFIT-TAKER";
const ABORT_MISSION: &str = "GameRulesImpl - changing state from SS_STARTED to SS_ENDING";
const EOM_COMMIT: &str = "Sys [Info]: EOM missionLocationUnlocked=";
const HOST_MIGRATION_JOB: &str =
    "\"jobId\" : \"/Lotus/Types/Gameplay/Venus/Jobs/Heists/HeistProfitTakerBountyFour";

const ORB_FIGHT: &str = "CamperHeistOrbFight.lua: ";
const PHASE_STARTS: [(&str, u8); 4] = [
    ("Orb Fight - Starting first attack Orb phase", 1),
    ("Orb Fight - Starting second attack Orb phase", 2),
    ("Orb Fight - Starting third attack Orb phase", 3),
    ("Orb Fight - Starting final attack Orb phase", 4),
];
const STATE_CHANGE: &str = "Landscape - New State: ";
const BODY_KILL_STATES: [i64; 3] = [3, 5, 6];

const CAMPER: &str = "Camper->";
const SHIELD_SWITCH: &str =
    "SwitchShieldVulnerability() - Switching shield damage vulnerability type to ";
const LEG_KILL: &str = "Leg freshly destroyed at part: ";
const BODY_VULNERABLE: &str = "StartVulnerable() - The Camper can now be damaged!";
const PYLONS_LAUNCHED: &str = "ArachnoidCamperScript.lua: Pylon launch complete";

/// Shield phases close on a transmission load; phase 2 has no shield phase.
const RESOURCE_LOADER: &str = "Sys [Info]: ResourceLoader";
const SHIELD_END_IDS: [&str; 3] = [
    "DBntyFourInterPrTk0920TheBusiness",
    "DBntyFourInterPrTk0890TheBusiness",
    "DBntyFourSatelReal0930TheBusiness",
];

static LOADOUT_LOADED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Game \[Info\]: (.+?) loadout loader finished").unwrap());

fn element_name(token: &str) -> String {
    match token {
        "DT_IMPACT" => "impact",
        "DT_PUNCTURE" => "puncture",
        "DT_SLASH" => "slash",
        "DT_FREEZE" => "cold",
        "DT_FIRE" => "heat",
        "DT_POISON" => "toxin",
        "DT_ELECTRICITY" => "electric",
        "DT_GAS" => "gas",
        "DT_VIRAL" => "viral",
        "DT_MAGNETIC" => "magnetic",
        "DT_RADIATION" => "radiation",
        "DT_CORROSIVE" => "corrosive",
        "DT_EXPLOSION" => "blast",
        other => return other.to_lowercase(),
    }
    .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PtLeg {
    FrontLeft,
    FrontRight,
    BackLeft,
    BackRight,
}

/// The log names legs from the orb's view; players call them the other way round.
fn leg_from_part(part: &str) -> Option<PtLeg> {
    match part {
        "ARM_RIGHT" => Some(PtLeg::FrontLeft),
        "ARM_LEFT" => Some(PtLeg::FrontRight),
        "LEG_RIGHT" => Some(PtLeg::BackLeft),
        "LEG_LEFT" => Some(PtLeg::BackRight),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PtRunEndReason {
    Completed,
    MissionEnd,
    Aborted,
    NewRun,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PtParserEvent {
    RunStart { game_time_sec: f64 },
    RunEnd { reason: PtRunEndReason },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PtShieldChange {
    pub element: String,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PtLegBreak {
    pub leg: PtLeg,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PtPhase {
    pub index: u8,
    pub total_sec: f64,
    pub shield_sec: f64,
    pub leg_sec: f64,
    pub body_sec: f64,
    pub pylon_sec: f64,
    pub shields: Vec<PtShieldChange>,
    pub legs: Vec<PtLegBreak>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtParsedRun {
    /// A `Session map string` line proved this is a real Profit-Taker mission.
    pub confirmed: bool,
    /// Flight start (last elevator exit, or the job-start line as fallback).
    pub run_start_sec: f64,
    pub run_end_sec: Option<f64>,
    pub last_activity_sec: f64,
    pub duration_sec: f64,
    pub flight_sec: f64,
    pub flight_unreliable: bool,
    pub shield_sec: f64,
    pub leg_sec: f64,
    pub body_sec: f64,
    pub pylon_sec: f64,
    pub phases: Vec<PtPhase>,
    pub players: Vec<String>,
    pub complete: bool,
    pub bugged: bool,
    pub aborted: bool,
    pub host_migration: bool,
    /// False for client-side runs: only the host logs the `Camper->` AI lines.
    pub host_telemetry: bool,
}

struct PhaseState {
    index: u8,
    start_sec: f64,
    shields: Vec<PtShieldChange>,
    legs: Vec<PtLegBreak>,
    body_vulnerable_sec: Option<f64>,
    body_kill_sec: Option<f64>,
    pylon_launch_sec: Option<f64>,
}

impl PhaseState {
    fn new(index: u8, start_sec: f64) -> Self {
        Self {
            index,
            start_sec,
            shields: Vec::new(),
            legs: Vec::new(),
            body_vulnerable_sec: None,
            body_kill_sec: None,
            pylon_launch_sec: None,
        }
    }
}

struct RunState {
    confirmed: bool,
    captured_at_sec: f64,
    flight_start_sec: Option<f64>,
    job_start_sec: Option<f64>,
    phase1_start_sec: Option<f64>,
    last_activity_sec: f64,
    event_count: u32,
    players: Vec<String>,
    phases: Vec<PtPhase>,
    current: PhaseState,
    previous_element: Option<String>,
    previous_time: f64,
    shield_phase_ended: bool,
    kill_sequence: u32,
    /// Set when pylons launch in phase 3; a second shield line then means phase 4.
    pylon_check: bool,
    shield_count: u32,
    run_end_sec: Option<f64>,
    /// Latched so a caller that keeps feeding after the end event cannot double-submit.
    ended: bool,
    complete: bool,
    bugged: bool,
    aborted: bool,
    host_migration: bool,
    host_telemetry: bool,
}

/// `parseInt` on the text after a marker: leading whitespace, optional sign, digits.
fn parse_int_prefix(s: &str) -> Option<i64> {
    let s = s.trim_start();
    let end = s
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
        .map_or(s.len(), |(i, _)| i);
    s[..end].parse().ok()
}

fn after<'a>(line: &'a str, marker: &str) -> &'a str {
    line.find(marker).map_or("", |i| &line[i + marker.len()..])
}

impl RunState {
    fn end(&mut self, ts: f64, reason: PtRunEndReason) -> PtParserEvent {
        if ts > 0.0 {
            self.run_end_sec = Some(ts);
        }
        self.ended = true;
        PtParserEvent::RunEnd { reason }
    }

    /// Close the running phase and open the next one at `end_sec`.
    fn submit_phase(&mut self, end_sec: f64) {
        let p = &self.current;
        let body_sec = match (p.body_vulnerable_sec, p.body_kill_sec) {
            (Some(v), Some(k)) => (k - v).max(0.0),
            _ => 0.0,
        };
        // A bugged phase 3 ends on a guessed marker, so its pylon window is garbage.
        let counts_pylons = p.index == 1 || (p.index == 3 && !self.bugged);
        let pylon_sec = match p.pylon_launch_sec {
            Some(l) if counts_pylons => (end_sec - l).max(0.0),
            _ => 0.0,
        };
        let next = (p.index + 1).min(4);
        let done = std::mem::replace(&mut self.current, PhaseState::new(next, end_sec));
        self.phases.push(PtPhase {
            index: done.index,
            total_sec: (end_sec - done.start_sec).max(0.0),
            shield_sec: done.shields.iter().map(|s| s.seconds).sum(),
            leg_sec: done.legs.iter().map(|l| l.seconds).sum(),
            body_sec,
            pylon_sec,
            shields: done.shields,
            legs: done.legs,
        });
        self.previous_time = end_sec;
        self.kill_sequence = 0;
    }

    fn push_shield_change(&mut self, ts: f64, element: String) {
        self.current.shields.push(PtShieldChange {
            element,
            seconds: (ts - self.previous_time).max(0.0),
        });
        self.previous_time = ts;
    }

    fn handle_shield_line(&mut self, line: &str, ts: f64, is_switch: bool) {
        // Bugged log: phase 4 never announced itself, so the second shield line after
        // the phase-3 pylons stands in for it.
        if self.current.index == 3 && self.pylon_check && self.shield_count > 0 {
            self.bugged = true;
            self.previous_time = ts;
            self.shield_phase_ended = false;
            self.submit_phase(ts);
        } else {
            self.shield_count += 1;
        }

        if is_switch {
            let element = element_name(after(line, SHIELD_SWITCH).trim());
            if !self.shield_phase_ended {
                let Some(prev) = self.previous_element.replace(element) else {
                    // Phase 3 measures its first shield from the phase start instead.
                    if self.current.index != 3 {
                        self.previous_time = ts;
                    }
                    return;
                };
                if self.phases.is_empty() && self.current.shields.is_empty() {
                    self.previous_time = self
                        .phase1_start_sec
                        .or(self.flight_start_sec)
                        .unwrap_or(0.0);
                }
                self.push_shield_change(ts, prev);
                return;
            }
            // Shields keep switching during the phase-3 pylons; only the element carries over.
            if self.current.index == 3 {
                self.previous_element = Some(element);
            }
            return;
        }

        if self.current.shields.is_empty()
            || self.shield_phase_ended
            || self.previous_element.is_none()
        {
            return;
        }
        let prev = self.previous_element.take().unwrap();
        self.push_shield_change(ts, prev);
        self.shield_phase_ended = true;
    }

    fn handle_camper_line(&mut self, line: &str, ts: f64) {
        self.host_telemetry = true;
        if line.contains(SHIELD_SWITCH) {
            self.event_count += 1;
            self.handle_shield_line(line, ts, true);
        } else if line.contains(LEG_KILL) {
            self.event_count += 1;
            if let Some(leg) = leg_from_part(after(line, LEG_KILL).trim()) {
                self.current.legs.push(PtLegBreak {
                    leg,
                    seconds: (ts - self.previous_time).max(0.0),
                });
                if self.current.legs.len() > 4 {
                    self.bugged = true;
                }
            }
            self.previous_time = ts;
            self.shield_count = 0;
        } else if line.contains(BODY_VULNERABLE) {
            self.event_count += 1;
            if self.kill_sequence == 0 {
                self.current.body_vulnerable_sec = Some(ts);
            }
            self.kill_sequence += 1;
        }
    }

    fn handle_orb_fight_line(&mut self, line: &str, ts: f64) {
        if line.contains(STATE_CHANGE) {
            if parse_int_prefix(after(line, STATE_CHANGE))
                .is_some_and(|s| BODY_KILL_STATES.contains(&s))
            {
                self.current.body_kill_sec = Some(ts);
            }
            return;
        }
        for (marker, index) in PHASE_STARTS {
            if !line.contains(marker) {
                continue;
            }
            self.event_count += 1;
            if index == 1 {
                self.phase1_start_sec = Some(ts);
                self.current.start_sec = ts;
                return;
            }
            if index == 3 {
                self.shield_phase_ended = false;
            }
            if index == 4 {
                self.pylon_check = false;
                self.shield_phase_ended = false;
            }
            self.submit_phase(ts);
            return;
        }
    }
}

#[derive(Default)]
pub struct ProfitTakerParser {
    run: Option<RunState>,
}

impl ProfitTakerParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_run_active(&self) -> bool {
        self.run.is_some()
    }

    pub fn last_activity_sec(&self) -> f64 {
        self.run.as_ref().map_or(0.0, |r| r.last_activity_sec)
    }

    /// Monotonic fight-event count; hub chatter cannot keep a capture alive.
    pub fn activity_count(&self) -> u32 {
        self.run.as_ref().map_or(0, |r| r.event_count)
    }

    pub fn reset(&mut self) {
        self.run = None;
    }

    fn start_run(&mut self, game_time_sec: f64, confirmed: bool) -> PtParserEvent {
        self.run = Some(RunState {
            confirmed,
            captured_at_sec: game_time_sec,
            flight_start_sec: None,
            job_start_sec: None,
            phase1_start_sec: None,
            last_activity_sec: game_time_sec,
            event_count: 0,
            players: Vec::new(),
            phases: Vec::new(),
            // Shield and leg events before the phase-1 line belong to phase 1, exactly
            // as the reference analyser accumulates them into its phase-0 placeholder.
            current: PhaseState::new(1, game_time_sec),
            previous_element: None,
            previous_time: 0.0,
            shield_phase_ended: false,
            kill_sequence: 0,
            pylon_check: false,
            shield_count: 0,
            run_end_sec: None,
            ended: false,
            complete: false,
            bugged: false,
            aborted: false,
            host_migration: false,
            host_telemetry: false,
        });
        PtParserEvent::RunStart { game_time_sec }
    }

    pub fn feed_line(&mut self, line: &str) -> Option<PtParserEvent> {
        if line.is_empty() {
            return None;
        }
        let ts = line_ts(line);

        if self.run.as_ref().is_some_and(|r| r.ended) {
            return None;
        }

        // Loadouts can finish before Active jobId; the mission-name line starts their capture.
        if self.run.is_none() && line.contains(PT_MISSION_NAME) {
            return Some(self.start_run(ts, false));
        }

        if line.contains(ACTIVE_JOB) {
            let is_four = line.contains(JOB_BOUNTY_FOUR);
            let Some(run) = self.run.as_mut() else {
                return is_four.then(|| self.start_run(ts, false));
            };
            // Picking another bounty while only armed means the capture was a false start.
            if !run.confirmed && !is_four {
                return Some(run.end(ts, PtRunEndReason::Aborted));
            }
            return None;
        }

        if line.contains(SESSION_MAP) {
            let is_four = line.contains(JOB_BOUNTY_FOUR);
            let Some(run) = self.run.as_mut() else {
                return is_four.then(|| self.start_run(ts, true));
            };
            if !run.confirmed {
                if is_four {
                    run.confirmed = true;
                    run.last_activity_sec = ts;
                    return None;
                }
                return Some(run.end(ts, PtRunEndReason::Aborted));
            }
            let reason = if is_four {
                PtRunEndReason::NewRun
            } else {
                PtRunEndReason::MissionEnd
            };
            return Some(run.end(ts, reason));
        }

        let r = self.run.as_mut()?;
        if ts > 0.0 {
            r.last_activity_sec = r.last_activity_sec.max(ts);
        }

        if line.contains(CAMPER) {
            r.handle_camper_line(line, ts);
        } else if line.contains(ORB_FIGHT) {
            r.handle_orb_fight_line(line, ts);
        } else if line.contains(PYLONS_LAUNCHED) {
            r.event_count += 1;
            r.current.pylon_launch_sec = Some(ts);
            if r.current.index == 3 {
                r.pylon_check = true;
                // Only shield lines after the launch feed the bugged-phase-4 guess.
                r.shield_count = 0;
            }
        } else if line.contains(RESOURCE_LOADER) {
            if SHIELD_END_IDS.iter().any(|id| line.contains(id)) {
                r.handle_shield_line(line, ts, false);
            }
        } else if line.contains(ELEVATOR_EXIT) {
            if r.phase1_start_sec.is_none() {
                r.flight_start_sec = Some(ts);
            }
        } else if line.contains(JOB_START) {
            r.job_start_sec = Some(ts);
        } else if line.contains("loadout loader finished") {
            // Warframe appends a platform glyph to the name in this line.
            let player = LOADOUT_LOADED
                .captures(line)
                .map(|c| strip_platform_glyphs(&c[1]))
                .unwrap_or_default();
            if !player.is_empty() && !r.players.contains(&player) {
                r.players.push(player);
            }
        } else if line.contains(HOST_MIGRATION_JOB) {
            r.host_migration = true;
        }

        if line.contains(ABORT_MISSION) || line.contains(BACK_TO_TOWN) {
            r.aborted = true;
            return Some(r.end(ts, PtRunEndReason::Aborted));
        }
        // Three vulnerability windows in one phase means the orb died.
        if r.kill_sequence == 3 {
            r.current.body_kill_sec = Some(ts);
            r.complete = true;
            r.submit_phase(ts);
            return Some(r.end(ts, PtRunEndReason::Completed));
        }
        if line.contains(EOM_COMMIT) {
            return Some(r.end(ts, PtRunEndReason::MissionEnd));
        }
        None
    }

    /// Closes the active run and returns its summary; None when idle.
    pub fn finalize(&mut self) -> Option<PtParsedRun> {
        let r = self.run.take()?;
        let flight_unreliable = r.flight_start_sec.is_none();
        let flight_start = r
            .flight_start_sec
            .or(r.job_start_sec)
            .unwrap_or(r.captured_at_sec);
        let flight_sec = r
            .phase1_start_sec
            .map_or(0.0, |p| (p - flight_start).max(0.0));
        let sum = |f: fn(&PtPhase) -> f64| r.phases.iter().map(f).sum::<f64>();

        Some(PtParsedRun {
            confirmed: r.confirmed,
            run_start_sec: flight_start,
            run_end_sec: r.run_end_sec,
            last_activity_sec: r.last_activity_sec,
            duration_sec: flight_sec + sum(|p| p.total_sec),
            flight_sec,
            flight_unreliable: flight_unreliable && r.phase1_start_sec.is_some(),
            shield_sec: sum(|p| p.shield_sec),
            leg_sec: sum(|p| p.leg_sec),
            body_sec: sum(|p| p.body_sec),
            pylon_sec: sum(|p| p.pylon_sec),
            phases: r.phases,
            players: r.players,
            complete: r.complete,
            bugged: r.bugged,
            aborted: r.aborted,
            host_migration: r.host_migration,
            host_telemetry: r.host_telemetry,
        })
    }
}

#[cfg(test)]
mod tests;
