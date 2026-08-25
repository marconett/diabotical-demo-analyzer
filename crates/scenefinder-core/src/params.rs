//! Search parameters and their validation.

use serde::{Deserialize, Serialize};

use crate::scenes::Criteria;
use crate::text::{fmt_g, py_repr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    #[serde(rename = "AND")]
    And,
    #[serde(rename = "OR")]
    Or,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Params {
    pub players: Vec<String>,
    pub damage: Option<u64>,
    pub frags: Option<u64>,
    pub condition: Condition,
    /// The scene must end with the player's round-winning frag.
    pub win: bool,
    /// Window length in seconds.
    pub window: Option<f64>,
}

impl Params {
    pub fn has_threshold(&self) -> bool {
        self.damage.is_some() || self.frags.is_some()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.players.is_empty() {
            return Err("at least one player name is required".into());
        }
        if !self.has_threshold() && !self.win {
            return Err(
                "at least one of -dmg/--damage_threshold, -frags or -win is required".into(),
            );
        }
        if self.window.is_none() && self.has_threshold() {
            return Err("-t/--time_window is required with -dmg/-frags".into());
        }
        if self.window.is_some_and(|w| w <= 0.0 || w.is_nan()) {
            return Err("--time_window must be > 0".into());
        }
        Ok(())
    }

    pub fn criteria(&self) -> Criteria {
        Criteria {
            dmg: self.damage,
            frags: self.frags,
            and: self.condition == Condition::And,
        }
    }

    /// The `# scene-finder ...` line that opens every report.
    pub fn header_line(&self) -> String {
        let mut crit = Vec::new();
        if let Some(d) = self.damage {
            crit.push(format!("dmg >= {d}"));
        }
        if let Some(f) = self.frags {
            crit.push(format!("frags >= {f}"));
        }
        let joiner = match self.condition {
            Condition::And => " AND ",
            Condition::Or => " OR ",
        };
        let mut desc = crit.join(joiner);
        if self.win {
            let prefix = if crit.len() > 1 {
                format!("({desc}) AND ")
            } else if !crit.is_empty() {
                format!("{desc} AND ")
            } else {
                String::new()
            };
            desc = format!("{prefix}round-winning frag");
        }
        if let Some(w) = self.window {
            desc.push_str(&format!(" in {}s", fmt_g(w)));
        }
        let who: Vec<String> = self.players.iter().map(|p| py_repr(p)).collect();
        let plural = if self.players.len() > 1 { "s" } else { "" };
        format!("# scene-finder  player{plural}={}  {desc}", who.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(
        players: &[&str],
        damage: Option<u64>,
        frags: Option<u64>,
        or: bool,
        win: bool,
        window: Option<f64>,
    ) -> Params {
        Params {
            players: players.iter().map(|s| s.to_string()).collect(),
            damage,
            frags,
            condition: if or { Condition::Or } else { Condition::And },
            win,
            window,
        }
    }

    #[test]
    fn validation_messages() {
        assert_eq!(
            p(&[], Some(1), None, false, false, Some(1.0))
                .validate()
                .unwrap_err(),
            "at least one player name is required"
        );
        assert_eq!(
            p(&["a"], None, None, false, false, None)
                .validate()
                .unwrap_err(),
            "at least one of -dmg/--damage_threshold, -frags or -win is required"
        );
        assert_eq!(
            p(&["a"], Some(1), None, false, false, None)
                .validate()
                .unwrap_err(),
            "-t/--time_window is required with -dmg/-frags"
        );
        assert_eq!(
            p(&["a"], Some(1), None, false, false, Some(0.0))
                .validate()
                .unwrap_err(),
            "--time_window must be > 0"
        );
        assert!(p(&["a"], None, None, false, true, None).validate().is_ok());
        assert!(p(&["a"], Some(1), Some(2), true, true, Some(5.0))
            .validate()
            .is_ok());
    }

    #[test]
    fn header_lines() {
        assert_eq!(
            p(
                &["27 shaft avg n1"],
                Some(200),
                Some(2),
                true,
                false,
                Some(5.0)
            )
            .header_line(),
            "# scene-finder  player='27 shaft avg n1'  dmg >= 200 OR frags >= 2 in 5s"
        );
        assert_eq!(
            p(
                &["27 shaft avg n1", "t0urizt"],
                None,
                None,
                false,
                true,
                None
            )
            .header_line(),
            "# scene-finder  players='27 shaft avg n1', 't0urizt'  round-winning frag"
        );
        assert_eq!(
            p(&["x"], None, Some(3), false, true, Some(10.0)).header_line(),
            "# scene-finder  player='x'  frags >= 3 AND round-winning frag in 10s"
        );
        assert_eq!(
            p(&["x"], Some(500), Some(2), false, true, Some(10.0)).header_line(),
            "# scene-finder  player='x'  (dmg >= 500 AND frags >= 2) AND round-winning frag in 10s"
        );
        assert_eq!(
            p(&["x"], Some(1), None, false, false, Some(0.25)).header_line(),
            "# scene-finder  player='x'  dmg >= 1 in 0.25s"
        );
    }
}
