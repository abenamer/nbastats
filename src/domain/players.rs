//! Player eras: the careers of the league's most valuable players, curated for the era
//! builder. The database holds no player data, so this table lists each player with the
//! seasons he was active (NBA/BAA, and the ABA where he played there), a one-line summary and
//! a few dated milestones. It holds every league MVP since the award began in 1955-56 and
//! every Finals MVP since 1969, plus George Mikan from before either award. A season belongs
//! to every listed player active in it. Spans and notes are from the public record and are
//! marked as needing a check in the documentation.

use crate::domain::ids::SeasonYear;
use crate::domain::records::{change, Change, League};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerEra {
    pub name: &'static str,
    /// Active NBA/BAA seasons as (first, last) end years, inclusive.
    pub nba: &'static [(i32, i32)],
    /// Active ABA seasons, the same way.
    pub aba: &'static [(i32, i32)],
    pub summary: &'static str,
    pub changes: &'static [Change],
}

impl PlayerEra {
    /// Whether the player was active in `season` of `league`.
    pub fn active(&self, league: League, season: SeasonYear) -> bool {
        let spans = match league {
            League::Nba => self.nba,
            League::Aba => self.aba,
        };
        let y = season.end_year();
        spans.iter().any(|&(a, b)| a <= y && y <= b)
    }

    /// The first and last active season over both leagues.
    pub fn span(&self) -> (SeasonYear, SeasonYear) {
        let years = self.nba.iter().chain(self.aba.iter());
        let first = years.clone().map(|s| s.0).min().unwrap_or(1947);
        let last = years.map(|s| s.1).max().unwrap_or(1947);
        (SeasonYear::of(first), SeasonYear::of(last))
    }
}

pub const PLAYER_ERAS: &[PlayerEra] = &[
    PlayerEra {
        name: "George Mikan",
        nba: &[(1949, 1954), (1956, 1956)],
        aba: &[],
        summary: "Minneapolis Lakers center, 1948-49 to 1953-54 and a 1955-56 comeback; five titles, before either MVP award existed.",
        changes: &[
            change(1949, "Joins the BAA with the Lakers after the NBL merger; the first dominant big man."),
            change(1952, "The lane is widened from six to twelve feet to slow him."),
            change(1954, "Retires after a fifth title; returns for part of 1955-56."),
        ],
    },
    PlayerEra {
        name: "Bob Cousy",
        nba: &[(1951, 1963), (1970, 1970)],
        aba: &[],
        summary: "Boston Celtics, 1950-51 to 1962-63, and seven games with Cincinnati in 1969-70; six titles, the 1957 MVP, eight straight assist titles.",
        changes: &[
            change(1951, "Arrives in Boston after a dispersal draft."),
            change(1954, "Leads the founding of the players' association."),
            change(1957, "MVP and the first Celtics title."),
            change(1963, "Retires after a sixth title."),
        ],
    },
    PlayerEra {
        name: "Bob Pettit",
        nba: &[(1955, 1965)],
        aba: &[],
        summary: "Milwaukee and St. Louis Hawks, 1954-55 to 1964-65; the 1958 title over Boston, the first MVP, the first to 20,000 points.",
        changes: &[
            change(1956, "Wins the first MVP award."),
            change(1958, "50 points in the title-clinching game against the Celtics."),
            change(1964, "First player past 20,000 points."),
        ],
    },
    PlayerEra {
        name: "Bill Russell",
        nba: &[(1957, 1969)],
        aba: &[],
        summary: "Boston Celtics, 1956-57 to 1968-69; eleven titles in thirteen seasons, five MVPs.",
        changes: &[
            change(1957, "Rookie season ends with the first Celtics title."),
            change(1967, "Becomes player-coach, the first Black head coach in the league."),
            change(1969, "Retires after an eleventh title."),
        ],
    },
    PlayerEra {
        name: "Wilt Chamberlain",
        nba: &[(1960, 1973)],
        aba: &[],
        summary: "Philadelphia and San Francisco Warriors, 76ers, Lakers, 1959-60 to 1972-73; two titles, four MVPs, the 100-point game.",
        changes: &[
            change(1960, "Rookie of the Year and MVP in the same season."),
            change(1962, "100 points in a game and 50.4 per game for the season."),
            change(1967, "First title with the 76ers, breaking the Celtics run; a second with the 1972 Lakers."),
        ],
    },
    PlayerEra {
        name: "Oscar Robertson",
        nba: &[(1961, 1974)],
        aba: &[],
        summary: "Cincinnati Royals and Milwaukee Bucks, 1960-61 to 1973-74; a triple-double season in 1961-62, the 1971 title, the lawsuit that ended the reserve clause.",
        changes: &[
            change(1962, "Averages a triple-double for the season."),
            change(1970, "As union president, files the antitrust suit that ends the reserve clause in 1976."),
            change(1971, "Title with Milwaukee beside Kareem Abdul-Jabbar."),
        ],
    },
    PlayerEra {
        name: "Jerry West",
        nba: &[(1961, 1974)],
        aba: &[],
        summary: "Los Angeles Lakers, 1960-61 to 1973-74; nine Finals, one title, the only Finals MVP from the losing side.",
        changes: &[
            change(1969, "Finals MVP in a loss to Boston."),
            change(1972, "Title with the 33-game-winning-streak Lakers."),
        ],
    },
    PlayerEra {
        name: "John Havlicek",
        nba: &[(1963, 1978)],
        aba: &[],
        summary: "Boston Celtics, 1962-63 to 1977-78; eight titles across the Russell and Cowens eras, the 1974 Finals MVP, \"Havlicek stole the ball\".",
        changes: &[
            change(1965, "Steals the inbounds pass to close out Philadelphia in the conference finals."),
            change(1974, "Finals MVP in a seven-game win over Milwaukee."),
            change(1978, "Retires as the Celtics' career scoring leader."),
        ],
    },
    PlayerEra {
        name: "Willis Reed",
        nba: &[(1965, 1974)],
        aba: &[],
        summary: "New York Knicks, 1964-65 to 1973-74; the 1970 MVP and two titles; the limp onto the floor for Game 7 in 1970.",
        changes: &[
            change(1970, "MVP, All-Star MVP and Finals MVP in one season; plays Game 7 on a torn thigh muscle."),
            change(1973, "Second title and Finals MVP."),
        ],
    },
    PlayerEra {
        name: "Rick Barry",
        nba: &[(1966, 1967), (1973, 1980)],
        aba: &[(1969, 1972)],
        summary: "San Francisco Warriors, then Oakland, Washington and the New York Nets in the ABA, then Golden State and Houston to 1979-80; the 1975 Finals MVP, scoring titles in both leagues, underhand free throws.",
        changes: &[
            change(1967, "NBA scoring title in his second season; sits out a year to jump to the ABA."),
            change(1975, "Finals MVP as Golden State sweeps Washington."),
        ],
    },
    PlayerEra {
        name: "Wes Unseld",
        nba: &[(1969, 1981)],
        aba: &[],
        summary: "Baltimore, Capital and Washington Bullets, 1968-69 to 1980-81; MVP and Rookie of the Year in the same season (1969), the 1978 title.",
        changes: &[
            change(1969, "MVP as a rookie, the second player to do it after Chamberlain."),
            change(1978, "Title and Finals MVP."),
        ],
    },
    PlayerEra {
        name: "Kareem Abdul-Jabbar",
        nba: &[(1970, 1989)],
        aba: &[],
        summary: "Milwaukee and the Lakers, 1969-70 to 1988-89; six titles, six MVPs, the scoring record for 39 years.",
        changes: &[
            change(1971, "Title in his second season with Milwaukee."),
            change(1976, "Traded to the Lakers."),
            change(1984, "Passes Chamberlain as the career scoring leader."),
            change(1988, "Sixth title at 40, the oldest Finals starter of his time."),
        ],
    },
    PlayerEra {
        name: "Jo Jo White",
        nba: &[(1970, 1981)],
        aba: &[],
        summary: "Boston, Golden State, Kansas City, 1969-70 to 1980-81; the 1976 Finals MVP after the triple-overtime Game 5.",
        changes: &[
            change(1976, "Finals MVP; plays 60 minutes in the triple-overtime Game 5 against Phoenix."),
        ],
    },
    PlayerEra {
        name: "Dave Cowens",
        nba: &[(1971, 1980), (1983, 1983)],
        aba: &[],
        summary: "Boston Celtics, 1970-71 to 1979-80, and a 1982-83 return with Milwaukee; the 1973 MVP, titles in 1974 and 1976.",
        changes: &[
            change(1973, "MVP in his third season."),
            change(1974, "First of two titles as the undersized center."),
        ],
    },
    PlayerEra {
        name: "Julius Erving",
        nba: &[(1977, 1987)],
        aba: &[(1972, 1976)],
        summary: "Virginia Squires and New York Nets in the ABA 1971-72 to 1975-76, then the Philadelphia 76ers to 1986-87; two ABA titles and the 1983 NBA title.",
        changes: &[
            change(1972, "Debuts with the ABA Squires."),
            change(1974, "First of two ABA titles with the Nets."),
            change(1977, "Sold to the 76ers in the merger summer."),
            change(1983, "NBA title with Moses Malone."),
        ],
    },
    PlayerEra {
        name: "Bob McAdoo",
        nba: &[(1973, 1986)],
        aba: &[],
        summary: "Buffalo, New York, Boston, Detroit, New Jersey, the Lakers, Philadelphia, 1972-73 to 1985-86; the 1975 MVP, three straight scoring titles, two Lakers titles off the bench.",
        changes: &[
            change(1975, "MVP at 23 with Buffalo; three scoring titles in a row."),
            change(1982, "First of two titles as a Lakers reserve."),
        ],
    },
    PlayerEra {
        name: "Bill Walton",
        nba: &[(1975, 1978), (1980, 1980), (1983, 1987)],
        aba: &[],
        summary: "Portland, San Diego and the Clippers, Boston, 1974-75 to 1986-87 around years lost to foot injuries; the 1978 MVP, titles in 1977 and 1986.",
        changes: &[
            change(1977, "Title and Finals MVP with Portland."),
            change(1978, "MVP, then a broken foot ends the season and starts years of injuries."),
            change(1986, "Sixth Man of the Year on the title-winning Celtics."),
        ],
    },
    PlayerEra {
        name: "Moses Malone",
        nba: &[(1977, 1995)],
        aba: &[(1975, 1976)],
        summary: "Utah and St. Louis in the ABA, then Buffalo, Houston, Philadelphia, Washington, Atlanta, Milwaukee, San Antonio to 1994-95; three MVPs (1979, 1982, 1983), the 1983 title, the first to go from high school to the pros.",
        changes: &[
            change(1975, "Signs with the ABA Utah Stars straight out of high school."),
            change(1979, "First of three MVPs."),
            change(1983, "MVP, Finals MVP and the \"fo, fo, fo\" title with Philadelphia."),
        ],
    },
    PlayerEra {
        name: "Dennis Johnson",
        nba: &[(1977, 1990)],
        aba: &[],
        summary: "Seattle, Phoenix, Boston, 1976-77 to 1989-90; the 1979 Finals MVP with the SuperSonics and two titles as the Celtics' guard.",
        changes: &[
            change(1979, "Finals MVP as Seattle beats Washington."),
            change(1984, "Traded to Boston; titles in 1984 and 1986."),
        ],
    },
    PlayerEra {
        name: "Cedric Maxwell",
        nba: &[(1978, 1988)],
        aba: &[],
        summary: "Boston, the Clippers, Houston, 1977-78 to 1987-88; the 1981 Finals MVP.",
        changes: &[
            change(1981, "Finals MVP against Houston."),
            change(1984, "Tells his teammates to \"climb on my back\" before Game 7 against the Lakers."),
        ],
    },
    PlayerEra {
        name: "Magic Johnson",
        nba: &[(1980, 1991), (1996, 1996)],
        aba: &[],
        summary: "Los Angeles Lakers, 1979-80 to 1990-91 and a 1995-96 return; five titles, three MVPs.",
        changes: &[
            change(1980, "Starts at center in the title-clinching game as a rookie."),
            change(1991, "Announces his HIV diagnosis and retires; returns for 32 games in 1996."),
        ],
    },
    PlayerEra {
        name: "Larry Bird",
        nba: &[(1980, 1992)],
        aba: &[],
        summary: "Boston Celtics, 1979-80 to 1991-92; three titles, three straight MVPs.",
        changes: &[
            change(1980, "Rookie of the Year; the Celtics improve by 32 wins."),
            change(1986, "Third title with the 67-win Celtics."),
            change(1992, "Retires after the Dream Team summer."),
        ],
    },
    PlayerEra {
        name: "Isiah Thomas",
        nba: &[(1982, 1994)],
        aba: &[],
        summary: "Detroit Pistons, 1981-82 to 1993-94; two titles with the Bad Boys, the 1990 Finals MVP, 25 points in one quarter on a sprained ankle in the 1988 Finals.",
        changes: &[
            change(1982, "All-Star as a rookie."),
            change(1989, "First title, a sweep of the Lakers."),
            change(1990, "Second title and Finals MVP."),
            change(1994, "Retires after an Achilles tear."),
        ],
    },
    PlayerEra {
        name: "James Worthy",
        nba: &[(1983, 1994)],
        aba: &[],
        summary: "Los Angeles Lakers, 1982-83 to 1993-94; three titles with Showtime and the 1988 Finals MVP after a Game 7 triple-double.",
        changes: &[
            change(1988, "Finals MVP; 36 points, 16 rebounds and 10 assists in Game 7 against Detroit."),
        ],
    },
    PlayerEra {
        name: "Michael Jordan",
        nba: &[(1985, 1993), (1995, 1998), (2002, 2003)],
        aba: &[],
        summary: "Chicago Bulls 1984-85 to 1992-93 and 1994-95 to 1997-98, Washington Wizards 2001-02 to 2002-03; six titles, five MVPs.",
        changes: &[
            change(1985, "Rookie of the Year."),
            change(1991, "First of six titles."),
            change(1994, "First retirement, to play baseball; returns in March 1995."),
            change(1999, "Second retirement after the 1998 title; two seasons in Washington from 2001."),
        ],
    },
    PlayerEra {
        name: "Charles Barkley",
        nba: &[(1985, 2000)],
        aba: &[],
        summary: "Philadelphia, Phoenix, Houston, 1984-85 to 1999-2000; the 1993 MVP and a Finals loss to Jordan that year.",
        changes: &[
            change(1993, "MVP in his first Phoenix season; the Suns lose the Finals in six."),
            change(2000, "Retires after a torn quadriceps, returning for one final game."),
        ],
    },
    PlayerEra {
        name: "Hakeem Olajuwon",
        nba: &[(1985, 2002)],
        aba: &[],
        summary: "Houston Rockets 1984-85 to 2000-01, Toronto 2001-02; two titles, 1994 MVP, the career blocks leader.",
        changes: &[
            change(1994, "MVP, Defensive Player of the Year and Finals MVP in one season."),
            change(1995, "Second title from the sixth seed."),
        ],
    },
    PlayerEra {
        name: "Joe Dumars",
        nba: &[(1986, 1999)],
        aba: &[],
        summary: "Detroit Pistons, 1985-86 to 1998-99; two titles with the Bad Boys and the 1989 Finals MVP.",
        changes: &[
            change(1989, "Finals MVP in the sweep of the Lakers."),
            change(1990, "Second title; plays through his father's death during the Finals."),
        ],
    },
    PlayerEra {
        name: "Karl Malone",
        nba: &[(1986, 2004)],
        aba: &[],
        summary: "Utah Jazz 1985-86 to 2002-03, the Lakers in 2003-04; two MVPs (1997, 1999), two Finals losses to the Bulls, 36,928 points.",
        changes: &[
            change(1997, "First MVP; the Jazz reach the Finals."),
            change(1999, "Second MVP in the lockout season."),
            change(2004, "One season with the Lakers; loses a third Finals."),
        ],
    },
    PlayerEra {
        name: "David Robinson",
        nba: &[(1990, 2003)],
        aba: &[],
        summary: "San Antonio Spurs, 1989-90 to 2002-03; the 1995 MVP, two titles with Duncan.",
        changes: &[
            change(1990, "Rookie of the Year after two years in the Navy; the Spurs improve by 35 wins."),
            change(1995, "MVP."),
            change(2003, "Retires with a second title."),
        ],
    },
    PlayerEra {
        name: "Shaquille O'Neal",
        nba: &[(1993, 2011)],
        aba: &[],
        summary: "Orlando, the Lakers, Miami, Phoenix, Cleveland, Boston, 1992-93 to 2010-11; four titles, the 2000 MVP.",
        changes: &[
            change(1993, "Rookie of the Year with Orlando."),
            change(2000, "MVP and the first of three straight titles with the Lakers."),
            change(2006, "Fourth title with Miami."),
        ],
    },
    PlayerEra {
        name: "Kevin Garnett",
        nba: &[(1996, 2016)],
        aba: &[],
        summary: "Minnesota, Boston, Minnesota again, 1995-96 to 2015-16; the 2004 MVP, the 2008 title and Defensive Player of the Year.",
        changes: &[
            change(1996, "Drafted from high school, the first in twenty years."),
            change(2004, "MVP; Minnesota reaches the conference finals."),
            change(2008, "Title with Boston in his first season there."),
        ],
    },
    PlayerEra {
        name: "Allen Iverson",
        nba: &[(1997, 2010)],
        aba: &[],
        summary: "Philadelphia, Denver, Detroit, Memphis, Philadelphia again, 1996-97 to 2009-10; the 2001 MVP and a Finals trip at six feet tall.",
        changes: &[
            change(1997, "Rookie of the Year; the crossover on Jordan."),
            change(2001, "MVP and a Finals win in Game 1 over the Lakers."),
        ],
    },
    PlayerEra {
        name: "Kobe Bryant",
        nba: &[(1997, 2016)],
        aba: &[],
        summary: "Los Angeles Lakers, 1996-97 to 2015-16; five titles, the 2008 MVP, 81 points in a game.",
        changes: &[
            change(1997, "Enters the league from high school at 18."),
            change(2006, "81 points against Toronto."),
            change(2010, "Fifth title, the second without O'Neal."),
            change(2016, "Scores 60 in his final game."),
        ],
    },
    PlayerEra {
        name: "Steve Nash",
        nba: &[(1997, 2014)],
        aba: &[],
        summary: "Phoenix, Dallas, Phoenix again, the Lakers, 1996-97 to 2013-14; back-to-back MVPs in 2005 and 2006 running the seven-seconds-or-less Suns.",
        changes: &[
            change(1997, "Drafted by Phoenix; traded to Dallas in 1998."),
            change(2005, "Returns to Phoenix and wins the first of two straight MVPs; the Suns add 33 wins."),
            change(2012, "Traded to the Lakers; retires in 2014."),
        ],
    },
    PlayerEra {
        name: "Tim Duncan",
        nba: &[(1998, 2016)],
        aba: &[],
        summary: "San Antonio Spurs, 1997-98 to 2015-16; five titles, two MVPs, a playoff appearance every season.",
        changes: &[
            change(1998, "Rookie of the Year."),
            change(1999, "First title in the lockout season."),
            change(2014, "Fifth title, fifteen years after the first."),
        ],
    },
    PlayerEra {
        name: "Chauncey Billups",
        nba: &[(1998, 2014)],
        aba: &[],
        summary: "Boston, Toronto, Denver, Minnesota, Detroit, Denver again, New York, the Clippers, 1997-98 to 2013-14; the 2004 Finals MVP as Detroit upset the Lakers.",
        changes: &[
            change(2004, "Finals MVP; the Pistons beat the Shaq, Kobe, Malone and Payton Lakers in five."),
        ],
    },
    PlayerEra {
        name: "Dirk Nowitzki",
        nba: &[(1999, 2019)],
        aba: &[],
        summary: "Dallas Mavericks, 1998-99 to 2018-19; the 2007 MVP and the 2011 title, 21 seasons with one team.",
        changes: &[
            change(2007, "First European MVP."),
            change(2011, "Title over the Miami Big Three."),
        ],
    },
    PlayerEra {
        name: "Paul Pierce",
        nba: &[(1999, 2017)],
        aba: &[],
        summary: "Boston, Brooklyn, Washington, the Clippers, 1998-99 to 2016-17; the 2008 Finals MVP with the Big Three.",
        changes: &[
            change(2008, "Finals MVP against the Lakers; Boston's seventeenth title."),
        ],
    },
    PlayerEra {
        name: "Tony Parker",
        nba: &[(2002, 2019)],
        aba: &[],
        summary: "San Antonio Spurs 2001-02 to 2017-18, Charlotte 2018-19; four titles and the 2007 Finals MVP.",
        changes: &[
            change(2007, "Finals MVP in the sweep of Cleveland, the first European to win it."),
            change(2014, "Fourth title."),
        ],
    },
    PlayerEra {
        name: "Dwyane Wade",
        nba: &[(2004, 2019)],
        aba: &[],
        summary: "Miami Heat, with a Chicago and Cleveland interlude, 2003-04 to 2018-19; three titles and the 2006 Finals MVP.",
        changes: &[
            change(2006, "Finals MVP; Miami wins four straight after losing the first two."),
            change(2011, "The Big Three era begins; two more titles."),
        ],
    },
    PlayerEra {
        name: "LeBron James",
        nba: &[(2004, 2026)],
        aba: &[],
        summary: "Cleveland, Miami, Cleveland again, the Lakers, 2003-04 to the present; four titles, four MVPs, ten Finals, the career scoring leader.",
        changes: &[
            change(2004, "Rookie of the Year at 19."),
            change(2011, "Joins Miami; four straight Finals."),
            change(2016, "Cleveland's first title, from 3-1 down."),
            change(2020, "Fourth title with the Lakers in the bubble."),
            change(2023, "Passes Abdul-Jabbar as the career scoring leader."),
        ],
    },
    PlayerEra {
        name: "Andre Iguodala",
        nba: &[(2005, 2023)],
        aba: &[],
        summary: "Philadelphia, Denver, Golden State, Miami, Golden State again, 2004-05 to 2022-23; four titles and the 2015 Finals MVP.",
        changes: &[
            change(2015, "Finals MVP off the bench, guarding LeBron James."),
        ],
    },
    PlayerEra {
        name: "Kevin Durant",
        nba: &[(2008, 2026)],
        aba: &[],
        summary: "Seattle and Oklahoma City, Golden State, Brooklyn, Phoenix, 2007-08 to the present; two titles, the 2014 MVP.",
        changes: &[
            change(2008, "Rookie of the Year with the last Seattle team."),
            change(2014, "MVP."),
            change(2017, "Joins Golden State; two titles and two Finals MVPs."),
        ],
    },
    PlayerEra {
        name: "Derrick Rose",
        nba: &[(2009, 2024)],
        aba: &[],
        summary: "Chicago, New York, Cleveland, Minnesota, Detroit, New York again, Memphis, 2008-09 to 2023-24; the youngest MVP (2011) before the knee injuries.",
        changes: &[
            change(2009, "Rookie of the Year."),
            change(2011, "MVP at 22; the Bulls win 62."),
            change(2012, "Tears his ACL in the first playoff game."),
        ],
    },
    PlayerEra {
        name: "Russell Westbrook",
        nba: &[(2009, 2026)],
        aba: &[],
        summary: "Oklahoma City, Houston, Washington, the Lakers, the Clippers, Denver, Sacramento, 2008-09 to the present; the 2017 MVP, four triple-double seasons.",
        changes: &[
            change(2017, "MVP after averaging a triple-double, the first since Robertson."),
            change(2021, "Passes Robertson for the career triple-double record."),
        ],
    },
    PlayerEra {
        name: "James Harden",
        nba: &[(2010, 2026)],
        aba: &[],
        summary: "Oklahoma City, Houston, Brooklyn, Philadelphia, the Clippers, 2009-10 to the present; the 2018 MVP, three straight scoring titles.",
        changes: &[
            change(2012, "Sixth Man of the Year; traded to Houston that autumn."),
            change(2018, "MVP; the Rockets win 65 and take Golden State to seven games."),
        ],
    },
    PlayerEra {
        name: "Stephen Curry",
        nba: &[(2010, 2026)],
        aba: &[],
        summary: "Golden State Warriors, 2009-10 to the present; four titles, two MVPs (one unanimous), the three-point record.",
        changes: &[
            change(2015, "MVP and first title."),
            change(2016, "Unanimous MVP; 402 threes in a season."),
            change(2022, "Fourth title and first Finals MVP."),
        ],
    },
    PlayerEra {
        name: "Kawhi Leonard",
        nba: &[(2012, 2026)],
        aba: &[],
        summary: "San Antonio, Toronto, the Clippers, 2011-12 to the present; Finals MVP with two franchises (2014, 2019), two Defensive Player of the Year awards.",
        changes: &[
            change(2014, "Finals MVP at 22 as the Spurs beat Miami."),
            change(2019, "Finals MVP in Toronto's title season after the Game 7 buzzer-beater against Philadelphia."),
        ],
    },
    PlayerEra {
        name: "Giannis Antetokounmpo",
        nba: &[(2014, 2026)],
        aba: &[],
        summary: "Milwaukee Bucks, 2013-14 to the present; the 2021 title, two MVPs, Defensive Player of the Year.",
        changes: &[
            change(2019, "First of two straight MVPs."),
            change(2021, "50 points in the title-clinching game."),
        ],
    },
    PlayerEra {
        name: "Nikola Jokić",
        nba: &[(2016, 2026)],
        aba: &[],
        summary: "Denver Nuggets, 2015-16 to the present; the 2023 title, three MVPs, a second-round pick.",
        changes: &[
            change(2021, "First MVP."),
            change(2023, "Title and Finals MVP."),
        ],
    },
    PlayerEra {
        name: "Joel Embiid",
        nba: &[(2017, 2026)],
        aba: &[],
        summary: "Philadelphia 76ers, 2016-17 to the present after two injured seasons; the 2023 MVP, two scoring titles.",
        changes: &[
            change(2017, "Debuts two seasons after being drafted."),
            change(2023, "MVP after a second scoring title."),
        ],
    },
    PlayerEra {
        name: "Jaylen Brown",
        nba: &[(2017, 2026)],
        aba: &[],
        summary: "Boston Celtics, 2016-17 to the present; the 2024 Finals MVP.",
        changes: &[
            change(2024, "Finals MVP as Boston wins its eighteenth title."),
        ],
    },
    PlayerEra {
        name: "Shai Gilgeous-Alexander",
        nba: &[(2019, 2026)],
        aba: &[],
        summary: "The Clippers, then Oklahoma City, 2018-19 to the present; back-to-back MVPs (2025, 2026), the 2025 scoring title and Finals MVP.",
        changes: &[
            change(2025, "MVP, scoring champion and Finals MVP as the Thunder win the title."),
            change(2026, "Second straight MVP, ahead of Jokić and Wembanyama."),
        ],
    },
    PlayerEra {
        name: "Jalen Brunson",
        nba: &[(2019, 2026)],
        aba: &[],
        summary: "Dallas, then the New York Knicks, 2018-19 to the present; the 2026 Finals MVP as the Knicks won their first title in 53 years.",
        changes: &[
            change(2022, "Signs with the Knicks after four seasons in Dallas."),
            change(2026, "Finals MVP: 45 points in the clinching Game 5 against San Antonio, 32.6 a game for the series."),
        ],
    },
];

/// The players active in `season` of `league`, in table order.
pub fn active_in(league: League, season: SeasonYear) -> impl Iterator<Item = &'static PlayerEra> {
    PLAYER_ERAS.iter().filter(move |p| p.active(league, season))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_well_formed() {
        let mut names: Vec<&str> = PLAYER_ERAS.iter().map(|p| p.name).collect();
        names.sort_unstable();
        let n = names.len();
        names.dedup();
        assert_eq!(names.len(), n, "names are unique");
        for p in PLAYER_ERAS {
            assert!(
                !p.nba.is_empty() || !p.aba.is_empty(),
                "{} has seasons",
                p.name
            );
            for spans in [p.nba, p.aba] {
                assert!(
                    spans.iter().all(|&(a, b)| 1947 <= a && a <= b && b <= 2100),
                    "{}",
                    p.name
                );
                assert!(
                    spans.windows(2).all(|w| w[0].1 < w[1].0),
                    "{} spans in order",
                    p.name
                );
            }
            assert!(
                !p.changes.is_empty() && !p.summary.is_empty(),
                "{} has notes",
                p.name
            );
        }
        let firsts: Vec<i32> = PLAYER_ERAS.iter().map(|p| p.span().0.end_year()).collect();
        assert!(
            firsts.windows(2).all(|w| w[0] <= w[1]),
            "table in debut order"
        );
    }

    #[test]
    fn membership_follows_the_spans_and_the_league() {
        let jordan = PLAYER_ERAS
            .iter()
            .find(|p| p.name == "Michael Jordan")
            .unwrap();
        assert!(jordan.active(League::Nba, SeasonYear::of(1991)));
        assert!(
            !jordan.active(League::Nba, SeasonYear::of(1994)),
            "first retirement"
        );
        assert!(jordan.active(League::Nba, SeasonYear::of(2003)));
        assert!(!jordan.active(League::Aba, SeasonYear::of(1991)));
        let erving = PLAYER_ERAS
            .iter()
            .find(|p| p.name == "Julius Erving")
            .unwrap();
        assert!(erving.active(League::Aba, SeasonYear::of(1974)));
        assert!(!erving.active(League::Nba, SeasonYear::of(1974)));
        assert_eq!(erving.span(), (SeasonYear::of(1972), SeasonYear::of(1987)));
        let in_1987: Vec<&str> = active_in(League::Nba, SeasonYear::of(1987))
            .map(|p| p.name)
            .collect();
        assert!(
            in_1987.contains(&"Larry Bird")
                && in_1987.contains(&"Michael Jordan")
                && in_1987.contains(&"Julius Erving")
        );
        assert_eq!(active_in(League::Nba, SeasonYear::of(1948)).count(), 0);
    }
}
