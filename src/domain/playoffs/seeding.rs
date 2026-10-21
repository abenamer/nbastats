//! Era-specific NBA playoff seeding and qualification rules.
//!
//! * ≤ 2004: two division winners per conference take seeds 1–2 (by record), rest by record.
//! * 2005–2006: three division winners take seeds 1–3.
//! * 2007–2015: division winners are guaranteed a top-four seed; seeds 1–4 are the three
//!   division winners plus the best remaining team, ordered by record.
//! * 2016+: strictly by record.
//!
//! Qualification: 8 teams per conference through 2019; the 2020 restart used a play-in for the
//! 8th seed when the 9th-place team finished within four games; 2021+ uses the play-in
//! tournament for seeds 7–10.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::standings::Standing;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Qualification {
    Top8,
    Bubble2020,
    PlayIn,
}

pub fn qualification(season: i32) -> Qualification {
    match season {
        s if s <= 2019 => Qualification::Top8,
        2020 => Qualification::Bubble2020,
        _ => Qualification::PlayIn,
    }
}

/// Order the conference (already sorted by record with tiebreaks) into seed order.
/// Returns the full ordering (seed 1 first).
pub fn seed_order(season: i32, ordered: &[Standing]) -> Vec<Standing> {
    if ordered.is_empty() {
        return vec![];
    }
    // division winners: first occurrence of each division in record order
    let mut seen = std::collections::HashSet::new();
    let mut div_winners: Vec<usize> = Vec::new();
    for (i, s) in ordered.iter().enumerate() {
        if seen.insert(s.division.clone()) {
            div_winners.push(i);
        }
    }
    let protected = match season {
        s if s <= 2004 => 2usize,
        2005 | 2006 => 3,
        s if s <= 2015 => 4,
        _ => 0,
    };
    if protected == 0 {
        return ordered.to_vec();
    }
    let mut top: Vec<usize> = Vec::new();
    if protected == 4 {
        // three division winners + best remaining team, then ordered by record
        top.extend(div_winners.iter().take(3));
        if let Some(best_other) = (0..ordered.len()).find(|i| !top.contains(i)) {
            top.push(best_other);
        }
    } else {
        top.extend(div_winners.iter().take(protected));
    }
    top.sort();
    let mut out: Vec<Standing> = top.iter().map(|&i| ordered[i].clone()).collect();
    for (i, s) in ordered.iter().enumerate() {
        if !top.contains(&i) {
            out.push(s.clone());
        }
    }
    out
}

/// Seed number (1-based) by team id from a seed ordering.
pub fn seed_map(order: &[Standing]) -> HashMap<i32, u8> {
    order.iter().enumerate().map(|(i, s)| (s.team_id, (i + 1) as u8)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::Conference;

    fn s(id: i32, div: &str, w: i32) -> Standing {
        Standing { team_id: id, conference: Conference::West, division: div.into(), wins: w, losses: 82 - w, point_diff: 0, conf_wins: 0, conf_losses: 0, div_wins: 0, div_losses: 0 }
    }

    #[test]
    fn division_winner_rules_2006() {
        // 2006 West by record: SAS 63, DAL 60, PHO 54, LAC 47, MEM 49, DEN 44, LAL 45, SAC 44
        let ordered = vec![
            s(1, "SW", 63), // SAS
            s(2, "SW", 60), // DAL
            s(3, "P", 54),  // PHO
            s(4, "SW", 49), // MEM
            s(5, "P", 47),  // LAC
            s(6, "P", 45),  // LAL
            s(7, "NW", 44), // DEN
            s(8, "P", 44),  // SAC
        ];
        let seeds: Vec<i32> = seed_order(2006, &ordered).iter().map(|x| x.team_id).collect();
        assert_eq!(seeds, vec![1, 3, 7, 2, 4, 5, 6, 8]);
        // 2007+ rule: top four = 3 division winners + best other, by record -> SAS, DAL, PHO, DEN
        let seeds: Vec<i32> = seed_order(2007, &ordered).iter().map(|x| x.team_id).collect();
        assert_eq!(seeds, vec![1, 2, 3, 7, 4, 5, 6, 8]);
        // 2016+: by record
        let seeds: Vec<i32> = seed_order(2016, &ordered).iter().map(|x| x.team_id).collect();
        assert_eq!(seeds, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn qualification_eras() {
        assert_eq!(qualification(2019), Qualification::Top8);
        assert_eq!(qualification(2020), Qualification::Bubble2020);
        assert_eq!(qualification(2021), Qualification::PlayIn);
    }
}
