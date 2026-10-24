use crate::{
    auth::User,
    permission::TournamentAccess,
    schema::{
        agg_team_results_of_debate, debates, judges, judges_of_debate, rooms,
        teams, teams_of_debate,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        rounds::{Round, side_names},
    },
    ui::{
        self, View,
        presentation::{Publication, action, publication},
    },
    util_resp::{StandardResponse, err_not_found, success},
};
use axum::extract::Path;
use diesel::prelude::*;
use hypertext::prelude::*;

struct ResultRow {
    room: Option<String>,
    teams: Vec<Option<(String, Option<i64>)>>,
    panel: Vec<String>,
}
struct ResultRound {
    name: String,
    public: bool,
    rows: Vec<ResultRow>,
}

pub async fn view_results_page(
    Path((tid, seq)): Path<(String, i64)>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let access = TournamentAccess::resolve(
        &tid,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    let rounds = crate::schema::rounds::table
        .filter(crate::schema::rounds::tournament_id.eq(&tid))
        .filter(crate::schema::rounds::seq.eq(seq))
        .load::<Round>(&mut *conn)?;
    if rounds.is_empty() {
        return err_not_found();
    }
    let columns = (tournament.teams_per_side * 2).max(0) as usize;
    let mut display = Vec::new();
    let mut hidden = false;
    for round in rounds {
        let public = tournament.show_round_results && round.is_results_public();
        if !public && !access.is_superuser() {
            hidden = true;
            continue;
        }
        let mut rows = Vec::new();
        let debate_rooms = debates::table
            .left_join(
                rooms::table.on(rooms::id
                    .nullable()
                    .eq(debates::room_id)
                    .and(rooms::tournament_id.eq(&tid))),
            )
            .filter(debates::tournament_id.eq(&tid))
            .filter(debates::round_id.eq(&round.id))
            .order(debates::id.asc())
            .select((debates::id, rooms::name.nullable()))
            .load::<(String, Option<String>)>(&mut *conn)?;
        for (id, room) in debate_rooms {
            let assigned = teams_of_debate::table
                .inner_join(
                    teams::table.on(teams::id.eq(teams_of_debate::team_id)),
                )
                .filter(teams_of_debate::tournament_id.eq(&tid))
                .filter(teams::tournament_id.eq(&tid))
                .filter(teams_of_debate::debate_id.eq(&id))
                .select((
                    teams::id,
                    teams::name,
                    teams_of_debate::seq,
                    teams_of_debate::side,
                ))
                .load::<(String, String, i64, i64)>(&mut *conn)?;
            let scores = agg_team_results_of_debate::table
                .filter(agg_team_results_of_debate::tournament_id.eq(&tid))
                .filter(agg_team_results_of_debate::debate_id.eq(&id))
                .select((
                    agg_team_results_of_debate::team_id,
                    agg_team_results_of_debate::points,
                ))
                .load::<(String, Option<i64>)>(&mut *conn)?;
            let mut team_columns = vec![None; columns];
            for (team_id, name, slot, side) in assigned {
                if slot >= 0 && (0..2).contains(&side) {
                    if let Some(cell) =
                        team_columns.get_mut((slot * 2 + side) as usize)
                    {
                        *cell = Some((
                            name,
                            scores
                                .iter()
                                .find(|(id, _)| id == &team_id)
                                .and_then(|(_, points)| *points),
                        ));
                    }
                }
            }
            let panel = judges_of_debate::table
                .inner_join(
                    judges::table.on(judges::id.eq(judges_of_debate::judge_id)),
                )
                .filter(judges_of_debate::tournament_id.eq(&tid))
                .filter(judges::tournament_id.eq(&tid))
                .filter(judges_of_debate::debate_id.eq(&id))
                .order(judges::name.asc())
                .select(judges::name)
                .load::<String>(&mut *conn)?;
            rows.push(ResultRow {
                room,
                teams: team_columns,
                panel,
            });
        }
        display.push(ResultRound {
            name: round.name,
            public,
            rows,
        });
    }
    let display = &display;
    let tid = tid.as_str();
    let tournament = &tournament;
    let view = ui::html(|ctx: &TournamentAccess, out| {
        let content = ui::protected(
            |_: &TournamentAccess| !display.is_empty(),
            || {
                ui::html(|ctx: &TournamentAccess, out| {
                    maud! {
            @for round in display {
                section class="my-4" {
                    h2 { (round.name) }
                    @if ctx.is_superuser() { (publication::<TournamentAccess>("Results",if round.public {Publication::Public}else{Publication::Unpublished}).bind(ctx)) }
                    table class="table" {
                        thead { tr { th scope="col" { "Room" } @for index in 0..columns { th scope="col" { (side_names::name_of_side(tournament,(index%2) as i64,(index/2) as i64,false)) } } th scope="col" { "Adjudicators" } } }
                        tbody { @for row in &round.rows { tr {
                            td { (row.room.as_deref().unwrap_or("TBA")) }
                            @for team in &row.teams { td { @if let Some((name,points))=team { (name) " " @if let Some(points)=points { span class="badge text-bg-secondary" { (points) } } } @else { "TBA" } } }
                            td { @for name in &row.panel { span class="badge text-bg-secondary me-1" { (name) } } }
                        } } }
                    }
                }
            }
        }.render_to(out)
                })
            },
            || {
                ui::html(|_: &TournamentAccess, out| {
                    maud! { p role="status" { "Round results are not yet public." } }.render_to(out)
                })
            },
        );
        maud! {
            div class="container py-4" {
                h1 { "Results" }
                (action(TournamentAccess::is_superuser,&format!("/tournaments/{tid}/rounds/{seq}/results/manage"),"Result controls",true).bind(ctx))
                @if hidden && !display.is_empty() { p role="status" { "Some results are not yet public." } }
                (content.bind(ctx))
                a href=(format!("/tournaments/{tid}/rounds/{seq}/draw")) { "Draw" }
            }
        }.render_to(out)
    });
    success(
        Page::new()
            .tournament((*tournament).clone())
            .user_opt(user)
            .body(view.bind(&access))
            .render(),
    )
}
