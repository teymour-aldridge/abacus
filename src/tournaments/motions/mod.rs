use crate::{
    auth::User,
    permission::TournamentAccess,
    schema::{motions_of_round, rounds},
    state::Conn,
    template::Page,
    tournaments::Tournament,
    ui::{
        self, View,
        presentation::{Publication, action, publication},
    },
    util_resp::{StandardResponse, success},
};
use axum::extract::Path;
use diesel::prelude::*;
use hypertext::prelude::*;

pub async fn public_motions_page(
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
    let mut query = motions_of_round::table
        .inner_join(rounds::table)
        .filter(motions_of_round::tournament_id.eq(&tid))
        .filter(rounds::tournament_id.eq(&tid))
        .into_boxed();
    if !access.is_superuser() {
        query = query.filter(motions_of_round::published_at.is_not_null());
    }
    let rows = query
        .order((rounds::seq.asc(), motions_of_round::id.asc()))
        .select((
            rounds::name,
            rounds::seq,
            motions_of_round::motion,
            motions_of_round::infoslide,
            motions_of_round::published_at.is_not_null(),
        ))
        .load::<(String, i64, String, Option<String>, bool)>(&mut *conn)?;
    let rows = &rows;
    let tid = tid.as_str();
    let view = ui::html(|ctx: &TournamentAccess, out| {
        maud! {
        div class="container py-4" {
            h1 { "Motions" }
            @if rows.is_empty() { p role="status" { "No motions have been released yet." } }
            @for (name, seq, motion, infoslide, published) in rows {
                section class="mb-4" {
                    h2 class="h4" { a href=(format!("/tournaments/{tid}/rounds/{seq}")) { (name) } }
                    @if ctx.is_superuser() {
                        (publication::<TournamentAccess>("Motion", if *published { Publication::Public } else { Publication::Unpublished }).bind(ctx))
                    }
                    p class="fw-semibold" { (motion) }
                    @if let Some(infoslide) = infoslide { p { (infoslide) } }
                    (action(TournamentAccess::is_superuser, &format!("/tournaments/{tid}/rounds/{seq}/briefing"), "Publication and briefing", true).bind(ctx))
                }
            }
        }
    }.render_to(out)
    });
    success(
        Page::new()
            .active_nav(crate::template::ActiveNav::Motions)
            .tournament(tournament)
            .user_opt(user)
            .body(view.bind(&access))
            .render(),
    )
}
