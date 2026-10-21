//! Regular-season standings computed from game records (played or simulated).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::Conference;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Standing {
    pub team_id: i32,
    pub conference: Conference,
    pub division: String,
    pub wins: i32,
    pub losses: i32,
    pub point_diff: i32,
    /// Record against conference opponents (NBA tiebreak criterion).
    #[serde(default)]
    pub conf_wins: i32,
    #[serde(default)]
    pub conf_losses: i32,
    /// Record against division opponents (NBA tiebreak criterion).
    #[serde(default)]
    pub div_wins: i32,
    #[serde(default)]
    pub div_losses: i32,
}

fn pct(w: i32, l: i32) -> f64 {
    if w + l == 0 {
        0.0
    } else {
        f64::from(w) / f64::from(w + l)
    }
}

impl Standing {
    pub fn win_pct(&self) -> f64 {
        pct(self.wins, self.losses)
    }
    pub fn conf_pct(&self) -> f64 {
        pct(self.conf_wins, self.conf_losses)
    }
    pub fn div_pct(&self) -> f64 {
        pct(self.div_wins, self.div_losses)
    }
}

/// A minimal game record used by both historical and simulated standings.
#[derive(Debug, Clone, Copy)]
pub struct GameResult {
    pub home: i32,
    pub away: i32,
    pub home_won: bool,
    pub margin: i32,
}

pub struct StandingsTable {
    pub rows: Vec<Standing>,
    /// head-to-head wins: (winner, loser) -> count
    pub h2h: HashMap<(i32, i32), i32>,
}

impl StandingsTable {
    pub fn new(alignment: &HashMap<i32, (Conference, String)>) -> Self {
        let rows = alignment
            .iter()
            .map(|(&team_id, (c, d))| Standing {
                team_id,
                conference: *c,
                division: d.clone(),
                wins: 0,
                losses: 0,
                point_diff: 0,
                conf_wins: 0,
                conf_losses: 0,
                div_wins: 0,
                div_losses: 0,
            })
            .collect();
        Self { rows, h2h: HashMap::new() }
    }

    pub fn index(&self) -> HashMap<i32, usize> {
        self.rows.iter().enumerate().map(|(i, s)| (s.team_id, i)).collect()
    }

    pub fn record(&mut self, idx: &HashMap<i32, usize>, g: &GameResult) {
        let (winner, loser) = if g.home_won { (g.home, g.away) } else { (g.away, g.home) };
        let (Some(&wi), Some(&li)) = (idx.get(&winner), idx.get(&loser)) else { return };
        let same_conf = self.rows[wi].conference == self.rows[li].conference;
        let same_div = same_conf && self.rows[wi].division == self.rows[li].division;
        let w = &mut self.rows[wi];
        w.wins += 1;
        w.point_diff += g.margin.abs();
        w.conf_wins += i32::from(same_conf);
        w.div_wins += i32::from(same_div);
        let l = &mut self.rows[li];
        l.losses += 1;
        l.point_diff -= g.margin.abs();
        l.conf_losses += i32::from(same_conf);
        l.div_losses += i32::from(same_div);
        *self.h2h.entry((winner, loser)).or_insert(0) += 1;
    }

    pub fn h2h_edge(&self, a: i32, b: i32) -> i32 {
        self.h2h.get(&(a, b)).copied().unwrap_or(0) - self.h2h.get(&(b, a)).copied().unwrap_or(0)
    }

    /// Order teams of one conference: win pct, then the head-to-head record within each group
    /// of tied teams (the NBA's group tiebreak, which is transitive), then point differential,
    /// then a caller-supplied tiebreak key (random for simulations, team id for history).
    pub fn conference_order(&self, conference: Conference, tiebreak: &HashMap<i32, u64>) -> Vec<Standing> {
        let rows: Vec<Standing> = self.rows.iter().filter(|s| s.conference == conference).cloned().collect();
        order_with_tiebreaks(rows, |a, b| self.h2h.get(&(a, b)).copied().unwrap_or(0), |t| tiebreak.get(&t).copied().unwrap_or(0))
    }
}

/// Sort standings by win pct, breaking ties within each equal-record group the way the NBA
/// does: division winners first, then the group's internal head-to-head winning percentage,
/// then division record (when all tied teams share a division), then conference record, then
/// point differential (a stand-in for the remaining criteria), then `key`.
pub fn order_with_tiebreaks<H, K>(rows: Vec<Standing>, h2h_wins: H, key: K) -> Vec<Standing>
where
    H: Fn(i32, i32) -> i32,
    K: Fn(i32) -> u64,
{
    // Division winners: best record in each division, ties broken by the same group rules
    // (without the division-winner criterion, which would be circular).
    let mut by_division: HashMap<&str, Vec<Standing>> = HashMap::new();
    for s in &rows {
        by_division.entry(s.division.as_str()).or_default().push(s.clone());
    }
    let division_winners: std::collections::HashSet<i32> = by_division
        .into_values()
        .filter_map(|div| sort_groups(div, &h2h_wins, &key, |_| false).first().map(|s| s.team_id))
        .collect();
    sort_groups(rows, &h2h_wins, &key, |t| division_winners.contains(&t))
}

fn sort_groups<H, K, D>(mut rows: Vec<Standing>, h2h_wins: &H, key: &K, is_division_winner: D) -> Vec<Standing>
where
    H: Fn(i32, i32) -> i32,
    K: Fn(i32) -> u64,
    D: Fn(i32) -> bool,
{
    rows.sort_by(|a, b| b.win_pct().partial_cmp(&a.win_pct()).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::with_capacity(rows.len());
    let mut i = 0;
    while i < rows.len() {
        let mut j = i + 1;
        while j < rows.len() && rows[j].win_pct() == rows[i].win_pct() {
            j += 1;
        }
        let mut group: Vec<Standing> = rows[i..j].to_vec();
        // NBA multi-team procedure: rank the tied group, remove the top team, then restart the
        // procedure with the teams that remain (their head-to-head record is recomputed).
        while !group.is_empty() {
            let ids: Vec<i32> = group.iter().map(|s| s.team_id).collect();
            let group_pct = |t: i32| -> f64 {
                let (w, l) = ids.iter().filter(|&&o| o != t).fold((0, 0), |(w, l), &o| (w + h2h_wins(t, o), l + h2h_wins(o, t)));
                if w + l == 0 {
                    0.5
                } else {
                    f64::from(w) / f64::from(w + l)
                }
            };
            let same_division = group.iter().all(|s| s.division == group[0].division);
            let cmp_f = |x: f64, y: f64| y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal);
            let best = group
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    is_division_winner(b.team_id)
                        .cmp(&is_division_winner(a.team_id))
                        .then_with(|| cmp_f(group_pct(a.team_id), group_pct(b.team_id)))
                        .then_with(|| if same_division { cmp_f(a.div_pct(), b.div_pct()) } else { std::cmp::Ordering::Equal })
                        .then_with(|| cmp_f(a.conf_pct(), b.conf_pct()))
                        .then_with(|| b.point_diff.cmp(&a.point_diff))
                        .then_with(|| key(a.team_id).cmp(&key(b.team_id)))
                        .then_with(|| a.team_id.cmp(&b.team_id))
                })
                .map(|(idx, _)| idx)
                .expect("non-empty group");
            out.push(group.remove(best));
        }
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_tiebreaks_follow_nba_procedure() {
        let mut al = HashMap::new();
        for t in 1..=3 {
            al.insert(t, (Conference::East, "A".to_string()));
        }
        let mut t = StandingsTable::new(&al);
        let idx = t.index();
        t.record(&idx, &GameResult { home: 1, away: 2, home_won: false, margin: 5 }); // 2 beat 1
        t.record(&idx, &GameResult { home: 2, away: 3, home_won: false, margin: 5 }); // 3 beat 2
        t.record(&idx, &GameResult { home: 3, away: 1, home_won: false, margin: 25 }); // 1 beat 3
        // All 1-1, circular head-to-head (0.5 each) -> point differential picks 1 (+20).
        // The tie then restarts for {2, 3}: 3 beat 2, so 3 is ahead of 2.
        let order: Vec<i32> = t.conference_order(Conference::East, &HashMap::new()).iter().map(|s| s.team_id).collect();
        assert_eq!(order, vec![1, 3, 2]);
        t.record(&idx, &GameResult { home: 3, away: 1, home_won: true, margin: 1 });
        t.record(&idx, &GameResult { home: 2, away: 3, home_won: true, margin: 1 });
        t.record(&idx, &GameResult { home: 1, away: 2, home_won: true, margin: 1 });
        // Everyone 2-2 overall and 2-2 inside the group; 2 and 3 split their games -> point differential.
        let order: Vec<i32> = t.conference_order(Conference::East, &HashMap::new()).iter().map(|s| s.team_id).collect();
        assert_eq!(order, vec![1, 2, 3]);
    }

    #[test]
    fn division_winner_ranks_first_in_a_tie() {
        let mut al = HashMap::new();
        al.insert(1, (Conference::East, "A".to_string()));
        al.insert(2, (Conference::East, "A".to_string()));
        al.insert(3, (Conference::East, "B".to_string()));
        let mut t = StandingsTable::new(&al);
        let idx = t.index();
        t.record(&idx, &GameResult { home: 1, away: 3, home_won: true, margin: 30 }); // 1 beat 3
        t.record(&idx, &GameResult { home: 2, away: 1, home_won: true, margin: 1 }); // 2 beat 1
        t.record(&idx, &GameResult { home: 3, away: 2, home_won: true, margin: 1 }); // 3 beat 2
        // 2 wins division A on head-to-head over 1; 3 wins division B. Division winners first:
        // 2 and 3 both 0.5 in the group -> point differential (2: 0, 3: -29) -> 2, then 3, then 1.
        let order: Vec<i32> = t.conference_order(Conference::East, &HashMap::new()).iter().map(|s| s.team_id).collect();
        assert_eq!(order, vec![2, 3, 1]);
    }
}
