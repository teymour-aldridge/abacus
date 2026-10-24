use crate::{
    auth::User,
    permission::TournamentAccess,
    state::Conn,
    template::{ActiveNav, Page},
    tournaments::{
        Tournament, config::RankableTeamMetric,
        standings::compute::TeamStandings,
    },
    ui::{
        self, View,
        presentation::{Publication, publication},
    },
    util_resp::{StandardResponse, success},
};
use axum::extract::Path;
use hypertext::prelude::*;
use rust_decimal::prelude::ToPrimitive;

struct StandingRow {
    rank: usize,
    tied: bool,
    name: String,
    metrics: Vec<String>,
}
pub async fn public_team_tab_page(
    Path(tid): Path<String>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let access = TournamentAccess::resolve(
        &tid,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    let full = access.is_superuser() || tournament.team_tab_public;
    let visible = full || tournament.standings_public;
    let mut headers = Vec::new();
    let mut rows = Vec::new();
    if visible {
        let standings = TeamStandings::recompute(&tid, &mut *conn);
        if full {
            headers =
                standings.metrics.iter().map(ToString::to_string).collect();
            let mut rank = 1;
            for group in &standings.teams_in_rank_order {
                for team in group {
                    rows.push(StandingRow {
                        rank,
                        tied: group.len() > 1,
                        name: team.name.clone(),
                        metrics: standings
                            .ranked_metrics_of_team
                            .get(&team.id)
                            .map(|metrics| {
                                metrics
                                    .iter()
                                    .map(|(_, value)| value.to_string())
                                    .collect()
                            })
                            .unwrap_or_else(|| vec!["0".into(); headers.len()]),
                    });
                }
                rank += group.len();
            }
        } else {
            headers.push("Points".into());
            let mut by_points = standings
                .teams_in_rank_order
                .iter()
                .flatten()
                .map(|team| {
                    let points = standings
                        .ranked_metrics_of_team
                        .get(&team.id)
                        .and_then(|metrics| {
                            metrics.iter().find(|(metric, _)| {
                                matches!(metric, RankableTeamMetric::Wins)
                            })
                        })
                        .and_then(|(_, value)| value.to_i64())
                        .unwrap_or(0);
                    (points, team.name.clone())
                })
                .collect::<Vec<_>>();
            by_points.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let mut start = 0;
            while start < by_points.len() {
                let end = start
                    + by_points[start..]
                        .iter()
                        .take_while(|(points, _)| *points == by_points[start].0)
                        .count();
                for (points, name) in &by_points[start..end] {
                    rows.push(StandingRow {
                        rank: start + 1,
                        tied: end - start > 1,
                        name: name.clone(),
                        metrics: vec![points.to_string()],
                    });
                }
                start = end;
            }
        }
    }
    let headers = &headers;
    let rows = &rows;
    let public = tournament.team_tab_public;
    let standings_public = tournament.standings_public || public;
    let view = ui::html(|ctx: &TournamentAccess, out| {
        let content = ui::protected(
            |_: &TournamentAccess| visible,
            || {
                ui::html(|_: &TournamentAccess, out| {
                    maud! {
            table class="table" {
                thead { tr { th scope="col" { "Rank" } th scope="col" { "Team name" } @for header in headers { th scope="col" { (header) } } } }
                tbody { @for row in rows { tr { th scope="row" { @if row.tied { "=" } (row.rank) } td { (row.name) } @for value in &row.metrics { td { (value) } } } } }
            }
        }.render_to(out)
                })
            },
            || {
                ui::html(|_: &TournamentAccess, out| {
                    maud! {p role="status" {"Team standings are not yet public."}}.render_to(out)
                })
            },
        );
        maud! {div class="container py-4" {
            h1 { "Team standings" }
            @if ctx.is_superuser() {
                (publication::<TournamentAccess>("Standings",if standings_public {Publication::Public}else{Publication::Unpublished}).bind(ctx))
                (publication::<TournamentAccess>("Full tab",if public {Publication::Public}else{Publication::Unpublished}).bind(ctx))
            }
            (content.bind(ctx))
        }}.render_to(out)
    });
    success(
        Page::new()
            .active_nav(ActiveNav::Standings)
            .tournament(tournament)
            .user_opt(user)
            .body(view.bind(&access))
            .render(),
    )
}
