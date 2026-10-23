//! Era builder: group seasons by one or more dimensions and compute the records of each
//! group. A dimension maps a season of a league to one label (a bargaining period, a rule
//! period, a league size, a league, a decade, a competition format) or to several (every
//! dynasty era and every player active in it). Combining dimensions intersects them, so only
//! combinations that occur in the data appear. Each group gets the same records as a
//! bargaining period: best and weakest team-season, titles and champions, title HHI and Elo
//! spread, plus the notes of the periods it belongs to.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::domain::cba::{cba_of, group_stats, GroupStats};
use crate::domain::ids::SeasonYear;
use crate::domain::models::PlayoffResult;
use crate::domain::players::active_in;
use crate::domain::records::{
    change, decade_of, period_of, Change, ChangeNote, League, TeamSeasonStat,
};

/// Dimensions a query may combine.
pub const MAX_DIMENSIONS: usize = 4;
/// Default thresholds of a dynasty era.
pub const DYNASTY_TITLES: usize = 3;
pub const DYNASTY_GAP: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dimension {
    /// Collective bargaining period (NBA/BAA).
    Cba,
    /// Rule period.
    Rule,
    /// Number of teams in the league.
    Size,
    /// NBA/BAA or ABA.
    League,
    /// Decade of the season's start year.
    Decade,
    /// Competition format: playoff field, series length, play-in, NBA Cup.
    Format,
    /// A franchise's run of titles (computed from the champions).
    Dynasty,
    /// A listed player's active seasons (curated careers; a season belongs to each one).
    Player,
}

impl Dimension {
    pub const ALL: [Dimension; 8] = [
        Dimension::Cba,
        Dimension::Rule,
        Dimension::Size,
        Dimension::League,
        Dimension::Decade,
        Dimension::Format,
        Dimension::Dynasty,
        Dimension::Player,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Dimension::Cba => "cba",
            Dimension::Rule => "rule",
            Dimension::Size => "size",
            Dimension::League => "league",
            Dimension::Decade => "decade",
            Dimension::Format => "format",
            Dimension::Dynasty => "dynasty",
            Dimension::Player => "player",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Dimension::Cba => "CBA period",
            Dimension::Rule => "Rule period",
            Dimension::Size => "League size",
            Dimension::League => "League",
            Dimension::Decade => "Decade",
            Dimension::Format => "Format",
            Dimension::Dynasty => "Dynasty era",
            Dimension::Player => "Player era",
        }
    }
}

impl FromStr for Dimension {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Dimension::ALL
            .iter()
            .copied()
            .find(|d| d.as_str() == s.trim().to_ascii_lowercase())
            .ok_or_else(|| {
                let shown: String = s.chars().take(40).collect();
                format!("unknown dimension {shown:?}; use cba, rule, size, league, decade, format, dynasty or player")
            })
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A competition-format period: how the postseason field, the series and the season's
/// extra competitions were organised. Curated from the public record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FormatPeriod {
    pub first_season: SeasonYear,
    pub name: &'static str,
    pub summary: &'static str,
    pub changes: &'static [Change],
}

/// The NBA/BAA format periods, in season order.
pub const FORMAT_PERIODS: &[FormatPeriod] = &[
    FormatPeriod {
        first_season: SeasonYear::of(1947),
        name: "Early playoffs",
        summary: "Division-based playoff fields that changed often: six, eight, then ten and twelve teams, with short early series.",
        changes: &[
            change(1947, "The BAA's first playoffs: six teams, with the division winners meeting in a semifinal."),
            change(1950, "The merged league of seventeen teams plays a twelve-team postseason in three divisions."),
            change(1954, "A round-robin opens the playoffs; the format returns to division series the next season."),
            change(1967, "The field grows to eight teams with the first expansion."),
            change(1975, "Ten teams qualify; the best second-place teams play a first-round miniseries."),
            change(1977, "Twelve teams qualify after the ABA merger; the top seeds skip the first round."),
        ],
    },
    FormatPeriod {
        first_season: SeasonYear::of(1984),
        name: "16-team field",
        summary: "Sixteen teams, eight per conference, with a best-of-five first round.",
        changes: &[
            change(1984, "Sixteen teams qualify; the first round is best-of-five and no seed has a bye."),
            change(1985, "The Finals move to the 2-3-2 home pattern, kept until 2013-14."),
        ],
    },
    FormatPeriod {
        first_season: SeasonYear::of(2003),
        name: "Best-of-seven throughout",
        summary: "Every round is best-of-seven; from 2015-16 the top eight of each conference qualify by record, without a division-winner guarantee.",
        changes: &[
            change(2003, "The first round becomes best-of-seven."),
            change(2007, "Seeds 1 to 4 go to the three division winners and the best other team, ordered by record."),
            change(2016, "Division winners get no guaranteed seed; the top eight by record qualify."),
            change(2020, "The Orlando bubble finishes the season with eight seeding games and a one-off play-in for the eighth seed."),
        ],
    },
    FormatPeriod {
        first_season: SeasonYear::of(2021),
        name: "Play-in era",
        summary: "Seeds seven to ten play a tournament for the last two playoff places in each conference.",
        changes: &[
            change(2021, "The play-in tournament: the seventh and eighth seeds play for one place, the losers and the ninth and tenth for the other."),
            change(2023, "The play-in becomes permanent in the 2023 agreement."),
        ],
    },
    FormatPeriod {
        first_season: SeasonYear::of(2024),
        name: "NBA Cup era",
        summary: "An in-season tournament with group games that count in the standings and a neutral-site final that does not.",
        changes: &[
            change(2024, "The in-season tournament (the NBA Cup): group play in November, knockout rounds in December, a final in Las Vegas that is not a standings game."),
            change(2025, "The tournament keeps its format; the final returns to Las Vegas."),
        ],
    },
];

/// The format period an NBA/BAA season belongs to.
pub fn format_of(season: SeasonYear) -> &'static FormatPeriod {
    FORMAT_PERIODS
        .iter()
        .rev()
        .find(|p| p.first_season <= season)
        .unwrap_or(&FORMAT_PERIODS[0])
}

/// Familiar names for the dynasties the data produces at the default thresholds, by
/// franchise key and first title; a run without a name is "<franchise> dynasty".
const DYNASTY_NICKNAMES: &[(&str, i32, &str)] = &[
    ("LAL", 1949, "Mikan Lakers"),
    ("BOS", 1957, "Russell Celtics"),
    ("BOS", 1974, "Cowens Celtics"),
    ("LAL", 1980, "Showtime Lakers"),
    ("BOS", 1981, "Bird Celtics"),
    ("BOS", 1984, "Bird Celtics"),
    ("LAL", 1985, "Showtime Lakers"),
    ("DET", 1989, "Bad Boys Pistons"),
    ("CHI", 1991, "Jordan Bulls"),
    ("HOU", 1994, "Olajuwon Rockets"),
    ("CHI", 1996, "Jordan Bulls"),
    ("SAS", 1999, "Duncan Spurs"),
    ("LAL", 2000, "Shaq and Kobe Lakers"),
    ("SAS", 2003, "Duncan Spurs"),
    ("LAL", 2009, "Kobe and Gasol Lakers"),
    ("MIA", 2012, "Heatles"),
    ("GSW", 2015, "Curry Warriors"),
];

/// A franchise's run of titles: at least `min_titles` titles with no gap longer than
/// `max_gap` seasons between two of them, from the first title to the last.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dynasty {
    pub franchise_key: String,
    pub name: String,
    pub label: String,
    pub first_season: SeasonYear,
    pub last_season: SeasonYear,
    pub titles: Vec<SeasonYear>,
}

/// The dynasties in `stats` (NBA/BAA champions grouped by franchise), in season order.
pub fn dynasties(stats: &[TeamSeasonStat], min_titles: usize, max_gap: usize) -> Vec<Dynasty> {
    let min_titles = min_titles.max(1);
    // Titles per franchise with the team name of each title season, so a run is named after
    // the team as it was at its last title (the Lakers, not the Minneapolis Lakers).
    let mut titles: BTreeMap<&str, Vec<(SeasonYear, &str)>> = BTreeMap::new();
    for s in stats.iter().filter(|s| {
        s.playoff_result == PlayoffResult::Champion && League::of_code(&s.code) == League::Nba
    }) {
        titles
            .entry(s.franchise_key.as_str())
            .or_default()
            .push((s.season, s.name.as_str()));
    }
    let mut out = Vec::new();
    for (key, mut seasons) in titles {
        seasons.sort_by_key(|(season, _)| *season);
        seasons.dedup_by_key(|(season, _)| *season);
        let mut run: Vec<(SeasonYear, &str)> = Vec::new();
        let flush = |run: &mut Vec<(SeasonYear, &str)>, out: &mut Vec<Dynasty>| {
            if let (true, Some(&(first, _)), Some(&(last, name))) =
                (run.len() >= min_titles, run.first(), run.last())
            {
                let label = DYNASTY_NICKNAMES
                    .iter()
                    .find(|(k, y, _)| *k == key && *y == first.end_year())
                    .map(|(_, _, n)| n.to_string())
                    .unwrap_or_else(|| format!("{name} dynasty"));
                out.push(Dynasty {
                    franchise_key: key.to_string(),
                    name: name.to_string(),
                    label,
                    first_season: first,
                    last_season: last,
                    titles: run.iter().map(|(s, _)| *s).collect(),
                });
            }
            run.clear();
        };
        for (s, name) in seasons {
            let gap = run
                .last()
                .map(|(last, _)| usize::try_from(s.end_year() - last.end_year() - 1).unwrap_or(0));
            if gap.is_some_and(|g| g > max_gap) {
                flush(&mut run, &mut out);
            }
            run.push((s, name));
        }
        flush(&mut run, &mut out);
    }
    out.sort_by(|a, b| {
        a.first_season
            .cmp(&b.first_season)
            .then(a.franchise_key.cmp(&b.franchise_key))
    });
    out
}

/// A group query: the dimensions to combine, the filters and the dynasty thresholds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupQuery {
    pub dimensions: Vec<Dimension>,
    pub league: Option<League>,
    pub from: Option<SeasonYear>,
    pub to: Option<SeasonYear>,
    pub dynasty_titles: usize,
    pub dynasty_gap: usize,
}

impl Default for GroupQuery {
    fn default() -> Self {
        Self {
            dimensions: Vec::new(),
            league: None,
            from: None,
            to: None,
            dynasty_titles: DYNASTY_TITLES,
            dynasty_gap: DYNASTY_GAP,
        }
    }
}

/// The period a group part belongs to, with its notes, for the "what changed" list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    pub dimension: Dimension,
    pub name: String,
    pub summary: String,
    pub changes: Vec<ChangeNote>,
}

/// One group: the combination of labels, the seasons it covers and its records. `notes`
/// are keys into `Grouping::notes`, so a period's changes travel once per answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupRow {
    pub key: String,
    pub parts: Vec<String>,
    pub first_season: SeasonYear,
    pub last_season: SeasonYear,
    pub seasons: usize,
    #[serde(flatten)]
    pub stats: GroupStats,
    pub notes: Vec<String>,
}

/// The result of a query.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Grouping {
    pub query: GroupQuery,
    pub dynasties: Vec<Dynasty>,
    pub groups: Vec<GroupRow>,
    /// The notes of every period the groups belong to, keyed by `<dimension>:<label>`.
    pub notes: BTreeMap<String, Note>,
}

fn note_key(dim: Dimension, label: &str) -> String {
    format!("{dim}:{label}")
}

/// What the dimensions need beyond the stats: the league sizes and the dynasties.
struct Context<'a> {
    sizes: &'a BTreeMap<(SeasonYear, League), usize>,
    dynasties: &'a [Dynasty],
}

/// The labels of a season of a league along one dimension; empty means the season is not
/// in the dimension (no listed player active).
fn labels(dim: Dimension, league: League, season: SeasonYear, ctx: &Context<'_>) -> Vec<String> {
    match dim {
        Dimension::Cba => vec![match league {
            League::Nba => cba_of(season).name.to_string(),
            League::Aba => "ABA (no agreement)".to_string(),
        }],
        Dimension::Rule => vec![period_of(league, season).name.to_string()],
        Dimension::Size => vec![format!(
            "{} teams",
            ctx.sizes.get(&(season, league)).copied().unwrap_or(0)
        )],
        Dimension::League => vec![league.label().to_string()],
        Dimension::Decade => vec![format!("{}s", decade_of(season))],
        Dimension::Format => vec![match league {
            League::Nba => format_of(season).name.to_string(),
            League::Aba => "ABA playoffs".to_string(),
        }],
        Dimension::Dynasty => vec![match league {
            League::Aba => "ABA (not computed)".to_string(),
            League::Nba => {
                let names: Vec<&str> = ctx
                    .dynasties
                    .iter()
                    .filter(|d| d.first_season <= season && season <= d.last_season)
                    .map(|d| d.label.as_str())
                    .collect();
                if names.is_empty() {
                    "No dynasty".to_string()
                } else {
                    names.join(" + ")
                }
            }
        }],
        Dimension::Player => active_in(league, season)
            .map(|p| p.name.to_string())
            .collect(),
    }
}

/// The notes of one label along a dimension, when the dimension has a period table.
fn note_of(dim: Dimension, label: &str, ctx: &Context<'_>) -> Option<Note> {
    let mk = |name: &str, summary: &str, changes: &[Change]| Note {
        dimension: dim,
        name: name.to_string(),
        summary: summary.to_string(),
        changes: changes.iter().map(ChangeNote::from).collect(),
    };
    match dim {
        Dimension::Cba => crate::domain::cba::CBA_PERIODS
            .iter()
            .find(|p| p.name == label)
            .map(|p| mk(p.name, p.summary, p.changes)),
        Dimension::Rule => crate::domain::records::RULE_PERIODS
            .iter()
            .find(|p| p.name == label)
            .map(|p| mk(p.name, p.rule, p.changes)),
        Dimension::Format => FORMAT_PERIODS
            .iter()
            .find(|p| p.name == label)
            .map(|p| mk(p.name, p.summary, p.changes)),
        Dimension::Player => crate::domain::players::PLAYER_ERAS
            .iter()
            .find(|p| p.name == label)
            .map(|p| mk(p.name, p.summary, p.changes)),
        Dimension::Dynasty => {
            // A nickname can name several runs (the two Bulls three-peats at a small gap):
            // every run with the label belongs to the note.
            let parts: Vec<&Dynasty> = label
                .split(" + ")
                .flat_map(|l| ctx.dynasties.iter().filter(move |d| d.label == l))
                .collect();
            if parts.is_empty() {
                return None;
            }
            let mut changes: Vec<ChangeNote> = parts
                .iter()
                .flat_map(|d| {
                    d.titles.iter().map(|s| ChangeNote {
                        season: *s,
                        text: format!("{} win the title.", d.name),
                    })
                })
                .collect();
            changes.sort_by_key(|c| c.season);
            let summary = parts
                .iter()
                .map(|d| {
                    format!(
                        "{}: {} titles, {} to {}",
                        d.name,
                        d.titles.len(),
                        d.first_season.label(),
                        d.last_season.label()
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            Some(Note {
                dimension: dim,
                name: label.to_string(),
                summary,
                changes,
            })
        }
        Dimension::Size | Dimension::League | Dimension::Decade => None,
    }
}

/// Group the team-seasons by the query's dimensions and compute each group's records.
pub fn group(
    query: &GroupQuery,
    stats: &[TeamSeasonStat],
    sizes: &BTreeMap<(SeasonYear, League), usize>,
) -> Grouping {
    let dynasties = dynasties(stats, query.dynasty_titles, query.dynasty_gap);
    let ctx = Context {
        sizes,
        dynasties: &dynasties,
    };
    let mut combos: HashMap<(SeasonYear, League), Vec<Vec<String>>> = HashMap::new();
    let mut members: BTreeMap<String, (Vec<String>, Vec<&TeamSeasonStat>)> = BTreeMap::new();
    for s in stats {
        let league = League::of_code(&s.code);
        if query.league.is_some_and(|l| l != league)
            || query.from.is_some_and(|f| s.season < f)
            || query.to.is_some_and(|t| s.season > t)
        {
            continue;
        }
        let keys = combos.entry((s.season, league)).or_insert_with(|| {
            let sets: Vec<Vec<String>> = query
                .dimensions
                .iter()
                .map(|d| labels(*d, league, s.season, &ctx))
                .collect();
            if sets.iter().any(Vec::is_empty) {
                return Vec::new();
            }
            sets.iter().fold(vec![Vec::new()], |acc, set| {
                acc.iter()
                    .flat_map(|prefix| {
                        set.iter().map(move |v| {
                            let mut p = prefix.clone();
                            p.push(v.clone());
                            p
                        })
                    })
                    .collect()
            })
        });
        for parts in keys.iter() {
            let key = if parts.is_empty() {
                "All seasons".to_string()
            } else {
                parts.join(" · ")
            };
            members
                .entry(key)
                .or_insert_with(|| (parts.clone(), Vec::new()))
                .1
                .push(s);
        }
    }
    let mut notes: BTreeMap<String, Note> = BTreeMap::new();
    let mut groups: Vec<GroupRow> = members
        .into_iter()
        .filter_map(|(key, (parts, rows))| {
            let mut seasons: Vec<SeasonYear> = rows.iter().map(|s| s.season).collect();
            seasons.sort_unstable();
            seasons.dedup();
            let mut keys = Vec::new();
            for (d, label) in query.dimensions.iter().zip(parts.iter()) {
                let k = note_key(*d, label);
                if !notes.contains_key(&k) {
                    if let Some(n) = note_of(*d, label, &ctx) {
                        notes.insert(k.clone(), n);
                    }
                }
                if notes.contains_key(&k) {
                    keys.push(k);
                }
            }
            Some(GroupRow {
                key,
                parts,
                first_season: *seasons.first()?,
                last_season: *seasons.last()?,
                seasons: seasons.len(),
                stats: group_stats(&rows)?,
                notes: keys,
            })
        })
        .collect();
    groups.sort_by(|a, b| a.first_season.cmp(&b.first_season).then(a.key.cmp(&b.key)));
    Grouping {
        query: query.clone(),
        dynasties,
        groups,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::GameType;
    use chrono::NaiveDate;

    fn stat(season: i32, code: &str, key: &str, peak: f64, champion: bool) -> TeamSeasonStat {
        TeamSeasonStat {
            season: SeasonYear::of(season),
            code: code.to_string(),
            name: format!("Team {code}"),
            franchise_key: key.to_string(),
            games: 82,
            peak,
            peak_date: NaiveDate::from_ymd_opt(season, 5, 1).unwrap(),
            peak_game_type: GameType::Regular,
            low: peak - 200.0,
            low_date: NaiveDate::from_ymd_opt(season - 1, 11, 1).unwrap(),
            low_game_type: GameType::Regular,
            wins: 50,
            losses: 32,
            playoff_wins: 0,
            playoff_losses: 0,
            elo_start: Some(1500.0),
            elo_pre_playoffs: Some(peak - 20.0),
            elo_end: Some(peak - 10.0),
            playoff_result: if champion {
                PlayoffResult::Champion
            } else {
                PlayoffResult::FirstRound
            },
            conference: None,
            division: None,
        }
    }

    fn sizes(stats: &[TeamSeasonStat]) -> BTreeMap<(SeasonYear, League), usize> {
        let mut m = BTreeMap::new();
        for s in stats {
            *m.entry((s.season, League::of_code(&s.code))).or_default() += 1;
        }
        m
    }

    #[test]
    fn dimensions_parse_and_print() {
        for d in Dimension::ALL {
            assert_eq!(d.as_str().parse::<Dimension>().unwrap(), d);
        }
        assert_eq!("CBA".parse::<Dimension>().unwrap(), Dimension::Cba);
        assert!("salary".parse::<Dimension>().is_err());
        assert_eq!(
            format_of(SeasonYear::of(2020)).name,
            "Best-of-seven throughout"
        );
        assert_eq!(format_of(SeasonYear::of(2021)).name, "Play-in era");
        assert_eq!(format_of(SeasonYear::of(2024)).name, "NBA Cup era");
        assert_eq!(format_of(SeasonYear::of(1947)).name, "Early playoffs");
    }

    #[test]
    fn dynasties_are_runs_of_titles_with_bounded_gaps() {
        // Lakers 2000-02 and 2009-10; Spurs 1999, 2003, 2005, 2007, 2014.
        let mut stats = Vec::new();
        for (y, key) in [
            (1999, "SAS"),
            (2000, "LAL"),
            (2001, "LAL"),
            (2002, "LAL"),
            (2003, "SAS"),
            (2005, "SAS"),
            (2007, "SAS"),
            (2009, "LAL"),
            (2010, "LAL"),
            (2014, "SAS"),
        ] {
            stats.push(stat(y, key, key, 1700.0, true));
            stats.push(stat(y, "BOS", "BOS", 1600.0, false));
        }
        let d = dynasties(&stats, 3, 3);
        let labels: Vec<(&str, i32, i32, usize)> = d
            .iter()
            .map(|x| {
                (
                    x.label.as_str(),
                    x.first_season.end_year(),
                    x.last_season.end_year(),
                    x.titles.len(),
                )
            })
            .collect();
        assert_eq!(
            labels,
            [
                ("Duncan Spurs", 1999, 2007, 4),
                ("Shaq and Kobe Lakers", 2000, 2002, 3)
            ]
        );
        // Two titles with no gap count at a lower threshold; a run without a nickname is named after the franchise.
        let d = dynasties(&stats, 2, 0);
        assert!(d
            .iter()
            .any(|x| x.label == "Kobe and Gasol Lakers" && x.first_season.end_year() == 2009));
        let mut odd = stats.clone();
        odd.push(stat(2016, "CLE", "CLE", 1700.0, true));
        odd.push(stat(2017, "CLE", "CLE", 1700.0, true));
        let d = dynasties(&odd, 2, 0);
        assert!(d.iter().any(|x| x.label == "Team CLE dynasty"));
        // ABA champions are not dynasties.
        let aba = vec![
            stat(1972, "INA", "INA", 1700.0, true),
            stat(1973, "INA", "INA", 1700.0, true),
        ];
        assert!(dynasties(&aba, 1, 0).is_empty());
    }

    #[test]
    fn groups_intersect_dimensions_and_players_overlap() {
        let stats = vec![
            stat(1986, "BOS", "BOS", 1816.0, true),
            stat(1986, "LAL", "LAL", 1760.0, false),
            stat(1987, "LAL", "LAL", 1780.0, true),
            stat(1987, "BOS", "BOS", 1750.0, false),
            stat(2016, "CLE", "CLE", 1750.0, true),
            stat(2016, "GSW", "GSW", 1839.0, false),
            stat(1974, "INA", "INA", 1650.0, true),
        ];
        let sz = sizes(&stats);
        // One dimension: CBA; the ABA season is its own group.
        let q = GroupQuery {
            dimensions: vec![Dimension::Cba],
            ..Default::default()
        };
        let g = group(&q, &stats, &sz);
        let keys: Vec<&str> = g.groups.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys,
            ["ABA (no agreement)", "First salary cap", "2011 agreement"]
        );
        let cap = &g.groups[1];
        assert_eq!(
            (cap.seasons, cap.stats.titles, cap.stats.distinct_champions),
            (2, 2, 2)
        );
        assert_eq!(cap.stats.best.code, "BOS");
        assert_eq!(cap.notes, ["cba:First salary cap"]);
        assert_eq!(g.notes["cba:First salary cap"].dimension, Dimension::Cba);
        // Two dimensions intersect; filters apply.
        let q = GroupQuery {
            dimensions: vec![Dimension::Cba, Dimension::Size],
            league: Some(League::Nba),
            from: Some(SeasonYear::of(1987)),
            ..Default::default()
        };
        let g = group(&q, &stats, &sz);
        let keys: Vec<&str> = g.groups.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys,
            ["First salary cap · 2 teams", "2011 agreement · 2 teams"]
        );
        assert_eq!(g.groups[0].parts, ["First salary cap", "2 teams"]);
        // The player dimension puts 1986-87 in several groups and drops a season without a listed player.
        let q = GroupQuery {
            dimensions: vec![Dimension::Player],
            league: Some(League::Nba),
            to: Some(SeasonYear::of(1987)),
            ..Default::default()
        };
        let g = group(&q, &stats, &sz);
        let keys: Vec<&str> = g.groups.iter().map(|r| r.key.as_str()).collect();
        assert!(
            keys.contains(&"Larry Bird")
                && keys.contains(&"Magic Johnson")
                && keys.contains(&"Michael Jordan")
        );
        let bird = g.groups.iter().find(|r| r.key == "Larry Bird").unwrap();
        assert_eq!(bird.seasons, 2);
        assert_eq!(g.notes[&bird.notes[0]].name, "Larry Bird");
        // A career that ends in 1985-86 has one season in the range.
        let mcadoo = g.groups.iter().find(|r| r.key == "Bob McAdoo").unwrap();
        assert_eq!(mcadoo.seasons, 1);
        // No dimension: one group of everything.
        let g = group(&GroupQuery::default(), &stats, &sz);
        assert_eq!(g.groups.len(), 1);
        assert_eq!(g.groups[0].key, "All seasons");
        assert_eq!(g.groups[0].seasons, 4);
        // Dynasty and format labels.
        let q = GroupQuery {
            dimensions: vec![Dimension::Dynasty, Dimension::Format],
            dynasty_titles: 1,
            dynasty_gap: 0,
            ..Default::default()
        };
        let g = group(&q, &stats, &sz);
        assert!(g.dynasties.iter().any(|d| d.label == "Team CLE dynasty"));
        assert!(g
            .groups
            .iter()
            .any(|r| r.key == "Team CLE dynasty · Best-of-seven throughout"));
        assert!(g
            .groups
            .iter()
            .any(|r| r.key == "ABA (not computed) · ABA playoffs"));
    }

    #[test]
    fn dynasty_names_and_shared_nicknames() {
        // A run is named after the team at its last title, not the franchise's first name.
        let moved = vec![
            stat(1949, "MNL", "LAL", 1700.0, true),
            stat(1950, "MNL", "LAL", 1700.0, true),
            stat(1972, "LAL", "LAL", 1700.0, true),
            stat(1973, "LAL", "LAL", 1700.0, true),
        ];
        let d = dynasties(&moved, 2, 0);
        assert_eq!(d.len(), 2);
        assert_eq!(
            (d[0].label.as_str(), d[0].name.as_str()),
            ("Mikan Lakers", "Team MNL")
        );
        assert_eq!(
            (d[1].label.as_str(), d[1].name.as_str()),
            ("Team LAL dynasty", "Team LAL")
        );
        // A nickname shared by two runs gives one group whose note covers both runs.
        let q = GroupQuery {
            dimensions: vec![Dimension::Dynasty],
            dynasty_titles: 2,
            dynasty_gap: 0,
            ..Default::default()
        };
        let mut bulls = Vec::new();
        for y in [1991, 1992, 1993, 1996, 1997, 1998] {
            bulls.push(stat(y, "CHI", "CHI", 1800.0, true));
        }
        let g = group(&q, &bulls, &sizes(&bulls));
        let row = g.groups.iter().find(|r| r.key == "Jordan Bulls").unwrap();
        assert_eq!(row.stats.titles, 6);
        assert_eq!(g.notes["dynasty:Jordan Bulls"].changes.len(), 6);
    }
}
