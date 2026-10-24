use crate::{
    auth::User,
    permission::{Permission, TournamentAccess},
    schema::{motions_of_round, rounds},
    state::Conn,
    template::Page,
    tournaments::{Tournament, rounds::Round},
    ui::{
        self, View,
        presentation::{Publication, action, publication},
    },
    util_resp::{StandardResponse, success},
};
use axum::extract::Path;
use diesel::prelude::*;
use hypertext::prelude::*;

fn round_list(
    tournament: &Tournament,
    access: &TournamentAccess,
    seq: Option<i64>,
    conn: &mut diesel::SqliteConnection,
) -> Result<Vec<Round>, crate::util_resp::FailureResponse> {
    let mut query = rounds::table
        .filter(rounds::tournament_id.eq(&tournament.id))
        .into_boxed();
    if let Some(seq) = seq {
        query = query.filter(rounds::seq.eq(seq));
    }
    if !access.allows(Permission::ViewDraw) {
        let motion_rounds = motions_of_round::table
            .filter(motions_of_round::tournament_id.eq(&tournament.id))
            .filter(motions_of_round::published_at.is_not_null())
            .select(motions_of_round::round_id);
        query = query.filter(
            rounds::results_published_at
                .is_not_null()
                .and(
                    tournament
                        .show_round_results
                        .into_sql::<diesel::sql_types::Bool>(),
                )
                .or(rounds::draw_status
                    .eq_any(["released_teams", "released_full"])
                    .and(
                        tournament
                            .show_draws
                            .into_sql::<diesel::sql_types::Bool>(),
                    ))
                .or(rounds::id.eq_any(motion_rounds)),
        );
    }
    Ok(query
        .order((rounds::seq.asc(), rounds::name.asc()))
        .load(conn)?)
}

pub async fn view_tournament_page(
    Path(tid): Path<String>,
    user: Option<User<true>>,
    conn: Conn<true>,
) -> StandardResponse {
    overview(tid, None, user, conn).await
}
pub async fn view_round_list(
    Path(tid): Path<String>,
    user: Option<User<true>>,
    conn: Conn<true>,
) -> StandardResponse {
    overview(tid, None, user, conn).await
}
pub async fn view_round_page(
    Path((tid, seq)): Path<(String, i64)>,
    user: Option<User<true>>,
    conn: Conn<true>,
) -> StandardResponse {
    overview(tid, Some(seq), user, conn).await
}
async fn overview(
    tid: String,
    seq: Option<i64>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let access = TournamentAccess::resolve(
        &tid,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    if let Some(seq) = seq {
        if !diesel::select(diesel::dsl::exists(
            rounds::table
                .filter(rounds::tournament_id.eq(&tid))
                .filter(rounds::seq.eq(seq)),
        ))
        .get_result::<bool>(&mut *conn)?
        {
            return crate::util_resp::err_not_found();
        }
    }
    let rounds = round_list(&tournament, &access, seq, &mut conn)?;
    let summary = if access.is_superuser() {
        match seq {
            Some(seq) => Some(
                crate::tournaments::rounds::manage::setup::SetupSummary::load(
                    &tid, seq, &rounds, &mut conn,
                )?,
            ),
            None => None,
        }
    } else {
        None
    };
    let mut motion_query = motions_of_round::table
        .filter(motions_of_round::tournament_id.eq(&tid))
        .filter(motions_of_round::round_id.eq_any(rounds.iter().map(|r| &r.id)))
        .into_boxed();
    if !access.is_superuser() {
        motion_query =
            motion_query.filter(motions_of_round::published_at.is_not_null());
    }
    let motions = if seq.is_some() {
        motion_query
            .select((
                motions_of_round::round_id,
                motions_of_round::motion,
                motions_of_round::infoslide,
                motions_of_round::published_at.is_not_null(),
            ))
            .load::<(String, String, Option<String>, bool)>(&mut *conn)?
    } else {
        vec![]
    };
    let current = Round::current_rounds(&tid, &mut *conn)
        .into_iter()
        .filter(|r| rounds.iter().any(|shown| shown.id == r.id))
        .collect();
    let tid = tid.as_str();
    let tournament = &tournament;
    let rounds = &rounds;
    let motions = &motions;
    let summary = &summary;
    let view = ui::html(|ctx: &TournamentAccess, out| {
        let admin = ui::protected_or_empty(
            TournamentAccess::is_superuser,
            || {
                ui::html(|_: &TournamentAccess, out| {
                    maud! {
            div class="d-flex flex-wrap gap-2 mb-4" {
                (action(TournamentAccess::is_superuser,&format!("/tournaments/{tid}/rounds/create"),"Create round",true).bind(ctx))
                a class="btn btn-outline-primary" href=(format!("/tournaments/{tid}/configuration")) { "Configure tournament" }
                a class="btn btn-outline-primary" href=(format!("/tournaments/{tid}/participants/privateurls")) { "Private URLs" }
                a class="btn btn-outline-primary" href=(format!("/tournaments/{tid}/feedback/manage")) { "Feedback questions" }
                a class="btn btn-outline-primary" href=(format!("/tournaments/{tid}/rooms")) { "Rooms" }
                a class="btn btn-outline-primary" href=(format!("/tournaments/{tid}/categories")) { "Categories" }
            }
        }.render_to(out)
                })
            },
        );
        maud! {
            div class="container py-4" {
                h1 { @if seq.is_some() { "Round overview" } @else { (tournament.name) } }
                (admin.bind(ctx))
                nav class="d-flex flex-wrap gap-3 mb-4" {
                    a href=(format!("/tournaments/{tid}/rounds")) { "Rounds" }
                    a href=(format!("/tournaments/{tid}/participants")) { "Participants" }
                    a href=(format!("/tournaments/{tid}/tab/team")) { "Standings" }
                    a href=(format!("/tournaments/{tid}/motions")) { "Motions" }
                }
                @if rounds.is_empty() { p role="status" { "Round information is not yet public." } }
                @for round in rounds {
                    section class="card my-3" { div class="card-body" {
                        h2 class="h4" { a href=(format!("/tournaments/{tid}/rounds/{}",round.seq)) { (round.name) } }
                        div class="d-flex flex-wrap gap-2 mb-3" {
                            @if ctx.allows(Permission::ViewDraw) {
                                (publication::<TournamentAccess>("Teams",if tournament.show_draws && round.is_draw_public() {Publication::Public}else{Publication::Unpublished}).bind(ctx))
                                (publication::<TournamentAccess>("Panel",if tournament.show_draws && round.draw_status=="released_full" {Publication::Public}else{Publication::Unpublished}).bind(ctx))
                            }
                            @if ctx.is_superuser() { (publication::<TournamentAccess>("Results",if tournament.show_round_results && round.is_results_public(){Publication::Public}else{Publication::Unpublished}).bind(ctx)) }
                        }
                        div class="d-flex flex-wrap gap-2" {
                            a class="btn btn-outline-primary" href=(format!("/tournaments/{tid}/rounds/{}/draw",round.seq)) { "Draw" }
                            a class="btn btn-outline-primary" href=(format!("/tournaments/{tid}/rounds/{}/results",round.seq)) { "Results" }
                            (action(TournamentAccess::is_superuser,&format!("/tournaments/{tid}/rounds/{}/motions/create",round.id),"Add motion",true).bind(ctx))
                            (action(TournamentAccess::is_superuser,&format!("/tournaments/{tid}/rounds/{}/edit",round.id),"Edit round",true).bind(ctx))
                            (action(TournamentAccess::is_superuser,&format!("/tournaments/{tid}/rounds/{}/briefing",round.seq),"Publication and briefing",true).bind(ctx))
                            (action(TournamentAccess::is_superuser,&format!("/tournaments/{tid}/rounds/{}/ballots",round.seq),"Ballots",false).bind(ctx))
                            (action(TournamentAccess::is_superuser,&format!("/tournaments/{tid}/rounds/{}/results/manage",round.seq),"Result controls",true).bind(ctx))
                        }
                        @for (_,motion,infoslide,published) in motions.iter().filter(|(id,_,_,_)|id==&round.id) {
                            article class="mt-3" {
                                @if ctx.is_superuser() { (publication::<TournamentAccess>("Motion",if *published{Publication::Public}else{Publication::Unpublished}).bind(ctx)) }
                                p class="fw-semibold" { (motion) }
                                @if let Some(infoslide)=infoslide { p { (infoslide) } }
                            }
                        }
                    } }
                }
                @let operational=ui::protected_or_empty(TournamentAccess::is_superuser,||ui::html(|_:&TournamentAccess,out|maud! {
                    @if let Some(summary)=&summary { (summary) }
                }.render_to(out)));
                (operational.bind(ctx))
            }
        }.render_to(out);
    });
    success(
        Page::new()
            .tournament((*tournament).clone())
            .user_opt(user)
            .current_rounds(current)
            .body(view.bind(&access))
            .render(),
    )
}
