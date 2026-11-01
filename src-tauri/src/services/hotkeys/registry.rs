//! Which accelerators are bound: the overlay's own (from settings).

use super::accelerator::{Accel, parse_valid};

#[derive(Default)]
pub struct Registry {
    owned: Vec<(String, Accel)>,
}

impl Registry {
    /// Replaces all owned bindings or changes nothing. An empty accelerator disables that binding.
    pub fn set_owned(&mut self, wanted: &[(&str, &str)]) -> Result<(), String> {
        let mut next: Vec<(String, Accel)> = Vec::new();
        for (id, s) in wanted {
            if s.trim().is_empty() {
                continue;
            }
            let a = parse_valid(s).map_err(|e| format!("{id}: {e}"))?;
            if let Some((other, _)) = next.iter().find(|(_, x)| *x == a) {
                return Err(format!("{id} and {other} use the same hotkey ({a})"));
            }
            next.push((id.to_string(), a));
        }
        self.owned = next;
        Ok(())
    }

    pub fn watch(&self) -> Vec<(String, Accel)> {
        self.owned.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNED: [(&str, &str); 3] = [
        ("reward_scan", "F8"),
        ("overlay_move", "F7"),
        ("riven_rescan", "Ctrl+F6"),
    ];

    fn reg() -> Registry {
        let mut r = Registry::default();
        r.set_owned(&OWNED).unwrap();
        r
    }

    #[test]
    fn owned_bindings_are_watched() {
        let ids: Vec<_> = reg().watch().into_iter().map(|(i, _)| i).collect();
        assert_eq!(ids, ["reward_scan", "overlay_move", "riven_rescan"]);
    }

    #[test]
    fn empty_accelerator_disables_a_binding() {
        let mut r = Registry::default();
        r.set_owned(&[("reward_scan", "F8"), ("riven_rescan", " ")])
            .unwrap();
        assert_eq!(r.watch().len(), 1);
    }

    #[test]
    fn duplicate_owned_is_rejected_and_keeps_previous() {
        let mut r = reg();
        let err = r
            .set_owned(&[("reward_scan", "F8"), ("riven_rescan", "f8")])
            .unwrap_err();
        assert!(err.contains("same hotkey"), "{err}");
        assert_eq!(r.watch().len(), 3);
    }

    #[test]
    fn duplicates_are_compared_by_key_not_text() {
        let mut r = Registry::default();
        assert!(
            r.set_owned(&[("a", "Ctrl+Shift+X"), ("b", "shift+control+x")])
                .is_err()
        );
    }

    #[test]
    fn invalid_or_reserved_owned_is_rejected() {
        let mut r = reg();
        for bad in ["Alt+F4", "A", "Ctrl+;", "Ctrl+Shift"] {
            assert!(r.set_owned(&[("overlay", bad)]).is_err(), "{bad}");
        }
        assert_eq!(r.watch().len(), 3);
    }
}
