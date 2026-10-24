use axum::extract::Path;
use diesel::prelude::*;
use hypertext::prelude::*;

use super::access::{DrawAccess, RoundDrawAccess};
use crate::{
    auth::User,
    schema::{
        debates, institutions, judges, judges_of_debate, teams, teams_of_debate,
    },
    state::Conn,
    template::Page,
    tournaments::{Tournament, rounds::Round},
    ui::{self, View},
    util_resp::{StandardResponse, success},
};

struct DrawRow {
    teams: Vec<Option<(String, String)>>,
    panel: Vec<(String, String)>,
}
struct DrawDisplay<'a> {
    round: &'a Round,
    rows: Vec<DrawRow>,
}
fn load_draw<'a>(
    round: &'a Round,
    access: RoundDrawAccess,
    team_columns: usize,
    conn: &mut diesel::SqliteConnection,
) -> Result<DrawDisplay<'a>, crate::util_resp::FailureResponse> {
    let mut rows = Vec::new();
    if access.can_see_draw() {
        let debate_ids = debates::table
            .filter(debates::round_id.eq(&round.id))
            .filter(debates::tournament_id.eq(&round.tournament_id))
            .order(debates::id.asc())
            .select(debates::id)
            .load::<String>(conn)?;
        for id in debate_ids {
            let teams = if access.can_see_teams() {
                let assigned = teams_of_debate::table
                    .inner_join(
                        teams::table.on(teams::id.eq(teams_of_debate::team_id)),
                    )
                    .left_join(
                        institutions::table.on(institutions::id
                            .nullable()
                            .eq(teams::institution_id)
                            .and(
                                institutions::tournament_id
                                    .eq(&round.tournament_id),
                            )),
                    )
                    .filter(teams_of_debate::debate_id.eq(&id))
                    .filter(
                        teams_of_debate::tournament_id.eq(&round.tournament_id),
                    )
                    .filter(teams::tournament_id.eq(&round.tournament_id))
                    .order((
                        teams_of_debate::seq.asc(),
                        teams_of_debate::side.asc(),
                    ))
                    .select((
                        teams::id,
                        teams::name,
                        institutions::code.nullable(),
                        teams_of_debate::seq,
                        teams_of_debate::side,
                    ))
                    .load::<(String, String, Option<String>, i64, i64)>(conn)?;
                let mut columns = vec![None; team_columns];
                for (id, name, institution, seq, side) in assigned {
                    let name = institution
                        .map(|code| format!("{code} {name}"))
                        .unwrap_or(name);
                    if seq >= 0 && (0..2).contains(&side) {
                        if let Some(slot) =
                            columns.get_mut((seq * 2 + side) as usize)
                        {
                            *slot = Some((id, name));
                        }
                    }
                }
                columns
            } else {
                Vec::new()
            };
            let panel = if access.can_see_panel() {
                judges_of_debate::table
                    .inner_join(
                        judges::table
                            .on(judges::id.eq(judges_of_debate::judge_id)),
                    )
                    .filter(judges_of_debate::debate_id.eq(&id))
                    .filter(
                        judges_of_debate::tournament_id
                            .eq(&round.tournament_id),
                    )
                    .filter(judges::tournament_id.eq(&round.tournament_id))
                    .order((
                        judges_of_debate::status.ne("C").asc(),
                        judges::name.asc(),
                    ))
                    .select((judges::name, judges_of_debate::status))
                    .load(conn)?
            } else {
                Vec::new()
            };
            rows.push(DrawRow { teams, panel });
        }
    }
    Ok(DrawDisplay { round, rows })
}

fn draw_view<'a>(
    draw: &'a DrawDisplay<'a>,
    tournament: &'a Tournament,
) -> impl View<DrawAccess> + 'a {
    ui::with_context(
        move |parent: &DrawAccess| parent.round(draw.round),
        ui::protected(
            RoundDrawAccess::can_see_draw,
            move || {
                ui::html(move |ctx: &RoundDrawAccess, out| {
                    let team_columns = ui::protected_or_empty(
                        RoundDrawAccess::can_see_teams,
                        || {
                            ui::html(|_: &RoundDrawAccess, out| {
                                maud! {
                                    @for i in 0..tournament.teams_per_side {
                                        th scope="col" { "Prop " (i + 1) }
                                        th scope="col" { "Opp " (i + 1) }
                                    }
                                }
                                .render_to(out)
                            })
                        },
                    );
                    let panel_column = ui::protected_or_empty(
                        RoundDrawAccess::can_see_panel,
                        || {
                            ui::html(|_: &RoundDrawAccess, out| {
                                maud! { th scope="col" { "Panel" } }
                                    .render_to(out)
                            })
                        },
                    );
                    let labels = ui::protected_or_empty(
                        RoundDrawAccess::annotate,
                        || {
                            ui::html(|ctx: &RoundDrawAccess, out| {
                                use ui::presentation::{
                                    Publication, publication,
                                };
                                publication::<RoundDrawAccess>(
                                    "Teams",
                                    if ctx.teams_public() {
                                        Publication::Public
                                    } else {
                                        Publication::Unpublished
                                    },
                                )
                                .render_to(ctx, out);
                                publication::<RoundDrawAccess>(
                                    "Panel",
                                    if ctx.panel_public() {
                                        Publication::Public
                                    } else {
                                        Publication::Unpublished
                                    },
                                )
                                .render_to(ctx, out);
                            })
                        },
                    );
                    maud! {
                    div class="card shadow-sm mb-4" {
                        div class="card-header bg-transparent py-3" {
                            h3 class="h5 mb-0 fw-bold" { (draw.round.name) }
                            (labels.bind(ctx))
                        }
                        div class="table-responsive" {
                            table class="table table-hover mb-0 align-middle" {
                                thead class="table-light" {
                                    tr {
                                        th scope="col" class="ps-3" { "#" }
                                        (team_columns.bind(ctx))
                                        (panel_column.bind(ctx))
                                    }
                                }
                                tbody {
                                    @for (i, row) in draw.rows.iter().enumerate() {
                                        @let team_cells = ui::protected_or_empty(RoundDrawAccess::can_see_teams, ||
                                            ui::html(|_: &RoundDrawAccess, out| maud! {
                                                @for team in &row.teams {
                                                    td {
                                                        @if let Some((id, name)) = team {
                                                        a href=(format!("/tournaments/{}/teams/{id}", tournament.id))
                                                            class="text-decoration-none fw-semibold text-dark" { (name) }
                                                        }
                                                    }
                                                }
                                            }.render_to(out)));
                                        @let panel_cells = ui::protected_or_empty(RoundDrawAccess::can_see_panel, ||
                                            ui::html(|_: &RoundDrawAccess, out| maud! {
                                                td class="pe-3" {
                                                    div class="d-flex flex-wrap gap-1" {
                                                        @for (name, status) in &row.panel {
                                                            span class="badge bg-light text-dark border fw-normal" {
                                                                (name) @if status == "C" { " (C)" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }.render_to(out)));
                                        tr {
                                            th scope="row" class="ps-3 text-secondary fw-normal" { (i + 1) }
                                            (team_cells.bind(ctx))
                                            (panel_cells.bind(ctx))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }.render_to(out);
                })
            },
            move || {
                ui::html(move |_: &RoundDrawAccess, out| {
                    maud! {
                        div class="alert alert-info" role="alert" {
                            "The draw is not yet available."
                        }
                    }
                    .render_to(out)
                })
            },
        ),
    )
}

pub async fn view_active_draw_page(
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let access = DrawAccess::resolve(&tournament, user.as_ref(), &mut *conn)?;
    let rounds = crate::schema::rounds::table
        .filter(crate::schema::rounds::tournament_id.eq(&tournament_id))
        .filter(crate::schema::rounds::seq.eq(round_seq))
        .load::<Round>(&mut *conn)?;
    if rounds.is_empty() {
        return crate::util_resp::err_not_found();
    }
    if (tournament.show_round_results || access.can_generate())
        && rounds.iter().all(|r| r.results_published_at.is_some())
    {
        return crate::util_resp::see_other_ok(axum::response::Redirect::to(
            &format!("/tournaments/{tournament_id}/rounds/{round_seq}/results"),
        ));
    }
    let draws = rounds
        .iter()
        .map(|round| {
            load_draw(
                round,
                access.round(round),
                (tournament.teams_per_side * 2) as usize,
                &mut conn,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn)
        .into_iter()
        .filter(|round| access.round(round).can_see_draw())
        .collect();
    let rounds = &rounds;
    let tournament_ref = &tournament;
    let view = ui::sequence((
        ui::html(|_: &DrawAccess, out| {
            maud! {
                div class="mb-4" {
                    h1 class="h2 mb-1 fw-bold" { "Draw" }
                }
            }
            .render_to(out)
        }),
        ui::html(|ctx: &DrawAccess, out| {
            let round_query = rounds
                .iter()
                .map(|round| format!("rounds={}", round.id))
                .collect::<Vec<_>>()
                .join("&");
            let allocator = format!(
                "/tournaments/{}/rounds/draws/edit?{round_query}",
                tournament_ref.id
            );
            let briefing = format!(
                "/tournaments/{}/rounds/{round_seq}/briefing",
                tournament_ref.id
            );
            maud! {
                div class="d-flex gap-2 mb-3" {
                    (ui::presentation::action(DrawAccess::can_allocate, &allocator, "Allocate judges and rooms", true).bind(ctx))
                    (ui::presentation::action(DrawAccess::can_generate, &briefing, "Publication and briefing", true).bind(ctx))
                }
            }.render_to(out);
        }),
        ui::for_each(&draws, |draw| {
            ui::sequence((
                draw_view(draw, tournament_ref),
                ui::html(move |ctx: &DrawAccess, out| {
                    let path = format!(
                        "/tournaments/{}/rounds/{}/draws/create",
                        tournament_ref.id, draw.round.id
                    );
                    maud! { div class="mb-3" {
                    (ui::presentation::action(DrawAccess::can_generate, &path,
                        if draw.round.draw_status == "none" { "Generate draw" } else { "Regenerate draw" }, true).bind(ctx))
                } }.render_to(out);
                }),
            ))
        }),
    ));
    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .active_nav(crate::template::ActiveNav::Draw)
            .body(maud! { div class="container py-4" { (view.bind(&access)) } })
            .render(),
    )
}
