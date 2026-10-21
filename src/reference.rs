//! Static reference data: franchise lineage for every team code that appears in the
//! FiveThirtyEight / Neil Paine Elo datasets, and conference/division alignment by season.
//!
//! Franchise identity follows FiveThirtyEight's `fran_id` convention: a franchise is the
//! organisation, followed through relocations and renamings. Notably the 1989–2002 Charlotte
//! Hornets (CHH) are part of the New Orleans Pelicans lineage, and the 2005+ Charlotte
//! Bobcats/Hornets (CHA/CHO) are a separate franchise.

/// (team code, franchise key, franchise display name, team display name, first season, last season)
/// Seasons are end-years (2004 == 2003-04). `last_season` of 9999 means "current".
pub const TEAM_LINEAGE: &[(&str, &str, &str, &str, i32, i32)] = &[
    // --- 30 active franchises (franchise key == current code) ---
    ("ATL", "ATL", "Atlanta Hawks", "Atlanta Hawks", 1969, 9999),
    ("STL", "ATL", "Atlanta Hawks", "St. Louis Hawks", 1956, 1968),
    ("MLH", "ATL", "Atlanta Hawks", "Milwaukee Hawks", 1952, 1955),
    ("TRI", "ATL", "Atlanta Hawks", "Tri-Cities Blackhawks", 1950, 1951),
    ("BOS", "BOS", "Boston Celtics", "Boston Celtics", 1947, 9999),
    ("BRK", "BRK", "Brooklyn Nets", "Brooklyn Nets", 2013, 9999),
    ("NJN", "BRK", "Brooklyn Nets", "New Jersey Nets", 1978, 2012),
    ("NYN", "BRK", "Brooklyn Nets", "New York Nets", 1977, 1977),
    ("NYA", "BRK", "Brooklyn Nets", "New York Nets (ABA)", 1969, 1976),
    ("NJA", "BRK", "Brooklyn Nets", "New Jersey Americans (ABA)", 1968, 1968),
    ("CHO", "CHO", "Charlotte Hornets", "Charlotte Hornets", 2005, 9999),
    ("CHA", "CHO", "Charlotte Hornets", "Charlotte Bobcats", 2005, 2014),
    ("CHI", "CHI", "Chicago Bulls", "Chicago Bulls", 1967, 9999),
    ("CLE", "CLE", "Cleveland Cavaliers", "Cleveland Cavaliers", 1971, 9999),
    ("DAL", "DAL", "Dallas Mavericks", "Dallas Mavericks", 1981, 9999),
    ("DEN", "DEN", "Denver Nuggets", "Denver Nuggets", 1977, 9999),
    ("DNA", "DEN", "Denver Nuggets", "Denver Nuggets (ABA)", 1975, 1976),
    ("DNR", "DEN", "Denver Nuggets", "Denver Rockets (ABA)", 1968, 1974),
    ("DET", "DET", "Detroit Pistons", "Detroit Pistons", 1958, 9999),
    ("FTW", "DET", "Detroit Pistons", "Fort Wayne Pistons", 1949, 1957),
    ("GSW", "GSW", "Golden State Warriors", "Golden State Warriors", 1972, 9999),
    ("SFW", "GSW", "Golden State Warriors", "San Francisco Warriors", 1963, 1971),
    ("PHW", "GSW", "Golden State Warriors", "Philadelphia Warriors", 1947, 1962),
    ("HOU", "HOU", "Houston Rockets", "Houston Rockets", 1972, 9999),
    ("SDR", "HOU", "Houston Rockets", "San Diego Rockets", 1968, 1971),
    ("IND", "IND", "Indiana Pacers", "Indiana Pacers", 1977, 9999),
    ("INA", "IND", "Indiana Pacers", "Indiana Pacers (ABA)", 1968, 1976),
    ("LAC", "LAC", "Los Angeles Clippers", "Los Angeles Clippers", 1985, 9999),
    ("SDC", "LAC", "Los Angeles Clippers", "San Diego Clippers", 1979, 1984),
    ("BUF", "LAC", "Los Angeles Clippers", "Buffalo Braves", 1971, 1978),
    ("LAL", "LAL", "Los Angeles Lakers", "Los Angeles Lakers", 1961, 9999),
    ("MNL", "LAL", "Los Angeles Lakers", "Minneapolis Lakers", 1949, 1960),
    ("MEM", "MEM", "Memphis Grizzlies", "Memphis Grizzlies", 2002, 9999),
    ("VAN", "MEM", "Memphis Grizzlies", "Vancouver Grizzlies", 1996, 2001),
    ("MIA", "MIA", "Miami Heat", "Miami Heat", 1989, 9999),
    ("MIL", "MIL", "Milwaukee Bucks", "Milwaukee Bucks", 1969, 9999),
    ("MIN", "MIN", "Minnesota Timberwolves", "Minnesota Timberwolves", 1990, 9999),
    ("NOP", "NOP", "New Orleans Pelicans", "New Orleans Pelicans", 2003, 9999),
    ("NOH", "NOP", "New Orleans Pelicans", "New Orleans Hornets", 2003, 2013),
    ("NOK", "NOP", "New Orleans Pelicans", "New Orleans/Oklahoma City Hornets", 2006, 2007),
    ("CHH", "NOP", "New Orleans Pelicans", "Charlotte Hornets (1989-2002)", 1989, 2002),
    ("NYK", "NYK", "New York Knicks", "New York Knicks", 1947, 9999),
    ("OKC", "OKC", "Oklahoma City Thunder", "Oklahoma City Thunder", 2009, 9999),
    ("SEA", "OKC", "Oklahoma City Thunder", "Seattle SuperSonics", 1968, 2008),
    ("ORL", "ORL", "Orlando Magic", "Orlando Magic", 1990, 9999),
    ("PHI", "PHI", "Philadelphia 76ers", "Philadelphia 76ers", 1964, 9999),
    ("SYR", "PHI", "Philadelphia 76ers", "Syracuse Nationals", 1950, 1963),
    ("PHO", "PHO", "Phoenix Suns", "Phoenix Suns", 1969, 9999),
    ("POR", "POR", "Portland Trail Blazers", "Portland Trail Blazers", 1971, 9999),
    ("SAC", "SAC", "Sacramento Kings", "Sacramento Kings", 1986, 9999),
    ("KCK", "SAC", "Sacramento Kings", "Kansas City Kings", 1976, 1985),
    ("KCO", "SAC", "Sacramento Kings", "Kansas City-Omaha Kings", 1973, 1975),
    ("CIN", "SAC", "Sacramento Kings", "Cincinnati Royals", 1958, 1972),
    ("ROC", "SAC", "Sacramento Kings", "Rochester Royals", 1949, 1957),
    ("SAS", "SAS", "San Antonio Spurs", "San Antonio Spurs", 1977, 9999),
    ("SAA", "SAS", "San Antonio Spurs", "San Antonio Spurs (ABA)", 1974, 1976),
    ("TEX", "SAS", "San Antonio Spurs", "Texas Chaparrals (ABA)", 1971, 1971),
    ("DLC", "SAS", "San Antonio Spurs", "Dallas Chaparrals (ABA)", 1968, 1973),
    ("TOR", "TOR", "Toronto Raptors", "Toronto Raptors", 1996, 9999),
    ("UTA", "UTA", "Utah Jazz", "Utah Jazz", 1980, 9999),
    ("NOJ", "UTA", "Utah Jazz", "New Orleans Jazz", 1975, 1979),
    ("WAS", "WAS", "Washington Wizards", "Washington Wizards", 1998, 9999),
    ("WSB", "WAS", "Washington Wizards", "Washington Bullets", 1975, 1997),
    ("CAP", "WAS", "Washington Wizards", "Capital Bullets", 1974, 1974),
    ("BAL", "WAS", "Washington Wizards", "Baltimore Bullets", 1964, 1973),
    ("CHZ", "WAS", "Washington Wizards", "Chicago Zephyrs", 1963, 1963),
    ("CHP", "WAS", "Washington Wizards", "Chicago Packers", 1962, 1962),
    // --- defunct franchises (franchise key == last code) ---
    ("BLB", "BLB", "Baltimore Bullets (1947-54)", "Baltimore Bullets (original)", 1948, 1954),
    ("STB", "STB", "St. Louis Bombers", "St. Louis Bombers", 1947, 1950),
    ("WSC", "WSC", "Washington Capitols", "Washington Capitols", 1947, 1951),
    ("KEN", "KEN", "Kentucky Colonels (ABA)", "Kentucky Colonels", 1968, 1976),
    ("PTC", "PTC", "Pittsburgh Condors (ABA)", "Pittsburgh Condors", 1971, 1972),
    ("PTP", "PTC", "Pittsburgh Condors (ABA)", "Pittsburgh Pipers", 1968, 1970),
    ("MNP", "PTC", "Pittsburgh Condors (ABA)", "Minnesota Pipers", 1969, 1969),
    ("DNN", "DNN", "Denver Nuggets (1949-50)", "Denver Nuggets (original)", 1950, 1950),
    ("DTF", "DTF", "Detroit Falcons", "Detroit Falcons", 1947, 1947),
    ("FLO", "FLO", "Floridians (ABA)", "The Floridians", 1971, 1972),
    ("MMF", "FLO", "Floridians (ABA)", "Miami Floridians", 1969, 1970),
    ("MNM", "FLO", "Floridians (ABA)", "Minnesota Muskies", 1968, 1968),
    ("TRH", "TRH", "Toronto Huskies", "Toronto Huskies", 1947, 1947),
    ("PIT", "PIT", "Pittsburgh Ironmen", "Pittsburgh Ironmen", 1947, 1947),
    ("INJ", "INJ", "Indianapolis Jets", "Indianapolis Jets", 1949, 1949),
    ("INO", "INO", "Indianapolis Olympians", "Indianapolis Olympians", 1950, 1953),
    ("AND", "AND", "Anderson Packers", "Anderson Packers", 1950, 1950),
    ("CLR", "CLR", "Cleveland Rebels", "Cleveland Rebels", 1947, 1947),
    ("SHE", "SHE", "Sheboygan Red Skins", "Sheboygan Red Skins", 1950, 1950),
    ("SDS", "SDS", "San Diego Sails (ABA)", "San Diego Sails", 1976, 1976),
    ("SDA", "SDS", "San Diego Sails (ABA)", "San Diego Conquistadors", 1973, 1975),
    ("MMS", "MMS", "Memphis Sounds (ABA)", "Memphis Sounds", 1975, 1975),
    ("MMT", "MMS", "Memphis Sounds (ABA)", "Memphis Tams", 1973, 1974),
    ("MMP", "MMS", "Memphis Sounds (ABA)", "Memphis Pros", 1971, 1972),
    ("NOB", "MMS", "Memphis Sounds (ABA)", "New Orleans Buccaneers", 1968, 1970),
    ("SSL", "SSL", "Spirits of St. Louis (ABA)", "Spirits of St. Louis", 1975, 1976),
    ("CAR", "SSL", "Spirits of St. Louis (ABA)", "Carolina Cougars", 1970, 1974),
    ("HSM", "SSL", "Spirits of St. Louis (ABA)", "Houston Mavericks", 1968, 1969),
    ("VIR", "VIR", "Virginia Squires (ABA)", "Virginia Squires", 1971, 1976),
    ("WSA", "VIR", "Virginia Squires (ABA)", "Washington Caps", 1970, 1970),
    ("OAK", "VIR", "Virginia Squires (ABA)", "Oakland Oaks", 1968, 1969),
    ("CHS", "CHS", "Chicago Stags", "Chicago Stags", 1947, 1950),
    ("UTS", "UTS", "Utah Stars (ABA)", "Utah Stars", 1971, 1976),
    ("LAS", "UTS", "Utah Stars (ABA)", "Los Angeles Stars", 1969, 1970),
    ("ANA", "UTS", "Utah Stars (ABA)", "Anaheim Amigos", 1968, 1968),
    ("PRO", "PRO", "Providence Steamrollers", "Providence Steamrollers", 1947, 1949),
    ("WAT", "WAT", "Waterloo Hawks", "Waterloo Hawks", 1950, 1950),
];

/// The 30 franchises active today, in alphabetical order of key.
pub fn active_franchise_keys() -> Vec<&'static str> {
    let mut v: Vec<&str> = TEAM_LINEAGE
        .iter()
        .filter(|t| t.5 == 9999)
        .map(|t| t.1)
        .collect();
    v.sort();
    v.dedup();
    v
}

pub fn lookup_team(code: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static str, i32, i32)> {
    TEAM_LINEAGE.iter().find(|t| t.0 == code)
}

/// Conference / division alignment of a team code for a season (end-year).
/// Only seasons from 2003 (2002-03) onward are covered; earlier seasons return `None`.
pub fn alignment(season: i32, code: &str) -> Option<(&'static str, &'static str)> {
    if season < 2003 {
        return None;
    }
    // Map any historical code to the franchise's "modern" code first.
    let fran = lookup_team(code).map(|t| t.1).unwrap_or(code);
    if season <= 2004 {
        let r = match fran {
            "BOS" | "MIA" | "BRK" | "NYK" | "ORL" | "PHI" | "WAS" => ("East", "Atlantic"),
            "ATL" | "CHI" | "CLE" | "DET" | "IND" | "MIL" | "NOP" | "TOR" => ("East", "Central"),
            "DAL" | "DEN" | "HOU" | "MEM" | "MIN" | "SAS" | "UTA" => ("West", "Midwest"),
            "GSW" | "LAC" | "LAL" | "PHO" | "POR" | "SAC" | "OKC" => ("West", "Pacific"),
            _ => return None,
        };
        return Some(r);
    }
    let r = match fran {
        "BOS" | "BRK" | "NYK" | "PHI" | "TOR" => ("East", "Atlantic"),
        "CHI" | "CLE" | "DET" | "IND" | "MIL" => ("East", "Central"),
        "ATL" | "CHO" | "MIA" | "ORL" | "WAS" => ("East", "Southeast"),
        "DEN" | "MIN" | "POR" | "OKC" | "UTA" => ("West", "Northwest"),
        "GSW" | "LAC" | "LAL" | "PHO" | "SAC" => ("West", "Pacific"),
        "DAL" | "HOU" | "MEM" | "NOP" | "SAS" => ("West", "Southwest"),
        _ => return None,
    };
    Some(r)
}

/// Curated championship records used to validate derived results and to cover seasons whose
/// playoff games are not present in the imported data. Winner codes are franchise keys.
pub const CURATED_CHAMPIONS: &[(i32, &str, &str)] = &[
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

pub fn season_label(season: i32) -> String {
    format!("{}-{:02}", season - 1, season % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for season in 2003..=2026 {
            let expected = if season <= 2004 { 29 } else { 30 };
            let n = active_franchise_keys()
                .iter()
                .filter(|k| alignment(season, k).is_some())
                .count();
            // CHO did not exist before 2005
            assert_eq!(n, expected, "season {season}");
        }
        assert_eq!(alignment(2008, "SEA"), Some(("West", "Northwest")));
        assert_eq!(alignment(2012, "NJN"), Some(("East", "Atlantic")));
        assert_eq!(alignment(2004, "NOH"), Some(("East", "Central")));
        assert_eq!(alignment(2007, "NOK"), Some(("West", "Southwest")));
    }

    #[test]
    fn labels() {
        assert_eq!(season_label(2004), "2003-04");
        assert_eq!(season_label(2000), "1999-00");
    }
}
