//! Static reference data: franchise lineage for every team code that appears in the
//! FiveThirtyEight / Neil Paine Elo datasets, and conference/division alignment by season.
//!
//! Franchise identity follows FiveThirtyEight's `fran_id` convention: a franchise is the
//! organisation, followed through relocations and renamings. Notably the 1989–2002 Charlotte
//! Hornets (CHH) are part of the New Orleans Pelicans lineage, and the 2005+ Charlotte
//! Bobcats/Hornets (CHA/CHO) are a separate franchise.

use super::ids::SeasonYear;
use super::models::Conference;

/// Sentinel `last_season` for a team code that is still in use.
pub const CURRENT: i32 = 9999;

/// (team code, franchise key, franchise display name, team display name, first season, last season)
/// Seasons are end-years (2004 == 2003-04). `last_season` of [`CURRENT`] means "current".
pub const TEAM_LINEAGE: &[(&str, &str, &str, &str, i32, i32)] = &[
    // --- 30 active franchises (franchise key == current code) ---
    ("ATL", "ATL", "Atlanta Hawks", "Atlanta Hawks", 1969, 9999),
    ("STL", "ATL", "Atlanta Hawks", "St. Louis Hawks", 1956, 1968),
    ("MLH", "ATL", "Atlanta Hawks", "Milwaukee Hawks", 1952, 1955),
    (
        "TRI",
        "ATL",
        "Atlanta Hawks",
        "Tri-Cities Blackhawks",
        1950,
        1951,
    ),
    ("BOS", "BOS", "Boston Celtics", "Boston Celtics", 1947, 9999),
    ("BRK", "BRK", "Brooklyn Nets", "Brooklyn Nets", 2013, 9999),
    ("NJN", "BRK", "Brooklyn Nets", "New Jersey Nets", 1978, 2012),
    ("NYN", "BRK", "Brooklyn Nets", "New York Nets", 1977, 1977),
    (
        "NYA",
        "BRK",
        "Brooklyn Nets",
        "New York Nets (ABA)",
        1969,
        1976,
    ),
    (
        "NJA",
        "BRK",
        "Brooklyn Nets",
        "New Jersey Americans (ABA)",
        1968,
        1968,
    ),
    (
        "CHO",
        "CHO",
        "Charlotte Hornets",
        "Charlotte Hornets",
        2005,
        9999,
    ),
    (
        "CHA",
        "CHO",
        "Charlotte Hornets",
        "Charlotte Bobcats",
        2005,
        2014,
    ),
    ("CHI", "CHI", "Chicago Bulls", "Chicago Bulls", 1967, 9999),
    (
        "CLE",
        "CLE",
        "Cleveland Cavaliers",
        "Cleveland Cavaliers",
        1971,
        9999,
    ),
    (
        "DAL",
        "DAL",
        "Dallas Mavericks",
        "Dallas Mavericks",
        1981,
        9999,
    ),
    ("DEN", "DEN", "Denver Nuggets", "Denver Nuggets", 1977, 9999),
    (
        "DNA",
        "DEN",
        "Denver Nuggets",
        "Denver Nuggets (ABA)",
        1975,
        1976,
    ),
    (
        "DNR",
        "DEN",
        "Denver Nuggets",
        "Denver Rockets (ABA)",
        1968,
        1974,
    ),
    (
        "DET",
        "DET",
        "Detroit Pistons",
        "Detroit Pistons",
        1958,
        9999,
    ),
    (
        "FTW",
        "DET",
        "Detroit Pistons",
        "Fort Wayne Pistons",
        1949,
        1957,
    ),
    (
        "GSW",
        "GSW",
        "Golden State Warriors",
        "Golden State Warriors",
        1972,
        9999,
    ),
    (
        "SFW",
        "GSW",
        "Golden State Warriors",
        "San Francisco Warriors",
        1963,
        1971,
    ),
    (
        "PHW",
        "GSW",
        "Golden State Warriors",
        "Philadelphia Warriors",
        1947,
        1962,
    ),
    (
        "HOU",
        "HOU",
        "Houston Rockets",
        "Houston Rockets",
        1972,
        9999,
    ),
    (
        "SDR",
        "HOU",
        "Houston Rockets",
        "San Diego Rockets",
        1968,
        1971,
    ),
    ("IND", "IND", "Indiana Pacers", "Indiana Pacers", 1977, 9999),
    (
        "INA",
        "IND",
        "Indiana Pacers",
        "Indiana Pacers (ABA)",
        1968,
        1976,
    ),
    (
        "LAC",
        "LAC",
        "Los Angeles Clippers",
        "Los Angeles Clippers",
        1985,
        9999,
    ),
    (
        "SDC",
        "LAC",
        "Los Angeles Clippers",
        "San Diego Clippers",
        1979,
        1984,
    ),
    (
        "BUF",
        "LAC",
        "Los Angeles Clippers",
        "Buffalo Braves",
        1971,
        1978,
    ),
    (
        "LAL",
        "LAL",
        "Los Angeles Lakers",
        "Los Angeles Lakers",
        1961,
        9999,
    ),
    (
        "MNL",
        "LAL",
        "Los Angeles Lakers",
        "Minneapolis Lakers",
        1949,
        1960,
    ),
    (
        "MEM",
        "MEM",
        "Memphis Grizzlies",
        "Memphis Grizzlies",
        2002,
        9999,
    ),
    (
        "VAN",
        "MEM",
        "Memphis Grizzlies",
        "Vancouver Grizzlies",
        1996,
        2001,
    ),
    ("MIA", "MIA", "Miami Heat", "Miami Heat", 1989, 9999),
    (
        "MIL",
        "MIL",
        "Milwaukee Bucks",
        "Milwaukee Bucks",
        1969,
        9999,
    ),
    (
        "MIN",
        "MIN",
        "Minnesota Timberwolves",
        "Minnesota Timberwolves",
        1990,
        9999,
    ),
    (
        "NOP",
        "NOP",
        "New Orleans Pelicans",
        "New Orleans Pelicans",
        2003,
        9999,
    ),
    (
        "NOH",
        "NOP",
        "New Orleans Pelicans",
        "New Orleans Hornets",
        2003,
        2013,
    ),
    (
        "NOK",
        "NOP",
        "New Orleans Pelicans",
        "New Orleans/Oklahoma City Hornets",
        2006,
        2007,
    ),
    (
        "CHH",
        "NOP",
        "New Orleans Pelicans",
        "Charlotte Hornets (1989-2002)",
        1989,
        2002,
    ),
    (
        "NYK",
        "NYK",
        "New York Knicks",
        "New York Knicks",
        1947,
        9999,
    ),
    (
        "OKC",
        "OKC",
        "Oklahoma City Thunder",
        "Oklahoma City Thunder",
        2009,
        9999,
    ),
    (
        "SEA",
        "OKC",
        "Oklahoma City Thunder",
        "Seattle SuperSonics",
        1968,
        2008,
    ),
    ("ORL", "ORL", "Orlando Magic", "Orlando Magic", 1990, 9999),
    (
        "PHI",
        "PHI",
        "Philadelphia 76ers",
        "Philadelphia 76ers",
        1964,
        9999,
    ),
    (
        "SYR",
        "PHI",
        "Philadelphia 76ers",
        "Syracuse Nationals",
        1950,
        1963,
    ),
    ("PHO", "PHO", "Phoenix Suns", "Phoenix Suns", 1969, 9999),
    (
        "POR",
        "POR",
        "Portland Trail Blazers",
        "Portland Trail Blazers",
        1971,
        9999,
    ),
    (
        "SAC",
        "SAC",
        "Sacramento Kings",
        "Sacramento Kings",
        1986,
        9999,
    ),
    (
        "KCK",
        "SAC",
        "Sacramento Kings",
        "Kansas City Kings",
        1976,
        1985,
    ),
    (
        "KCO",
        "SAC",
        "Sacramento Kings",
        "Kansas City-Omaha Kings",
        1973,
        1975,
    ),
    (
        "CIN",
        "SAC",
        "Sacramento Kings",
        "Cincinnati Royals",
        1958,
        1972,
    ),
    (
        "ROC",
        "SAC",
        "Sacramento Kings",
        "Rochester Royals",
        1949,
        1957,
    ),
    (
        "SAS",
        "SAS",
        "San Antonio Spurs",
        "San Antonio Spurs",
        1977,
        9999,
    ),
    (
        "SAA",
        "SAS",
        "San Antonio Spurs",
        "San Antonio Spurs (ABA)",
        1974,
        1976,
    ),
    (
        "TEX",
        "SAS",
        "San Antonio Spurs",
        "Texas Chaparrals (ABA)",
        1971,
        1971,
    ),
    (
        "DLC",
        "SAS",
        "San Antonio Spurs",
        "Dallas Chaparrals (ABA)",
        1968,
        1973,
    ),
    (
        "TOR",
        "TOR",
        "Toronto Raptors",
        "Toronto Raptors",
        1996,
        9999,
    ),
    ("UTA", "UTA", "Utah Jazz", "Utah Jazz", 1980, 9999),
    ("NOJ", "UTA", "Utah Jazz", "New Orleans Jazz", 1975, 1979),
    (
        "WAS",
        "WAS",
        "Washington Wizards",
        "Washington Wizards",
        1998,
        9999,
    ),
    (
        "WSB",
        "WAS",
        "Washington Wizards",
        "Washington Bullets",
        1975,
        1997,
    ),
    (
        "CAP",
        "WAS",
        "Washington Wizards",
        "Capital Bullets",
        1974,
        1974,
    ),
    (
        "BAL",
        "WAS",
        "Washington Wizards",
        "Baltimore Bullets",
        1964,
        1973,
    ),
    (
        "CHZ",
        "WAS",
        "Washington Wizards",
        "Chicago Zephyrs",
        1963,
        1963,
    ),
    (
        "CHP",
        "WAS",
        "Washington Wizards",
        "Chicago Packers",
        1962,
        1962,
    ),
    // --- defunct franchises (franchise key == last code) ---
    (
        "BLB",
        "BLB",
        "Baltimore Bullets (1947-54)",
        "Baltimore Bullets (original)",
        1948,
        1954,
    ),
    (
        "STB",
        "STB",
        "St. Louis Bombers",
        "St. Louis Bombers",
        1947,
        1950,
    ),
    (
        "WSC",
        "WSC",
        "Washington Capitols",
        "Washington Capitols",
        1947,
        1951,
    ),
    (
        "KEN",
        "KEN",
        "Kentucky Colonels (ABA)",
        "Kentucky Colonels",
        1968,
        1976,
    ),
    (
        "PTC",
        "PTC",
        "Pittsburgh Condors (ABA)",
        "Pittsburgh Condors",
        1971,
        1972,
    ),
    (
        "PTP",
        "PTC",
        "Pittsburgh Condors (ABA)",
        "Pittsburgh Pipers",
        1968,
        1970,
    ),
    (
        "MNP",
        "PTC",
        "Pittsburgh Condors (ABA)",
        "Minnesota Pipers",
        1969,
        1969,
    ),
    (
        "DNN",
        "DNN",
        "Denver Nuggets (1949-50)",
        "Denver Nuggets (original)",
        1950,
        1950,
    ),
    (
        "DTF",
        "DTF",
        "Detroit Falcons",
        "Detroit Falcons",
        1947,
        1947,
    ),
    (
        "FLO",
        "FLO",
        "Floridians (ABA)",
        "The Floridians",
        1971,
        1972,
    ),
    (
        "MMF",
        "FLO",
        "Floridians (ABA)",
        "Miami Floridians",
        1969,
        1970,
    ),
    (
        "MNM",
        "FLO",
        "Floridians (ABA)",
        "Minnesota Muskies",
        1968,
        1968,
    ),
    (
        "TRH",
        "TRH",
        "Toronto Huskies",
        "Toronto Huskies",
        1947,
        1947,
    ),
    (
        "PIT",
        "PIT",
        "Pittsburgh Ironmen",
        "Pittsburgh Ironmen",
        1947,
        1947,
    ),
    (
        "INJ",
        "INJ",
        "Indianapolis Jets",
        "Indianapolis Jets",
        1949,
        1949,
    ),
    (
        "INO",
        "INO",
        "Indianapolis Olympians",
        "Indianapolis Olympians",
        1950,
        1953,
    ),
    (
        "AND",
        "AND",
        "Anderson Packers",
        "Anderson Packers",
        1950,
        1950,
    ),
    (
        "CLR",
        "CLR",
        "Cleveland Rebels",
        "Cleveland Rebels",
        1947,
        1947,
    ),
    (
        "SHE",
        "SHE",
        "Sheboygan Red Skins",
        "Sheboygan Red Skins",
        1950,
        1950,
    ),
    (
        "SDS",
        "SDS",
        "San Diego Sails (ABA)",
        "San Diego Sails",
        1976,
        1976,
    ),
    (
        "SDA",
        "SDS",
        "San Diego Sails (ABA)",
        "San Diego Conquistadors",
        1973,
        1975,
    ),
    (
        "MMS",
        "MMS",
        "Memphis Sounds (ABA)",
        "Memphis Sounds",
        1975,
        1975,
    ),
    (
        "MMT",
        "MMS",
        "Memphis Sounds (ABA)",
        "Memphis Tams",
        1973,
        1974,
    ),
    (
        "MMP",
        "MMS",
        "Memphis Sounds (ABA)",
        "Memphis Pros",
        1971,
        1972,
    ),
    (
        "NOB",
        "MMS",
        "Memphis Sounds (ABA)",
        "New Orleans Buccaneers",
        1968,
        1970,
    ),
    (
        "SSL",
        "SSL",
        "Spirits of St. Louis (ABA)",
        "Spirits of St. Louis",
        1975,
        1976,
    ),
    (
        "CAR",
        "SSL",
        "Spirits of St. Louis (ABA)",
        "Carolina Cougars",
        1970,
        1974,
    ),
    (
        "HSM",
        "SSL",
        "Spirits of St. Louis (ABA)",
        "Houston Mavericks",
        1968,
        1969,
    ),
    (
        "VIR",
        "VIR",
        "Virginia Squires (ABA)",
        "Virginia Squires",
        1971,
        1976,
    ),
    (
        "WSA",
        "VIR",
        "Virginia Squires (ABA)",
        "Washington Caps",
        1970,
        1970,
    ),
    (
        "OAK",
        "VIR",
        "Virginia Squires (ABA)",
        "Oakland Oaks",
        1968,
        1969,
    ),
    ("CHS", "CHS", "Chicago Stags", "Chicago Stags", 1947, 1950),
    ("UTS", "UTS", "Utah Stars (ABA)", "Utah Stars", 1971, 1976),
    (
        "LAS",
        "UTS",
        "Utah Stars (ABA)",
        "Los Angeles Stars",
        1969,
        1970,
    ),
    (
        "ANA",
        "UTS",
        "Utah Stars (ABA)",
        "Anaheim Amigos",
        1968,
        1968,
    ),
    (
        "PRO",
        "PRO",
        "Providence Steamrollers",
        "Providence Steamrollers",
        1947,
        1949,
    ),
    ("WAT", "WAT", "Waterloo Hawks", "Waterloo Hawks", 1950, 1950),
];

/// The 30 franchises active today, in alphabetical order of key.
pub fn active_franchise_keys() -> Vec<&'static str> {
    let mut v: Vec<&str> = TEAM_LINEAGE
        .iter()
        .filter(|t| t.5 == CURRENT)
        .map(|t| t.1)
        .collect();
    v.sort();
    v.dedup();
    v
}

pub fn lookup_team(
    code: &str,
) -> Option<&'static (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    i32,
    i32,
)> {
    TEAM_LINEAGE.iter().find(|t| t.0 == code)
}

/// Division membership of the BAA and NBA from 1946-47 to 2001-02, as (first end year, last
/// end year, conference, division, team codes of those seasons). Curated from the league's
/// standings; checked against the game log so that every team of every season is listed once.
/// Before 1970-71 the league had two divisions (three in 1949-50) and no conferences; the
/// Eastern Division is filed under the East, the Western and the 1949-50 Central under the
/// West. ABA teams have no alignment data.
pub const HISTORICAL_ALIGNMENT: &[(i32, i32, Conference, &str, &[&str])] = &[
    (
        1947,
        1947,
        Conference::East,
        "Eastern",
        &["WSC", "PHW", "NYK", "PRO", "TRH", "BOS"],
    ),
    (
        1947,
        1947,
        Conference::West,
        "Western",
        &["CHS", "STB", "CLR", "DTF", "PIT"],
    ),
    (
        1948,
        1948,
        Conference::East,
        "Eastern",
        &["PHW", "NYK", "BOS", "PRO"],
    ),
    (
        1948,
        1948,
        Conference::West,
        "Western",
        &["STB", "BLB", "CHS", "WSC"],
    ),
    (
        1949,
        1949,
        Conference::East,
        "Eastern",
        &["WSC", "NYK", "BLB", "PHW", "BOS", "PRO"],
    ),
    (
        1949,
        1949,
        Conference::West,
        "Western",
        &["ROC", "MNL", "CHS", "STB", "FTW", "INJ"],
    ),
    (
        1950,
        1950,
        Conference::East,
        "Eastern",
        &["SYR", "NYK", "WSC", "PHW", "BLB", "BOS"],
    ),
    (
        1950,
        1950,
        Conference::West,
        "Central",
        &["MNL", "ROC", "FTW", "CHS", "STB"],
    ),
    (
        1950,
        1950,
        Conference::West,
        "Western",
        &["INO", "AND", "TRI", "SHE", "WAT", "DNN"],
    ),
    (
        1951,
        1951,
        Conference::East,
        "Eastern",
        &["PHW", "BOS", "NYK", "SYR", "BLB", "WSC"],
    ),
    (
        1951,
        1951,
        Conference::West,
        "Western",
        &["MNL", "ROC", "FTW", "INO", "TRI"],
    ),
    (
        1952,
        1953,
        Conference::East,
        "Eastern",
        &["SYR", "BOS", "NYK", "PHW", "BLB"],
    ),
    (
        1952,
        1953,
        Conference::West,
        "Western",
        &["ROC", "MNL", "INO", "FTW", "MLH"],
    ),
    (
        1954,
        1954,
        Conference::East,
        "Eastern",
        &["NYK", "BOS", "SYR", "PHW", "BLB"],
    ),
    (
        1954,
        1954,
        Conference::West,
        "Western",
        &["MNL", "ROC", "FTW", "MLH"],
    ),
    (
        1955,
        1955,
        Conference::East,
        "Eastern",
        &["SYR", "NYK", "BOS", "PHW"],
    ),
    (
        1955,
        1955,
        Conference::West,
        "Western",
        &["FTW", "MNL", "ROC", "MLH"],
    ),
    (
        1956,
        1957,
        Conference::East,
        "Eastern",
        &["PHW", "BOS", "SYR", "NYK"],
    ),
    (
        1956,
        1957,
        Conference::West,
        "Western",
        &["FTW", "MNL", "STL", "ROC"],
    ),
    (
        1958,
        1960,
        Conference::East,
        "Eastern",
        &["BOS", "SYR", "PHW", "NYK"],
    ),
    (
        1958,
        1960,
        Conference::West,
        "Western",
        &["STL", "DET", "CIN", "MNL"],
    ),
    (
        1961,
        1961,
        Conference::East,
        "Eastern",
        &["BOS", "PHW", "SYR", "NYK"],
    ),
    (
        1961,
        1961,
        Conference::West,
        "Western",
        &["STL", "LAL", "DET", "CIN"],
    ),
    (
        1962,
        1962,
        Conference::East,
        "Eastern",
        &["BOS", "PHW", "SYR", "NYK"],
    ),
    (
        1962,
        1962,
        Conference::West,
        "Western",
        &["LAL", "CIN", "DET", "STL", "CHP"],
    ),
    (
        1963,
        1963,
        Conference::East,
        "Eastern",
        &["BOS", "SYR", "NYK", "CIN"],
    ),
    (
        1963,
        1963,
        Conference::West,
        "Western",
        &["LAL", "STL", "DET", "SFW", "CHZ"],
    ),
    (
        1964,
        1966,
        Conference::East,
        "Eastern",
        &["BOS", "CIN", "PHI", "NYK"],
    ),
    (
        1964,
        1966,
        Conference::West,
        "Western",
        &["SFW", "STL", "LAL", "BAL", "DET"],
    ),
    (
        1967,
        1967,
        Conference::East,
        "Eastern",
        &["PHI", "BOS", "CIN", "NYK", "BAL"],
    ),
    (
        1967,
        1967,
        Conference::West,
        "Western",
        &["SFW", "STL", "LAL", "CHI", "DET"],
    ),
    (
        1968,
        1968,
        Conference::East,
        "Eastern",
        &["PHI", "BOS", "NYK", "DET", "CIN", "BAL"],
    ),
    (
        1968,
        1968,
        Conference::West,
        "Western",
        &["STL", "LAL", "SFW", "CHI", "SEA", "SDR"],
    ),
    (
        1969,
        1970,
        Conference::East,
        "Eastern",
        &["BAL", "PHI", "NYK", "BOS", "CIN", "DET", "MIL"],
    ),
    (
        1969,
        1970,
        Conference::West,
        "Western",
        &["LAL", "ATL", "SFW", "SDR", "CHI", "SEA", "PHO"],
    ),
    (
        1971,
        1971,
        Conference::East,
        "Atlantic",
        &["NYK", "PHI", "BOS", "BUF"],
    ),
    (
        1971,
        1971,
        Conference::East,
        "Central",
        &["BAL", "ATL", "CIN", "CLE"],
    ),
    (
        1971,
        1971,
        Conference::West,
        "Midwest",
        &["MIL", "CHI", "PHO", "DET"],
    ),
    (
        1971,
        1971,
        Conference::West,
        "Pacific",
        &["LAL", "SFW", "SDR", "SEA", "POR"],
    ),
    (
        1972,
        1972,
        Conference::East,
        "Atlantic",
        &["BOS", "NYK", "PHI", "BUF"],
    ),
    (
        1972,
        1972,
        Conference::East,
        "Central",
        &["BAL", "ATL", "CIN", "CLE"],
    ),
    (
        1972,
        1972,
        Conference::West,
        "Midwest",
        &["MIL", "CHI", "PHO", "DET"],
    ),
    (
        1972,
        1972,
        Conference::West,
        "Pacific",
        &["LAL", "GSW", "SEA", "HOU", "POR"],
    ),
    (
        1973,
        1973,
        Conference::East,
        "Atlantic",
        &["BOS", "NYK", "BUF", "PHI"],
    ),
    (
        1973,
        1973,
        Conference::East,
        "Central",
        &["BAL", "ATL", "HOU", "CLE"],
    ),
    (
        1973,
        1973,
        Conference::West,
        "Midwest",
        &["MIL", "CHI", "DET", "KCO"],
    ),
    (
        1973,
        1973,
        Conference::West,
        "Pacific",
        &["LAL", "GSW", "PHO", "SEA", "POR"],
    ),
    (
        1974,
        1974,
        Conference::East,
        "Atlantic",
        &["BOS", "NYK", "BUF", "PHI"],
    ),
    (
        1974,
        1974,
        Conference::East,
        "Central",
        &["CAP", "ATL", "HOU", "CLE"],
    ),
    (
        1974,
        1974,
        Conference::West,
        "Midwest",
        &["MIL", "CHI", "DET", "KCO"],
    ),
    (
        1974,
        1974,
        Conference::West,
        "Pacific",
        &["LAL", "GSW", "SEA", "PHO", "POR"],
    ),
    (
        1975,
        1975,
        Conference::East,
        "Atlantic",
        &["BOS", "BUF", "NYK", "PHI"],
    ),
    (
        1975,
        1975,
        Conference::East,
        "Central",
        &["WSB", "HOU", "CLE", "ATL", "NOJ"],
    ),
    (
        1975,
        1975,
        Conference::West,
        "Midwest",
        &["CHI", "KCO", "DET", "MIL"],
    ),
    (
        1975,
        1975,
        Conference::West,
        "Pacific",
        &["GSW", "SEA", "POR", "PHO", "LAL"],
    ),
    (
        1976,
        1976,
        Conference::East,
        "Atlantic",
        &["BOS", "BUF", "PHI", "NYK"],
    ),
    (
        1976,
        1976,
        Conference::East,
        "Central",
        &["CLE", "WSB", "HOU", "NOJ", "ATL"],
    ),
    (
        1976,
        1976,
        Conference::West,
        "Midwest",
        &["MIL", "DET", "KCK", "CHI"],
    ),
    (
        1976,
        1976,
        Conference::West,
        "Pacific",
        &["GSW", "SEA", "PHO", "LAL", "POR"],
    ),
    (
        1977,
        1977,
        Conference::East,
        "Atlantic",
        &["PHI", "BOS", "NYK", "BUF", "NYN"],
    ),
    (
        1977,
        1977,
        Conference::East,
        "Central",
        &["HOU", "WSB", "SAS", "CLE", "NOJ", "ATL"],
    ),
    (
        1977,
        1977,
        Conference::West,
        "Midwest",
        &["DEN", "DET", "CHI", "KCK", "IND", "MIL"],
    ),
    (
        1977,
        1977,
        Conference::West,
        "Pacific",
        &["LAL", "POR", "GSW", "SEA", "PHO"],
    ),
    (
        1978,
        1978,
        Conference::East,
        "Atlantic",
        &["PHI", "NYK", "BOS", "BUF", "NJN"],
    ),
    (
        1978,
        1978,
        Conference::East,
        "Central",
        &["SAS", "WSB", "CLE", "ATL", "NOJ", "HOU"],
    ),
    (
        1978,
        1978,
        Conference::West,
        "Midwest",
        &["DEN", "MIL", "CHI", "DET", "IND", "KCK"],
    ),
    (
        1978,
        1978,
        Conference::West,
        "Pacific",
        &["POR", "PHO", "SEA", "LAL", "GSW"],
    ),
    (
        1979,
        1979,
        Conference::East,
        "Atlantic",
        &["WSB", "PHI", "NJN", "NYK", "BOS"],
    ),
    (
        1979,
        1979,
        Conference::East,
        "Central",
        &["SAS", "HOU", "ATL", "CLE", "DET", "NOJ"],
    ),
    (
        1979,
        1979,
        Conference::West,
        "Midwest",
        &["KCK", "DEN", "IND", "MIL", "CHI"],
    ),
    (
        1979,
        1979,
        Conference::West,
        "Pacific",
        &["SEA", "PHO", "LAL", "POR", "SDC", "GSW"],
    ),
    (
        1980,
        1980,
        Conference::East,
        "Atlantic",
        &["BOS", "PHI", "WSB", "NYK", "NJN"],
    ),
    (
        1980,
        1980,
        Conference::East,
        "Central",
        &["ATL", "HOU", "SAS", "IND", "CLE", "DET"],
    ),
    (
        1980,
        1980,
        Conference::West,
        "Midwest",
        &["MIL", "KCK", "DEN", "CHI", "UTA"],
    ),
    (
        1980,
        1980,
        Conference::West,
        "Pacific",
        &["LAL", "SEA", "PHO", "POR", "SDC", "GSW"],
    ),
    (
        1981,
        1984,
        Conference::East,
        "Atlantic",
        &["BOS", "PHI", "NYK", "WSB", "NJN"],
    ),
    (
        1981,
        1984,
        Conference::East,
        "Central",
        &["MIL", "CHI", "IND", "ATL", "CLE", "DET"],
    ),
    (
        1981,
        1984,
        Conference::West,
        "Midwest",
        &["SAS", "KCK", "HOU", "DEN", "UTA", "DAL"],
    ),
    (
        1981,
        1984,
        Conference::West,
        "Pacific",
        &["PHO", "LAL", "POR", "GSW", "SDC", "SEA"],
    ),
    (
        1985,
        1985,
        Conference::East,
        "Atlantic",
        &["BOS", "PHI", "NYK", "WSB", "NJN"],
    ),
    (
        1985,
        1985,
        Conference::East,
        "Central",
        &["MIL", "CHI", "IND", "ATL", "CLE", "DET"],
    ),
    (
        1985,
        1985,
        Conference::West,
        "Midwest",
        &["SAS", "KCK", "HOU", "DEN", "UTA", "DAL"],
    ),
    (
        1985,
        1985,
        Conference::West,
        "Pacific",
        &["PHO", "LAL", "POR", "GSW", "LAC", "SEA"],
    ),
    (
        1986,
        1988,
        Conference::East,
        "Atlantic",
        &["BOS", "PHI", "WSB", "NJN", "NYK"],
    ),
    (
        1986,
        1988,
        Conference::East,
        "Central",
        &["MIL", "ATL", "DET", "CHI", "CLE", "IND"],
    ),
    (
        1986,
        1988,
        Conference::West,
        "Midwest",
        &["HOU", "DEN", "DAL", "UTA", "SAC", "SAS"],
    ),
    (
        1986,
        1988,
        Conference::West,
        "Pacific",
        &["LAL", "POR", "LAC", "PHO", "SEA", "GSW"],
    ),
    (
        1989,
        1989,
        Conference::East,
        "Atlantic",
        &["NYK", "PHI", "BOS", "WSB", "NJN", "CHH"],
    ),
    (
        1989,
        1989,
        Conference::East,
        "Central",
        &["DET", "CLE", "ATL", "MIL", "CHI", "IND"],
    ),
    (
        1989,
        1989,
        Conference::West,
        "Midwest",
        &["UTA", "HOU", "DEN", "DAL", "SAS", "MIA"],
    ),
    (
        1989,
        1989,
        Conference::West,
        "Pacific",
        &["LAL", "PHO", "SEA", "GSW", "POR", "SAC", "LAC"],
    ),
    (
        1990,
        1990,
        Conference::East,
        "Atlantic",
        &["PHI", "BOS", "NYK", "WSB", "MIA", "NJN"],
    ),
    (
        1990,
        1990,
        Conference::East,
        "Central",
        &["DET", "CHI", "MIL", "ATL", "CLE", "IND", "ORL"],
    ),
    (
        1990,
        1990,
        Conference::West,
        "Midwest",
        &["SAS", "UTA", "DAL", "DEN", "HOU", "MIN", "CHH"],
    ),
    (
        1990,
        1990,
        Conference::West,
        "Pacific",
        &["LAL", "POR", "PHO", "SEA", "GSW", "LAC", "SAC"],
    ),
    (
        1991,
        1991,
        Conference::East,
        "Atlantic",
        &["BOS", "PHI", "NYK", "WSB", "NJN", "MIA"],
    ),
    (
        1991,
        1991,
        Conference::East,
        "Central",
        &["CHI", "DET", "MIL", "ATL", "IND", "CLE", "CHH"],
    ),
    (
        1991,
        1991,
        Conference::West,
        "Midwest",
        &["SAS", "UTA", "HOU", "ORL", "MIN", "DAL", "DEN"],
    ),
    (
        1991,
        1991,
        Conference::West,
        "Pacific",
        &["POR", "LAL", "PHO", "GSW", "SEA", "LAC", "SAC"],
    ),
    (
        1992,
        1995,
        Conference::East,
        "Atlantic",
        &["BOS", "NYK", "NJN", "MIA", "PHI", "WSB", "ORL"],
    ),
    (
        1992,
        1995,
        Conference::East,
        "Central",
        &["CHI", "CLE", "DET", "IND", "ATL", "CHH", "MIL"],
    ),
    (
        1992,
        1995,
        Conference::West,
        "Midwest",
        &["UTA", "SAS", "HOU", "DEN", "DAL", "MIN"],
    ),
    (
        1992,
        1995,
        Conference::West,
        "Pacific",
        &["POR", "GSW", "PHO", "SEA", "LAC", "LAL", "SAC"],
    ),
    (
        1996,
        1997,
        Conference::East,
        "Atlantic",
        &["ORL", "NYK", "MIA", "WSB", "BOS", "PHI", "NJN"],
    ),
    (
        1996,
        1997,
        Conference::East,
        "Central",
        &["CHI", "IND", "CLE", "ATL", "DET", "CHH", "MIL", "TOR"],
    ),
    (
        1996,
        1997,
        Conference::West,
        "Midwest",
        &["SAS", "UTA", "HOU", "DEN", "DAL", "MIN", "VAN"],
    ),
    (
        1996,
        1997,
        Conference::West,
        "Pacific",
        &["SEA", "LAL", "POR", "PHO", "SAC", "GSW", "LAC"],
    ),
    (
        1998,
        2001,
        Conference::East,
        "Atlantic",
        &["ORL", "NYK", "MIA", "WAS", "BOS", "PHI", "NJN"],
    ),
    (
        1998,
        2001,
        Conference::East,
        "Central",
        &["CHI", "IND", "CLE", "ATL", "DET", "CHH", "MIL", "TOR"],
    ),
    (
        1998,
        2001,
        Conference::West,
        "Midwest",
        &["SAS", "UTA", "HOU", "DEN", "DAL", "MIN", "VAN"],
    ),
    (
        1998,
        2001,
        Conference::West,
        "Pacific",
        &["SEA", "LAL", "POR", "PHO", "SAC", "GSW", "LAC"],
    ),
    (
        2002,
        2002,
        Conference::East,
        "Atlantic",
        &["ORL", "NYK", "MIA", "WAS", "BOS", "PHI", "NJN"],
    ),
    (
        2002,
        2002,
        Conference::East,
        "Central",
        &["CHI", "IND", "CLE", "ATL", "DET", "CHH", "MIL", "TOR"],
    ),
    (
        2002,
        2002,
        Conference::West,
        "Midwest",
        &["SAS", "UTA", "HOU", "DEN", "DAL", "MIN", "MEM"],
    ),
    (
        2002,
        2002,
        Conference::West,
        "Pacific",
        &["SEA", "LAL", "POR", "PHO", "SAC", "GSW", "LAC"],
    ),
];

/// Conference / division alignment of a team code for a season. From 2002-03 the layout is
/// keyed by franchise; before that `HISTORICAL_ALIGNMENT` lists the team codes of the season.
pub fn alignment(season: SeasonYear, code: &str) -> Option<(Conference, &'static str)> {
    use Conference::{East, West};
    if season < SeasonYear::FIRST_MODERN {
        let y = season.end_year();
        return HISTORICAL_ALIGNMENT
            .iter()
            .find(|(a, b, _, _, codes)| *a <= y && y <= *b && codes.contains(&code))
            .map(|(_, _, conf, div, _)| (*conf, *div));
    }
    // Map any historical code to the franchise's "modern" code first.
    let fran = lookup_team(code).map(|t| t.1).unwrap_or(code);
    if season.end_year() <= 2004 {
        let r = match fran {
            "BOS" | "MIA" | "BRK" | "NYK" | "ORL" | "PHI" | "WAS" => (East, "Atlantic"),
            "ATL" | "CHI" | "CLE" | "DET" | "IND" | "MIL" | "NOP" | "TOR" => (East, "Central"),
            "DAL" | "DEN" | "HOU" | "MEM" | "MIN" | "SAS" | "UTA" => (West, "Midwest"),
            "GSW" | "LAC" | "LAL" | "PHO" | "POR" | "SAC" | "OKC" => (West, "Pacific"),
            _ => return None,
        };
        return Some(r);
    }
    let r = match fran {
        "BOS" | "BRK" | "NYK" | "PHI" | "TOR" => (East, "Atlantic"),
        "CHI" | "CLE" | "DET" | "IND" | "MIL" => (East, "Central"),
        "ATL" | "CHO" | "MIA" | "ORL" | "WAS" => (East, "Southeast"),
        "DEN" | "MIN" | "POR" | "OKC" | "UTA" => (West, "Northwest"),
        "GSW" | "LAC" | "LAL" | "PHO" | "SAC" => (West, "Pacific"),
        "DAL" | "HOU" | "MEM" | "NOP" | "SAS" => (West, "Southwest"),
        _ => return None,
    };
    Some(r)
}

/// Curated championship records used to validate derived results and to cover seasons whose
/// playoff games are incomplete in the imported data (1946-47 to 1959-60 and 1967-68 to
/// 1982-83 have byes or formats the bracket model does not resolve). Codes are the team code
/// of that season, which equals the franchise key for current teams.
pub const CURATED_CHAMPIONS: &[(i32, &str, &str)] = &[
    (1947, "PHW", "CHS"),
    (1948, "BLB", "PHW"),
    (1949, "MNL", "WSC"),
    (1950, "MNL", "SYR"),
    (1951, "ROC", "NYK"),
    (1952, "MNL", "NYK"),
    (1953, "MNL", "NYK"),
    (1954, "MNL", "SYR"),
    (1955, "SYR", "FTW"),
    (1956, "PHW", "FTW"),
    (1957, "BOS", "STL"),
    (1958, "STL", "BOS"),
    (1959, "BOS", "MNL"),
    (1960, "BOS", "STL"),
    (1968, "BOS", "LAL"),
    (1969, "BOS", "LAL"),
    (1970, "NYK", "LAL"),
    (1971, "MIL", "BAL"),
    (1972, "LAL", "NYK"),
    (1973, "NYK", "LAL"),
    (1974, "BOS", "MIL"),
    (1975, "GSW", "WSB"),
    (1976, "BOS", "PHO"),
    (1977, "POR", "PHI"),
    (1978, "WSB", "SEA"),
    (1979, "SEA", "WSB"),
    (1980, "LAL", "PHI"),
    (1981, "BOS", "HOU"),
    (1982, "LAL", "PHI"),
    (1983, "PHI", "LAL"),
    (2003, "SAS", "BRK"),
    (2004, "DET", "LAL"),
    (2005, "SAS", "DET"),
    (2006, "MIA", "DAL"),
    (2007, "SAS", "CLE"),
    (2008, "BOS", "LAL"),
    (2009, "LAL", "ORL"),
    (2010, "LAL", "BOS"),
    (2011, "DAL", "MIA"),
    (2012, "MIA", "OKC"),
    (2013, "MIA", "SAS"),
    (2014, "SAS", "MIA"),
    (2015, "GSW", "CLE"),
    (2016, "CLE", "GSW"),
    (2017, "GSW", "CLE"),
    (2018, "GSW", "CLE"),
    (2019, "TOR", "GSW"),
    (2020, "LAL", "MIA"),
    (2021, "MIL", "PHO"),
    (2022, "GSW", "BOS"),
    (2023, "DEN", "MIA"),
    (2024, "BOS", "DAL"),
    (2025, "OKC", "IND"),
];

/// Uniform colours of the active franchises as (primary, secondary) hex values, used by the
/// charts. Primaries are dark enough for a white chart surface (the Spurs' silver is a
/// secondary for that reason). Defunct franchises fall back to `DEFAULT_COLORS`.
pub const FRANCHISE_COLORS: &[(&str, &str, &str)] = &[
    ("ATL", "#C8102E", "#FDB927"),
    ("BOS", "#007A33", "#BA9653"),
    ("BRK", "#000000", "#A1A1A4"),
    ("CHI", "#CE1141", "#000000"),
    ("CHO", "#1D1160", "#00788C"),
    ("CLE", "#860038", "#FDBB30"),
    ("DAL", "#00538C", "#B8C4CA"),
    ("DEN", "#0E2240", "#FEC524"),
    ("DET", "#C8102E", "#1D42BA"),
    ("GSW", "#1D428A", "#FFC72C"),
    ("HOU", "#CE1141", "#000000"),
    ("IND", "#002D62", "#FDBB30"),
    ("LAC", "#C8102E", "#1D428A"),
    ("LAL", "#552583", "#FDB927"),
    ("MEM", "#12173F", "#5D76A9"),
    ("MIA", "#98002E", "#F9A01B"),
    ("MIL", "#00471B", "#EEE1C6"),
    ("MIN", "#0C2340", "#78BE20"),
    ("NOP", "#0C2340", "#C8102E"),
    ("NYK", "#006BB6", "#F58426"),
    ("OKC", "#007AC1", "#EF3B24"),
    ("ORL", "#0077C0", "#C4CED4"),
    ("PHI", "#006BB6", "#ED174C"),
    ("PHO", "#1D1160", "#E56020"),
    ("POR", "#E03A3E", "#000000"),
    ("SAC", "#5A2D81", "#63727A"),
    ("SAS", "#000000", "#C4CED4"),
    ("TOR", "#CE1141", "#000000"),
    ("UTA", "#002B5C", "#F9A01B"),
    ("WAS", "#002B5C", "#E31837"),
];

pub const DEFAULT_COLORS: (&str, &str) = ("#475569", "#94A3B8");

/// Team codes of the ABA (1967-68 to 1975-76) in the dataset. Every other code is NBA or BAA.
pub const ABA_TEAM_CODES: &[&str] = &[
    "ANA", "CAR", "DLC", "DNA", "DNR", "FLO", "HSM", "INA", "KEN", "LAS", "MMF", "MMP", "MMS",
    "MMT", "MNM", "MNP", "NJA", "NOB", "NYA", "OAK", "PTC", "PTP", "SAA", "SDA", "SDS", "SSL",
    "TEX", "UTS", "VIR", "WSA",
];

pub fn is_aba_code(code: &str) -> bool {
    ABA_TEAM_CODES.contains(&code)
}

/// (primary, secondary) colours of a franchise key; defunct franchises get the default pair.
pub fn franchise_colors(key: &str) -> (&'static str, &'static str) {
    FRANCHISE_COLORS
        .iter()
        .find(|c| c.0 == key)
        .map(|c| (c.1, c.2))
        .unwrap_or(DEFAULT_COLORS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_alignment_lists_every_team_of_every_season_once() {
        // NBA/BAA teams per season, 1946-47 to 2001-02, from the game log.
        const TEAMS: [usize; 56] = [
            11, 8, 12, 17, 11, 10, 10, 9, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 10, 12, 14, 14, 17,
            17, 17, 17, 18, 18, 22, 22, 22, 22, 23, 23, 23, 23, 23, 23, 23, 23, 25, 27, 27, 27, 27,
            27, 27, 29, 29, 29, 29, 29, 29, 29,
        ];
        for (i, expected) in TEAMS.iter().enumerate() {
            let y = 1947 + i as i32;
            let mut codes: Vec<&str> = HISTORICAL_ALIGNMENT
                .iter()
                .filter(|(a, b, _, _, _)| *a <= y && y <= *b)
                .flat_map(|(_, _, _, _, codes)| codes.iter().copied())
                .collect();
            let n = codes.len();
            codes.sort_unstable();
            codes.dedup();
            assert_eq!(n, codes.len(), "duplicate code in {y}");
            assert_eq!(n, *expected, "team count in {y}");
            for code in &codes {
                let t = lookup_team(code).unwrap_or_else(|| panic!("unknown code {code} in {y}"));
                assert!(t.4 <= y && y <= t.5, "{code} did not play in {y}");
                assert!(!is_aba_code(code), "{code} is an ABA code");
            }
        }
        assert_eq!(
            alignment(SeasonYear::of(1991), "ORL"),
            Some((Conference::West, "Midwest"))
        );
        assert_eq!(
            alignment(SeasonYear::of(1947), "PHW"),
            Some((Conference::East, "Eastern"))
        );
        assert_eq!(alignment(SeasonYear::of(1975), "KEN"), None);
    }

    #[test]
    fn aba_codes_exist_in_the_lineage_and_active_codes_are_not_aba() {
        for code in ABA_TEAM_CODES {
            assert!(TEAM_LINEAGE.iter().any(|t| t.0 == *code), "{code}");
        }
        for key in active_franchise_keys() {
            assert!(!is_aba_code(key), "{key}");
        }
    }

    #[test]
    fn every_active_franchise_has_colours() {
        for key in active_franchise_keys() {
            assert_ne!(franchise_colors(key), DEFAULT_COLORS, "{key}");
        }
        assert_eq!(franchise_colors("CHS"), DEFAULT_COLORS);
        for (key, p, s) in FRANCHISE_COLORS {
            for c in [p, s] {
                assert!(c.len() == 7 && c.starts_with('#'), "{key}: {c}");
                assert!(i64::from_str_radix(&c[1..], 16).is_ok(), "{key}: {c}");
            }
        }
    }

    #[test]
    fn thirty_active_franchises() {
        assert_eq!(active_franchise_keys().len(), 30);
    }

    #[test]
    fn codes_are_unique() {
        let mut codes: Vec<&str> = TEAM_LINEAGE.iter().map(|t| t.0).collect();
        codes.sort();
        let n = codes.len();
        codes.dedup();
        assert_eq!(n, codes.len());
    }

    #[test]
    fn alignment_covers_all_active_teams_each_season() {
        for year in 2003..=2026 {
            let season = SeasonYear::of(year);
            let expected = if year <= 2004 { 29 } else { 30 };
            let n = active_franchise_keys()
                .iter()
                .filter(|k| alignment(season, k).is_some())
                .count();
            // CHO did not exist before 2005
            assert_eq!(n, expected, "season {season}");
        }
        assert_eq!(
            alignment(SeasonYear::of(2008), "SEA"),
            Some((Conference::West, "Northwest"))
        );
        assert_eq!(
            alignment(SeasonYear::of(2012), "NJN"),
            Some((Conference::East, "Atlantic"))
        );
        assert_eq!(
            alignment(SeasonYear::of(2004), "NOH"),
            Some((Conference::East, "Central"))
        );
        assert_eq!(
            alignment(SeasonYear::of(2007), "NOK"),
            Some((Conference::West, "Southwest"))
        );
        assert_eq!(
            alignment(SeasonYear::of(2002), "LAL"),
            Some((Conference::West, "Pacific"))
        );
        assert_eq!(alignment(SeasonYear::of(2002), "KEN"), None);
    }
}
