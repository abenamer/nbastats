//! Server-rendered pages (Askama). Pages are shells with selectors and chart containers;
//! data arrives from the JSON API.

use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::application::ports::GameLog;
use crate::config::APP_VERSION;
use crate::domain::ids::SeasonYear;
use crate::domain::models::{Franchise, Season};

use super::State as AppState;

/// Everything the base layout needs.
pub struct Layout {
    pub title: &'static str,
    pub active: &'static str,
    pub version: &'static str,
    /// Appended to the static URLs so a rebuilt script is fetched fresh.
    pub assets: &'static str,
}

impl Layout {
    fn new(title: &'static str, active: &'static str) -> Self {
        Self {
            title,
            active,
            version: APP_VERSION,
            assets: super::asset_version(),
        }
    }
}

fn render<T: Template>(t: T) -> Response {
    match t.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "template render failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response()
        }
    }
}

async fn season_list(app: &AppState) -> Vec<Season> {
    match app.store.seasons().await {
        Ok(s) => s.into_iter().rev().collect(),
        Err(e) => {
            tracing::warn!(error = %e, "season list unavailable");
            vec![]
        }
    }
}

async fn franchise_list(app: &AppState) -> Vec<Franchise> {
    match app.store.franchises(true).await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "franchise list unavailable");
            vec![]
        }
    }
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardPage {
    pub layout: Layout,
    pub seasons: Vec<Season>,
}

pub async fn dashboard(State(app): State<AppState>) -> Response {
    render(DashboardPage {
        layout: Layout::new("Dashboard", "dashboard"),
        seasons: season_list(&app).await,
    })
}

#[derive(Template)]
#[template(path = "elo.html")]
pub struct EloPage {
    pub layout: Layout,
    pub seasons: Vec<Season>,
    pub franchises: Vec<Franchise>,
}

pub async fn elo(State(app): State<AppState>) -> Response {
    render(EloPage {
        layout: Layout::new("Elo Explorer", "elo"),
        seasons: season_list(&app).await,
        franchises: franchise_list(&app).await,
    })
}

#[derive(Template)]
#[template(path = "franchise.html")]
pub struct FranchisePage {
    pub layout: Layout,
    pub franchises: Vec<Franchise>,
}

pub async fn franchise(State(app): State<AppState>) -> Response {
    // Every franchise, including the defunct ones: their histories are part of the chart.
    let franchises = match app.store.franchises(false).await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "franchise list unavailable");
            vec![]
        }
    };
    render(FranchisePage {
        layout: Layout::new("Franchise History", "franchise"),
        franchises,
    })
}

#[derive(Template)]
#[template(path = "records.html")]
pub struct RecordsPage {
    pub layout: Layout,
}

pub async fn records() -> Response {
    render(RecordsPage {
        layout: Layout::new("Elo Records", "records"),
    })
}

#[derive(Template)]
#[template(path = "alpha.html")]
pub struct AlphaPage {
    pub layout: Layout,
    pub franchises: Vec<Franchise>,
}

pub async fn alpha(State(app): State<AppState>) -> Response {
    render(AlphaPage {
        layout: Layout::new("Alpha Simulator", "alpha"),
        franchises: franchise_list(&app).await,
    })
}

#[derive(Template)]
#[template(path = "playoffs.html")]
pub struct PlayoffsPage {
    pub layout: Layout,
    pub seasons: Vec<Season>,
}

pub async fn playoffs(State(app): State<AppState>) -> Response {
    let seasons = season_list(&app)
        .await
        .into_iter()
        .filter(|s| s.season >= SeasonYear::FIRST_MODERN && s.playoff_game_count > 0)
        .collect();
    render(PlayoffsPage {
        layout: Layout::new("Playoff Simulator", "playoffs"),
        seasons,
    })
}

#[derive(Template)]
#[template(path = "season.html")]
pub struct SeasonPage {
    pub layout: Layout,
    pub seasons: Vec<Season>,
}

pub async fn season(State(app): State<AppState>) -> Response {
    let seasons = season_list(&app)
        .await
        .into_iter()
        .filter(|s| s.season >= SeasonYear::FIRST_MODERN)
        .collect();
    render(SeasonPage {
        layout: Layout::new("Season Forecast", "season"),
        seasons,
    })
}

#[derive(Template)]
#[template(path = "dynasty.html")]
pub struct DynastyPage {
    pub layout: Layout,
    pub franchises: Vec<Franchise>,
    pub seasons: Vec<Season>,
}

pub async fn dynasty(State(app): State<AppState>) -> Response {
    let seasons = season_list(&app)
        .await
        .into_iter()
        .filter(|s| s.season >= SeasonYear::FIRST_MODERN)
        .collect();
    render(DynastyPage {
        layout: Layout::new("Dynasty Laboratory", "dynasty"),
        franchises: franchise_list(&app).await,
        seasons,
    })
}

#[derive(Template)]
#[template(path = "validation.html")]
pub struct ValidationPage {
    pub layout: Layout,
    pub seasons: Vec<Season>,
}

pub async fn validation(State(app): State<AppState>) -> Response {
    let seasons = season_list(&app)
        .await
        .into_iter()
        .filter(|s| s.season >= SeasonYear::FIRST_MODERN && s.complete)
        .collect();
    render(ValidationPage {
        layout: Layout::new("Model Validation", "validation"),
        seasons,
    })
}

#[derive(Template)]
#[template(path = "data.html")]
pub struct DataPage {
    pub layout: Layout,
}

pub async fn data(State(_app): State<AppState>) -> Response {
    render(DataPage {
        layout: Layout::new("Data Management", "data"),
    })
}

#[derive(Template)]
#[template(path = "jobs.html")]
pub struct JobsPage {
    pub layout: Layout,
}

pub async fn jobs(State(_app): State<AppState>) -> Response {
    render(JobsPage {
        layout: Layout::new("Simulation Jobs", "jobs"),
    })
}
