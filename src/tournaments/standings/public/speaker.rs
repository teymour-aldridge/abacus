use axum::extract::Path;
use hypertext::prelude::*;

use crate::{
    auth::User,
    permission::TournamentAccess,
    state::Conn,
    template::{ActiveNav, Page},
    tournaments::{Tournament, standings::compute::speaker::SpeakerStandings},
    ui::{
        self, View,
        presentation::{Publication, publication},
    },
    util_resp::{StandardResponse, success},
};

pub async fn public_speaker_tab_page(
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
    // Team-tab publication and points-only standings do not publish speakers.
    let public = tournament.speaker_tab_public;
    let visible = public || access.is_superuser();
    let standings = if visible {
        Some(SpeakerStandings::compute(&tournament, &mut *conn)?)
    } else {
        None
    };
    let standings = standings.as_ref();
    let tid = tid.as_str();
    let view = ui::html(|ctx: &TournamentAccess, out| {
        maud! {
            div class="container py-4" {
                h1 { "Speaker standings" }
                nav class="d-flex gap-3 mb-3" aria-label="Standings navigation" {
                    a href=(format!("/tournaments/{tid}/tab/team")) { "Teams" }
                    a href=(format!("/tournaments/{tid}/tab/speaker")) aria-current="page" { "Speakers" }
                }
                @if ctx.is_superuser() {
                    (publication::<TournamentAccess>(
                        "Speaker tab",
                        if public { Publication::Public } else { Publication::Unpublished },
                    ).bind(ctx))
                }
                @if let Some(standings) = standings {
                    @if standings.rows.is_empty() {
                        p role="status" { "No speaker standings are available yet." }
                    } @else {
                        div class="table-responsive" {
                            table class="table" {
                                thead { tr {
                                    th scope="col" { "Rank" }
                                    th scope="col" { "Speaker name" }
                                    @for metric in &standings.metrics {
                                        th scope="col" { (metric.to_string()) }
                                    }
                                } }
                                tbody {
                                    @for row in &standings.rows {
                                        tr {
                                            th scope="row" { @if row.tied { "=" } (row.rank) }
                                            td { (row.name) }
                                            @for value in &row.metrics {
                                                td { (format!("{value:.2}")) }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } @else {
                    p role="status" { "Speaker standings are not yet public." }
                }
            }
        }.render_to(out)
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

#[cfg(test)]
mod tests {
    use diesel::{
        connection::SimpleConnection,
        r2d2::{ConnectionManager, Pool},
    };
    use diesel_migrations::MigrationHarness;

    fn database() -> crate::state::DbPool {
        let pool = Pool::builder()
            .max_size(1)
            .build(ConnectionManager::<diesel::SqliteConnection>::new(
                ":memory:",
            ))
            .unwrap();
        let mut conn = pool.get().unwrap();
        conn.run_pending_migrations(crate::MIGRATIONS).unwrap();
        conn.batch_execute(r#"
            INSERT INTO tournaments (
                id, name, abbrv, slug, created_at, teams_per_side,
                substantive_speakers, pool_ballot_setup, elim_ballot_setup,
                institution_penalty, history_penalty, pullup_metrics,
                repeat_pullup_penalty, team_standings_metrics, speaker_standings_metrics
            ) VALUES
                ('t', 'Test', 'T', 'test', CURRENT_TIMESTAMP, 1, 2,
                 'consensus', 'consensus', 0, 0, '[]', 0, '["wins"]', '["Avg","StdDev"]'),
                ('other', 'Other', 'O', 'other', CURRENT_TIMESTAMP, 1, 2,
                 'consensus', 'consensus', 0, 0, '[]', 0, '["wins"]', '["Avg"]');
            INSERT INTO users (id, email, username, password_hash, created_at) VALUES
                ('tab', 'tab@example.com', 'tabber', 'password', CURRENT_TIMESTAMP),
                ('ca', 'ca@example.com', 'caaaa', 'password', CURRENT_TIMESTAMP),
                ('equity', 'equity@example.com', 'equity', 'password', CURRENT_TIMESTAMP),
                ('outsider', 'outsider@example.com', 'outsider', 'password', CURRENT_TIMESTAMP),
                ('foreign', 'foreign@example.com', 'foreign', 'password', CURRENT_TIMESTAMP);
            INSERT INTO tournament_membership (tournament_id, user_id, role) VALUES
                ('t', 'tab', 'tab'), ('t', 'ca', 'ca'),
                ('t', 'equity', 'equity'), ('other', 'foreign', 'tab');
            INSERT INTO teams (id, tournament_id, name, number)
                VALUES ('team', 't', 'Team', 1);
            INSERT INTO speakers (id, tournament_id, name, email, private_url)
                VALUES ('speaker', 't', 'Secret <Speaker>', 'secret@example.com', 'private-token');
            INSERT INTO rounds (id, tournament_id, seq, name, kind, completed, draw_status)
                VALUES ('round', 't', 1, 'Round 1', 'P', 1, 'released_full');
            INSERT INTO debates (id, tournament_id, round_id, number, status)
                VALUES ('debate', 't', 'round', 1, 'confirmed');
            INSERT INTO agg_speaker_results_of_debate
                (id, tournament_id, debate_id, speaker_id, team_id, position, score)
                VALUES ('score', 't', 'debate', 'speaker', 'team', 0, 81.5);
        "#).unwrap();
        drop(conn);
        pool
    }

    #[tokio::test]
    async fn speaker_publication_and_tournament_scoped_permissions() {
        let pool = database();
        for user in [
            None,
            Some("tab"),
            Some("ca"),
            Some("equity"),
            Some("outsider"),
            Some("foreign"),
        ] {
            let mut server = axum_test::TestServer::new(
                crate::config::create_app(pool.clone()),
            )
            .unwrap();
            server.do_save_cookies();
            if let Some(user) = user {
                server
                    .post("/login")
                    .form(&[
                        ("id", format!("{user}@example.com")),
                        ("password", "password".into()),
                    ])
                    .await
                    .assert_status_see_other();
            }
            for speaker_public in [false, true] {
                for team_public in [false, true] {
                    for points_public in [false, true] {
                        pool.get().unwrap().batch_execute(&format!(
                            "UPDATE tournaments SET speaker_tab_public = {}, team_tab_public = {}, standings_public = {} WHERE id = 't';",
                            i32::from(speaker_public), i32::from(team_public), i32::from(points_public)
                        )).unwrap();
                        let response =
                            server.get("/tournaments/t/tab/speaker").await;
                        response.assert_status_ok();
                        let body = response.text();
                        let visible = speaker_public || user == Some("tab");
                        assert_eq!(
                            body.contains("Secret &lt;Speaker&gt;"),
                            visible,
                            "{user:?}, {speaker_public}, {team_public}, {points_public}"
                        );
                        assert_eq!(body.contains("81.50"), visible);
                        assert_eq!(
                            body.contains(
                                "Speaker standings are not yet public."
                            ),
                            !visible
                        );
                        assert!(!body.contains("secret@example.com"));
                        assert!(!body.contains("private-token"));
                        assert!(!body.contains("Secret <Speaker>"));
                        assert_eq!(
                            body.contains("Unpublished"),
                            !speaker_public && user == Some("tab")
                        );
                        assert_eq!(
                            response.headers()["cache-control"],
                            "private, no-store"
                        );
                    }
                }
            }
            server
                .get("/tournaments/missing/tab/speaker")
                .await
                .assert_status_not_found();
        }
    }

    #[tokio::test]
    async fn public_empty_standings_and_navigation() {
        let pool = database();
        pool.get()
            .unwrap()
            .batch_execute(
                "UPDATE tournaments SET speaker_tab_public = 1;
             DELETE FROM agg_speaker_results_of_debate;",
            )
            .unwrap();
        let server =
            axum_test::TestServer::new(crate::config::create_app(pool))
                .unwrap();
        let response = server.get("/tournaments/t/tab/speaker").await;
        response.assert_status_ok();
        assert!(
            response
                .text()
                .contains("No speaker standings are available yet.")
        );
        assert!(response.text().contains("/tournaments/t/tab/team"));
        let response = server.get("/tournaments/t/tab/team").await;
        response.assert_status_ok();
        assert!(response.text().contains("/tournaments/t/tab/speaker"));
    }
}
