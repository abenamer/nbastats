//! The absences declared by hand for the career worth: the seasons a player sat out that
//! the stint rule of [`super::career`] cannot see, because the store has no rosters of past
//! seasons. [`DECLARED_ABSENCES`] is the whole list, and ADR 0021 has the criterion, the
//! players left out and the runs.
//!
//! A season of a team is on the list for a player when all of this holds:
//!
//! * One outage for an injury, an illness, or a retirement he came back from covers at
//!   least half of the team's games of that season.
//! * His last game before the outage and his first game after it were for that team.
//! * The team did not trade, waive or release him during the outage.
//! * A stint can receive the games: his stint with that team in that season, else in the
//!   season before, else in the season after.
//!
//! A suspension, a holdout, a season in another league and a coach's decision are not on the
//! list: the list is for a player the team would have used. The store cannot check a row
//! against a roster, so each row is a judgement about history, and its cause is given.

use serde::Serialize;

use super::ids::SeasonYear;

/// Team games a player sat out that the stint rule cannot see, declared by hand: every game
/// his team played in `season` is read as inside his stint of `stint_season` with that
/// team. The games he missed become absences of that stint, against the games he played in
/// it. A declaration is a judgement that he was that team's player the whole time and that
/// the team around him was near enough the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DeclaredAbsence {
    /// The player's key at the player source: a store keeps it across loads, and the
    /// database key is its own.
    pub source: &'static str,
    pub source_id: i64,
    /// His name, for the reader of the list. The store's name is the one a page shows.
    pub player: &'static str,
    pub team_code: &'static str,
    /// The stint that receives the games; without it the declaration adds nothing.
    pub stint_season: SeasonYear,
    /// The season whose team games are all read as inside that stint.
    pub season: SeasonYear,
    /// Why he was out, in a few words.
    pub cause: &'static str,
}

/// Which of his stints with the team receives the games of a declared season.
#[derive(Clone, Copy)]
enum Stint {
    /// The stint of the declared season: he played a part of it.
    Own,
    /// The stint of the season before: the declared season has no stint of his.
    Before,
    /// The stint of the season after: neither the season nor the one before has a stint.
    After,
}
use Stint::{After, Before, Own};

/// One row of the list. `season` is the end year of the declared season.
const fn out(
    player: &'static str,
    source_id: i64,
    team_code: &'static str,
    season: i32,
    stint: Stint,
    cause: &'static str,
) -> DeclaredAbsence {
    let stint_season = match stint {
        Own => season,
        Before => season - 1,
        After => season + 1,
    };
    DeclaredAbsence {
        source: "balldontlie",
        source_id,
        player,
        team_code,
        stint_season: SeasonYear::of(stint_season),
        season: SeasonYear::of(season),
        cause,
    }
}

/// Every declared absence, by player and season. Michael Jordan's two rows are those of
/// ADR 0020: he left the Bulls on 6 October 1993 and played again on 19 March 1995. Magic
/// Johnson's 1992-93 and 1993-94 are not here: no stint of his is beside them.
pub const DECLARED_ABSENCES: &[DeclaredAbsence] = &[
    out(
        "Adrian Dantley",
        3085,
        "UTA",
        1983,
        Own,
        "torn wrist ligaments",
    ),
    out("Al Horford", 219, "ATL", 2012, Own, "torn pectoral muscle"),
    out("Al Horford", 219, "ATL", 2014, Own, "torn pectoral muscle"),
    out(
        "Amar'e Stoudemire",
        1433,
        "PHO",
        2006,
        Before,
        "knee surgery",
    ),
    out(
        "Anfernee Hardaway",
        799,
        "PHO",
        2001,
        Before,
        "knee surgeries",
    ),
    out(
        "Anthony Peeler",
        744,
        "LAL",
        1994,
        Own,
        "stress fracture of the leg",
    ),
    out("Antonio McDyess", 965, "DEN", 2002, Before, "knee surgery"),
    out(
        "Bill Cartwright",
        2925,
        "NYK",
        1985,
        Before,
        "foot fractures",
    ),
    out(
        "Bill Cartwright",
        2925,
        "NYK",
        1986,
        After,
        "foot fractures",
    ),
    out("Blake Griffin", 189, "DET", 2020, Own, "knee surgery"),
    out("Brad Daugherty", 2860, "CLE", 1990, Own, "foot surgery"),
    out("Brook Lopez", 283, "BRK", 2014, Own, "broken foot"),
    out("Brook Lopez", 283, "MIL", 2022, Before, "back surgery"),
    out("Bryant Stith", 751, "DEN", 1998, Before, "foot surgery"),
    out(
        "Calvin Natt",
        2340,
        "DEN",
        1987,
        Before,
        "torn Achilles tendon",
    ),
    out("Carlos Boozer", 1410, "UTA", 2006, Own, "hamstring injury"),
    out("Channing Frye", 164, "PHO", 2013, Before, "enlarged heart"),
    out(
        "Chauncey Billups",
        1097,
        "LAC",
        2012,
        Own,
        "torn Achilles tendon",
    ),
    out("Chris Mills", 809, "GSW", 2000, Own, "ankle surgery"),
    out("Chris Mills", 809, "GSW", 2001, Own, "ankle surgery"),
    out("Chris Webber", 847, "SAC", 2004, Own, "knee surgery"),
    out("Chuck Person", 2881, "SAS", 1997, Before, "back surgery"),
    out("Damian Lillard", 278, "POR", 2022, Own, "abdominal surgery"),
    out(
        "Damon Stoudamire",
        960,
        "MEM",
        2006,
        Own,
        "torn patellar tendon",
    ),
    out(
        "Danilo Gallinari",
        167,
        "DEN",
        2014,
        Before,
        "torn knee ligament",
    ),
    out("Danny Granger", 1617, "IND", 2013, Before, "knee injury"),
    out(
        "Danny Manning",
        3006,
        "LAC",
        1989,
        Own,
        "torn knee ligament",
    ),
    out(
        "Danny Manning",
        3006,
        "PHO",
        1996,
        Own,
        "torn knee ligament",
    ),
    out(
        "Darrell Griffith",
        515,
        "UTA",
        1986,
        Before,
        "stress fracture of the foot",
    ),
    out(
        "David Robinson",
        2981,
        "SAS",
        1997,
        Own,
        "broken foot, after a back injury",
    ),
    out("Dee Brown", 2850, "BOS", 1992, Own, "knee surgery"),
    out(
        "Dejounte Murray",
        334,
        "SAS",
        2019,
        After,
        "torn knee ligament",
    ),
    out(
        "Dejounte Murray",
        334,
        "NOP",
        2026,
        Own,
        "torn Achilles tendon",
    ),
    out("Dennis Scott", 531, "ORL", 1992, Own, "knee surgery"),
    out(
        "Derek Fisher",
        1041,
        "LAL",
        2001,
        Own,
        "foot stress fracture",
    ),
    out(
        "Derrick Rose",
        401,
        "CHI",
        2013,
        Before,
        "torn knee ligament",
    ),
    out("Derrick Rose", 401, "CHI", 2014, Own, "torn meniscus"),
    out("Derrick Rose", 401, "NYK", 2022, Own, "ankle surgery"),
    out("Dillon Brooks", 66, "MEM", 2019, Before, "toe surgery"),
    out("Doc Rivers", 549, "NYK", 1994, Own, "torn knee ligament"),
    out(
        "Dorian Finney-Smith",
        158,
        "DAL",
        2018,
        After,
        "knee tendinitis",
    ),
    out(
        "Elton Brand",
        1247,
        "LAC",
        2008,
        Own,
        "torn Achilles tendon",
    ),
    out("Emeka Okafor", 1559, "CHO", 2006, Own, "ankle injury"),
    out("Eric Bledsoe", 51, "PHO", 2016, Own, "torn meniscus"),
    out("Eric Gordon", 178, "HOU", 2021, Own, "groin strain"),
    out(
        "Fred VanVleet",
        458,
        "HOU",
        2026,
        Before,
        "torn knee ligament",
    ),
    out(
        "Gilbert Arenas",
        1374,
        "WAS",
        2009,
        Before,
        "knee surgeries",
    ),
    out("Glen Rice", 508, "HOU", 2002, Own, "torn patellar tendon"),
    out("Gordon Hayward", 204, "BOS", 2018, After, "broken leg"),
    out("Grant Hill", 879, "ORL", 2001, After, "ankle injury"),
    out("Grant Hill", 879, "ORL", 2002, Own, "ankle injury"),
    out("Grant Hill", 879, "ORL", 2003, Own, "ankle injury"),
    out("Grant Hill", 879, "ORL", 2004, Before, "ankle injury"),
    out(
        "Jamal Crawford",
        111,
        "CHI",
        2002,
        After,
        "torn knee ligament",
    ),
    out("Jamal Mashburn", 826, "DAL", 1996, Own, "knee injury"),
    out("Jamal Mashburn", 826, "NOP", 2004, Own, "knee injury"),
    out(
        "Jamal Murray",
        335,
        "DEN",
        2022,
        Before,
        "torn knee ligament",
    ),
    out(
        "Jaren Jackson Jr.",
        231,
        "MEM",
        2021,
        Before,
        "torn meniscus",
    ),
    out("Jason Richardson", 1372, "PHI", 2013, Own, "knee surgery"),
    out(
        "Jason Richardson",
        1372,
        "PHI",
        2014,
        Before,
        "knee surgery",
    ),
    out(
        "Jayson Tatum",
        434,
        "BOS",
        2026,
        Own,
        "torn Achilles tendon",
    ),
    out("Jerry Stackhouse", 946, "WAS", 2004, Own, "knee surgery"),
    out(
        "Jrue Holiday",
        214,
        "NOP",
        2014,
        Own,
        "stress fracture of the leg",
    ),
    out("Julius Randle", 387, "LAL", 2015, After, "broken leg"),
    out("Jusuf Nurkic", 349, "POR", 2020, Own, "broken leg"),
    out(
        "Kawhi Leonard",
        274,
        "LAC",
        2022,
        Before,
        "torn knee ligament",
    ),
    out(
        "Kendrick Perkins",
        1513,
        "BOS",
        2011,
        Own,
        "torn knee ligament",
    ),
    out("Kevin Johnson", 501, "PHO", 1999, Before, "retirement"),
    out("Kevin Love", 285, "MIN", 2013, Own, "broken hand"),
    out("Kevin Willis", 579, "ATL", 1989, Before, "broken foot"),
    out("Khris Middleton", 315, "MIL", 2017, Own, "torn hamstring"),
    out("Kiki Vandeweghe", 2961, "NYK", 1990, Own, "back injury"),
    out(
        "Klay Thompson",
        443,
        "GSW",
        2020,
        Before,
        "torn knee ligament",
    ),
    out(
        "Klay Thompson",
        443,
        "GSW",
        2021,
        After,
        "torn Achilles tendon",
    ),
    out(
        "Kobe Bryant",
        1043,
        "LAL",
        2014,
        Own,
        "knee fracture, after a torn Achilles tendon",
    ),
    out("Kyle Lowry", 286, "MEM", 2007, After, "broken wrist"),
    out(
        "Kyrie Irving",
        228,
        "DAL",
        2026,
        Before,
        "torn knee ligament",
    ),
    out("LaPhonso Ellis", 752, "DEN", 1995, Before, "knee surgery"),
    out("LaPhonso Ellis", 752, "ATL", 1999, Own, "hernia surgery"),
    out("Lafayette Lever", 602, "DAL", 1991, After, "knee injury"),
    out("Lafayette Lever", 602, "DAL", 1992, Own, "knee injury"),
    out("Lafayette Lever", 602, "DAL", 1993, Before, "knee injury"),
    out("Lamar Odom", 1216, "LAC", 2002, Own, "wrist injury"),
    out(
        "Larry Bird",
        2851,
        "BOS",
        1989,
        Own,
        "surgery on both heels",
    ),
    out("Lou Williams", 481, "ATL", 2013, Own, "torn knee ligament"),
    out("Magic Johnson", 2994, "LAL", 1992, Before, "retirement"),
    out("Magic Johnson", 2994, "LAL", 1995, After, "retirement"),
    out("Magic Johnson", 2994, "LAL", 1996, Own, "retirement"),
    out("Marcus Smart", 420, "MEM", 2024, Own, "finger injury"),
    out("Mark Price", 529, "CLE", 1991, Own, "torn knee ligament"),
    out("Matt Harpring", 1159, "UTA", 2004, Own, "knee surgery"),
    out("Michael Jordan", 2931, "CHI", 1994, Before, "retirement"),
    out("Michael Jordan", 2931, "CHI", 1995, Own, "retirement"),
    out(
        "Michael Redd",
        1287,
        "MIL",
        2010,
        Own,
        "torn knee ligaments",
    ),
    out(
        "Michael Redd",
        1287,
        "MIL",
        2011,
        Before,
        "torn knee ligaments",
    ),
    out("Mike Conley", 104, "MEM", 2018, Own, "heel surgery"),
    out("Mike Dunleavy", 1429, "CHI", 2016, Before, "back surgery"),
    out("Monta Ellis", 1635, "GSW", 2009, Own, "torn ankle ligament"),
    out("Moses Malone", 593, "MIL", 1993, Before, "back surgery"),
    out("Mychal Thompson", 2995, "POR", 1980, Before, "broken leg"),
    out("Nene", 342, "DEN", 2006, After, "torn knee ligament"),
    out("Nicolas Batum", 33, "POR", 2010, Own, "shoulder surgery"),
    out("Norm Nixon", 2249, "LAC", 1987, Before, "torn knee tendon"),
    out(
        "Norm Nixon",
        2249,
        "LAC",
        1988,
        After,
        "torn Achilles tendon",
    ),
    out("Patrick Beverley", 45, "LAC", 2018, Own, "knee surgery"),
    out("Patrick Ewing", 2964, "NYK", 1998, Own, "wrist injury"),
    out("Paul George", 172, "IND", 2015, Before, "broken leg"),
    out(
        "Raef LaFrentz",
        1193,
        "DEN",
        1999,
        Own,
        "torn knee ligament",
    ),
    out("Raef LaFrentz", 1193, "BOS", 2004, After, "knee surgery"),
    out(
        "Randy Foye",
        1677,
        "MIN",
        2008,
        Own,
        "stress reaction in the kneecap",
    ),
    out("Rick Fox", 670, "LAL", 2004, Before, "foot surgery"),
    out(
        "Robert Covington",
        108,
        "MIN",
        2019,
        Own,
        "knee bone bruise",
    ),
    out("Robert Reid", 3088, "HOU", 1983, Before, "retirement"),
    out("Ron Harper", 3070, "LAC", 1990, Own, "torn knee ligament"),
    out("Ron Harper", 3070, "LAC", 1991, Own, "torn knee ligament"),
    out(
        "Ryan Anderson",
        13,
        "NOP",
        2014,
        Own,
        "herniated disc in the neck",
    ),
    out("Samuel Dalembert", 1393, "PHI", 2003, After, "knee surgery"),
    out(
        "Sean Elliott",
        2984,
        "SAS",
        1998,
        Own,
        "quadriceps tendinitis",
    ),
    out(
        "Sean Elliott",
        2984,
        "SAS",
        2000,
        Before,
        "kidney transplant",
    ),
    out(
        "Terry Cummings",
        2987,
        "SAS",
        1993,
        Before,
        "torn knee ligament",
    ),
    out("Terry Porter", 2873, "POR", 1995, Before, "ankle injury"),
    out("Theo Ratliff", 964, "ATL", 2002, After, "hip injury"),
    out(
        "Tim Hardaway",
        576,
        "GSW",
        1994,
        Before,
        "torn knee ligament",
    ),
    out("Trevor Ariza", 20, "LAL", 2008, After, "broken foot"),
    out("Udonis Haslem", 203, "MIA", 2011, Own, "torn foot ligament"),
    out(
        "Victor Oladipo",
        357,
        "IND",
        2020,
        Own,
        "ruptured quadriceps tendon",
    ),
    out(
        "Victor Oladipo",
        357,
        "MIA",
        2022,
        After,
        "quadriceps tendon surgery",
    ),
    out(
        "Voshon Lenard",
        993,
        "MIA",
        1999,
        Before,
        "stress fracture of the leg",
    ),
    out(
        "Voshon Lenard",
        993,
        "DEN",
        2005,
        Before,
        "torn Achilles tendon",
    ),
    out("Wally Szczerbiak", 1251, "MIN", 2004, Before, "foot injury"),
    out("Walter Davis", 619, "PHO", 1985, Own, "torn knee ligaments"),
    out("Wilson Chandler", 95, "DEN", 2016, Before, "hip surgery"),
    out("Yao Ming", 1445, "HOU", 2010, Before, "foot fracture"),
    out("Zydrunas Ilgauskas", 1107, "CLE", 1999, Own, "foot injury"),
    out(
        "Zydrunas Ilgauskas",
        1107,
        "CLE",
        2000,
        Before,
        "broken foot",
    ),
    out("Zydrunas Ilgauskas", 1107, "CLE", 2001, Own, "foot injury"),
];

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::super::career::CellRule;
    use super::*;

    fn row(player: &str, season: i32) -> Option<&'static DeclaredAbsence> {
        DECLARED_ABSENCES
            .iter()
            .find(|d| d.player == player && d.season == SeasonYear::of(season))
    }

    #[test]
    fn each_row_can_be_read_and_no_season_is_declared_twice() {
        let first = CellRule::default().first_season;
        let mut seen = BTreeSet::new();
        for d in DECLARED_ABSENCES {
            assert_eq!(d.source, "balldontlie");
            assert!(d.season >= first && d.stint_season >= first, "{d:?}");
            // The receiving stint is of the season or of one beside it: the constant for
            // the change of a team between seasons is measured for seasons in a row.
            let apart = d.season.end_year() - d.stint_season.end_year();
            assert!((-1..=1).contains(&apart), "{d:?}");
            assert!(!d.cause.is_empty() && d.cause.len() <= 48, "{d:?}");
            assert!(seen.insert((d.source_id, d.team_code, d.season)), "{d:?}");
        }
        assert_eq!(DECLARED_ABSENCES.len(), 136);
    }

    #[test]
    fn a_player_has_one_key_and_one_name_and_the_list_is_in_order() {
        let mut names: BTreeMap<i64, &str> = BTreeMap::new();
        let mut keys: BTreeMap<&str, i64> = BTreeMap::new();
        for d in DECLARED_ABSENCES {
            assert_eq!(*names.entry(d.source_id).or_insert(d.player), d.player);
            assert_eq!(*keys.entry(d.player).or_insert(d.source_id), d.source_id);
        }
        assert_eq!(names.len(), 107);
        assert!(DECLARED_ABSENCES
            .windows(2)
            .all(|w| (w[0].player, w[0].season) < (w[1].player, w[1].season)));
    }

    #[test]
    fn the_rows_of_the_best_known_absences_say_which_stint_reads_them() {
        let stint = |player, season| row(player, season).map(|d| d.stint_season.end_year());
        // Michael Jordan's retirement, as ADR 0020 declared it.
        assert_eq!(stint("Michael Jordan", 1994), Some(1993));
        assert_eq!(stint("Michael Jordan", 1995), Some(1995));
        assert_eq!(row("Michael Jordan", 1994).map(|d| d.source_id), Some(2931));
        assert_eq!(
            row("Michael Jordan", 1994).map(|d| d.team_code),
            Some("CHI")
        );
        // Klay Thompson's two seasons: the knee against the season before, the Achilles
        // tendon against the season of his return.
        assert_eq!(stint("Klay Thompson", 2020), Some(2019));
        assert_eq!(stint("Klay Thompson", 2021), Some(2022));
        // Magic Johnson: the season he retired before, the season before his return, and
        // the games of 1995-96 before it. The two seasons between have no stint beside them.
        assert_eq!(stint("Magic Johnson", 1992), Some(1991));
        assert_eq!(stint("Magic Johnson", 1995), Some(1996));
        assert_eq!(stint("Magic Johnson", 1996), Some(1996));
        assert_eq!(stint("Magic Johnson", 1993), None);
        assert_eq!(stint("Magic Johnson", 1994), None);
        assert_eq!(
            row("Magic Johnson", 1992).map(|d| d.cause),
            Some("retirement")
        );
    }
}
