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

impl Condition {
    pub fn combine(self, values: impl IntoIterator<Item = bool>) -> bool {
        match self {
            Condition::And => values.into_iter().all(|value| value),
            Condition::Or => values.into_iter().any(|value| value),
        }
    }

    fn word(self) -> &'static str {
        match self {
            Condition::And => " AND ",
            Condition::Or => " OR ",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleQuery {
    pub condition: Condition,
    pub groups: Vec<RuleGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleGroup {
    pub id: String,
    pub condition: Condition,
    pub rules: Vec<SearchRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SearchRule {
    Damage {
        id: String,
        minimum: u64,
        weapon: Option<u32>,
    },
    Frags {
        id: String,
        minimum: u64,
        weapon: Option<u32>,
    },
    Speed {
        id: String,
        minimum: u64,
        duration: f64,
    },
    Accuracy {
        id: String,
        minimum: f64,
        weapon: u32,
    },
    Siphonator {
        id: String,
    },
    FlagCarrier {
        id: String,
    },
    RoundWin {
        id: String,
    },
    FalloutDeath {
        id: String,
        /// False requires a fallout; true rejects windows containing one.
        exclude: bool,
    },
}

impl SearchRule {
    pub fn id(&self) -> &str {
        match self {
            SearchRule::Damage { id, .. }
            | SearchRule::Frags { id, .. }
            | SearchRule::Speed { id, .. }
            | SearchRule::Accuracy { id, .. }
            | SearchRule::Siphonator { id }
            | SearchRule::FlagCarrier { id }
            | SearchRule::RoundWin { id }
            | SearchRule::FalloutDeath { id, .. } => id,
        }
    }

    pub fn description(&self) -> String {
        match self {
            SearchRule::Damage {
                minimum, weapon, ..
            } => format!(
                "{}damage >= {minimum}",
                weapon
                    .map(|id| format!("{} ", weapon_name(id)))
                    .unwrap_or_default()
            ),
            SearchRule::Frags {
                minimum, weapon, ..
            } => format!(
                "{}frags >= {minimum}",
                weapon
                    .map(|id| format!("{} ", weapon_name(id)))
                    .unwrap_or_default()
            ),
            SearchRule::Speed {
                minimum, duration, ..
            } if *duration > 0.0 => {
                format!("speed >= {minimum} for {}s", fmt_g(*duration))
            }
            SearchRule::Speed { minimum, .. } => format!("speed >= {minimum}"),
            SearchRule::Accuracy {
                minimum, weapon, ..
            } => format!(
                "{} reported accuracy >= {}%",
                weapon_name(*weapon),
                fmt_g(*minimum)
            ),
            SearchRule::Siphonator { .. } => "siphonator active".into(),
            SearchRule::FlagCarrier { .. } => "carrying flag".into(),
            SearchRule::RoundWin { .. } => "round-winning frag".into(),
            SearchRule::FalloutDeath { exclude: true, .. } => "no fallout death".into(),
            SearchRule::FalloutDeath { exclude: false, .. } => "fallout death".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Params {
    pub players: Vec<String>,
    pub damage: Option<u64>,
    pub frags: Option<u64>,
    /// Optional weapon id. It scopes damage to POV hit-confirmation events.
    #[serde(default)]
    pub weapon: Option<u32>,
    /// Minimum server-reported horizontal speed in game units/second.
    #[serde(default)]
    pub speed: Option<u64>,
    /// Required time at or above `speed` inside the scene window.
    #[serde(default)]
    pub speed_duration: Option<f64>,
    /// Minimum game-reported match-to-date weapon accuracy, as a percentage.
    #[serde(default)]
    pub accuracy: Option<f64>,
    pub condition: Condition,
    /// The scene must end with the player's round-winning frag.
    pub win: bool,
    /// Window length in seconds.
    pub window: Option<f64>,
    /// Flexible GUI query. Legacy fields above remain supported by the CLI.
    #[serde(default)]
    pub rule_query: Option<RuleQuery>,
}

impl Params {
    pub fn has_threshold(&self) -> bool {
        self.damage.is_some()
            || self.frags.is_some()
            || self.speed.is_some()
            || self.accuracy.is_some()
    }

    pub fn threshold_count(&self) -> usize {
        [
            self.damage.is_some(),
            self.frags.is_some(),
            self.speed.is_some(),
            self.accuracy.is_some(),
        ]
        .into_iter()
        .filter(|v| *v)
        .count()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.players.is_empty() {
            return Err("at least one player name is required".into());
        }
        if let Some(query) = &self.rule_query {
            if query.groups.is_empty() || query.groups.iter().any(|group| group.rules.is_empty()) {
                return Err("add at least one criterion to every group".into());
            }
            let mut ids = std::collections::BTreeSet::new();
            for rule in query.groups.iter().flat_map(|group| &group.rules) {
                if rule.id().is_empty() || !ids.insert(rule.id()) {
                    return Err("criteria must have unique ids".into());
                }
                match rule {
                    SearchRule::Speed { duration, .. }
                        if !duration.is_finite() || *duration < 0.0 =>
                    {
                        return Err("speed duration must be >= 0".into());
                    }
                    SearchRule::Accuracy { minimum, .. }
                        if !minimum.is_finite() || !(0.0..=100.0).contains(minimum) =>
                    {
                        return Err("reported accuracy must be between 0 and 100".into());
                    }
                    _ => {}
                }
            }
            if self.window.is_none() {
                return Err("a time window is required".into());
            }
            if self.window.is_some_and(|w| w <= 0.0 || !w.is_finite()) {
                return Err("time window must be > 0".into());
            }
            return Ok(());
        }
        if !self.has_threshold() && !self.win {
            return Err(
                "at least one damage, frag, speed, accuracy or round-win criterion is required"
                    .into(),
            );
        }
        if self.window.is_none() && self.has_threshold() {
            return Err("a time window is required with damage/frags/speed/accuracy".into());
        }
        if self.window.is_some_and(|w| w <= 0.0 || w.is_nan()) {
            return Err("--time_window must be > 0".into());
        }
        if self.speed_duration.is_some() && self.speed.is_none() {
            return Err("speed duration requires a minimum speed".into());
        }
        if self
            .speed_duration
            .is_some_and(|duration| duration < 0.0 || duration.is_nan())
        {
            return Err("speed duration must be >= 0".into());
        }
        if self.accuracy.is_some() && self.weapon.is_none() {
            return Err("reported accuracy requires a weapon".into());
        }
        if self
            .accuracy
            .is_some_and(|accuracy| !(0.0..=100.0).contains(&accuracy) || accuracy.is_nan())
        {
            return Err("reported accuracy must be between 0 and 100".into());
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
        if let Some(query) = &self.rule_query {
            let groups: Vec<String> = query
                .groups
                .iter()
                .map(|group| {
                    let rules: Vec<String> =
                        group.rules.iter().map(SearchRule::description).collect();
                    format!("({})", rules.join(group.condition.word()))
                })
                .collect();
            let mut desc = groups.join(query.condition.word());
            if let Some(window) = self.window {
                desc.push_str(&format!(" in {}s", fmt_g(window)));
            }
            let who: Vec<String> = self.players.iter().map(|p| py_repr(p)).collect();
            let plural = if self.players.len() > 1 { "s" } else { "" };
            return format!("# scene-finder  player{plural}={}  {desc}", who.join(", "));
        }
        let mut crit = Vec::new();
        if let Some(d) = self.damage {
            let prefix = self
                .weapon
                .map(|weapon| format!("{} ", weapon_name(weapon)))
                .unwrap_or_default();
            crit.push(format!("{prefix}dmg >= {d}"));
        }
        if let Some(f) = self.frags {
            crit.push(format!("frags >= {f}"));
        }
        if let Some(speed) = self.speed {
            let duration = self.speed_duration.unwrap_or(0.0);
            if duration > 0.0 {
                crit.push(format!("speed >= {speed} for {}s", fmt_g(duration)));
            } else {
                crit.push(format!("speed >= {speed}"));
            }
        }
        if let Some(accuracy) = self.accuracy {
            crit.push(format!(
                "{} reported accuracy >= {}%",
                weapon_name(self.weapon.unwrap_or_default()),
                fmt_g(accuracy)
            ));
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

pub fn weapon_name(id: u32) -> &'static str {
    match id {
        0 => "Melee",
        1 => "Machine Gun",
        2 => "Blaster",
        3 => "Super Shotgun",
        4 => "Rocket Launcher",
        5 => "Shaft",
        6 => "Crossbow",
        7 => "PnCR",
        8 => "Grenade Launcher",
        9 => "Weapon 9",
        10 => "Minigun",
        11 => "Showtime",
        12 => "Healing Weeball",
        13 => "Implosion Weeball",
        14 => "Slowfield Weeball",
        15 => "Explosive Weeball",
        16 => "Smoke Weeball",
        17 => "Knock Weeball",
        18 => "Hook",
        19 => "Void Cannon",
        20 => "Super Shotgun Secondary",
        21 => "Rocket Launcher Secondary",
        22 => "Void Cannon Secondary",
        23 => "Survival Amplifier",
        24 => "Survival Vulnerability",
        25 => "Survival Heal",
        205 => "Ring Out",
        _ => "weapon",
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
            weapon: None,
            speed: None,
            speed_duration: None,
            accuracy: None,
            condition: if or { Condition::Or } else { Condition::And },
            win,
            window,
            rule_query: None,
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
            "at least one damage, frag, speed, accuracy or round-win criterion is required"
        );
        assert_eq!(
            p(&["a"], Some(1), None, false, false, None)
                .validate()
                .unwrap_err(),
            "a time window is required with damage/frags/speed/accuracy"
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

    #[test]
    fn flexible_query_deserializes_from_gui_shape() {
        let params: Params = serde_json::from_str(
            r#"{
                "players":["alice"], "damage":null, "frags":null,
                "weapon":null, "speed":null, "speedDuration":null,
                "accuracy":null, "condition":"AND", "win":false, "window":8,
                "ruleQuery":{"condition":"OR","groups":[
                    {"id":"g1","condition":"AND","rules":[
                        {"id":"speed","kind":"speed","minimum":900,"duration":0.5},
                        {"id":"sipho","kind":"siphonator"},
                        {"id":"fallout","kind":"falloutDeath","exclude":true}
                    ]},
                    {"id":"g2","condition":"AND","rules":[
                        {"id":"flag","kind":"flagCarrier"},
                        {"id":"frag","kind":"frags","minimum":2,"weapon":7}
                    ]}
                ]}
            }"#,
        )
        .unwrap();
        params.validate().unwrap();
        assert!(params.header_line().contains(
            "(speed >= 900 for 0.5s AND siphonator active AND no fallout death) OR (carrying flag AND PnCR frags >= 2)"
        ));
    }
}
