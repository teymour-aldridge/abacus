use crate::schema::*;
use axum::body::Bytes;
use axum_test::TestServer;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use fuzzcheck::DefaultMutator;
use serde::{Deserialize, Serialize};

macro_rules! get_id_by_idx {
    ($conn:expr, $query:expr, $idx:expr $(,)?) => {{
        let count = $query.count().get_result::<i64>($conn).unwrap_or(0);
        if count == 0 {
            None
        } else {
            let offset = ($idx as i64) % count;
            $query
                .offset(offset)
                .limit(1)
                .get_result::<String>($conn)
                .ok()
        }
    }};
}

#[derive(DefaultMutator, Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    // Auth
    RegisterUser {
        username: String,
        email: String,
        password: String,
    },
    LoginUser {
        user_idx: usize,
    },

    // Tournaments
    CreateTournament {
        name: String,
        abbrv: String,
        slug: String,
    },
    UpdateTournamentConfiguration {
        tournament_idx: usize,
        name: String,
        abbrv: String,
        slug: String,
    },

    // Participants
    CreateTeam {
        tournament_idx: usize,
        name: String,
        institution_idx: Option<usize>,
    },
    EditTeam {
        tournament_idx: usize,
        team_idx: usize,
        name: String,
        institution_idx: Option<usize>,
    },
    CreateSpeaker {
        tournament_idx: usize,
        team_idx: usize,
        name: String,
        email: String,
    },
    CreateJudge {
        tournament_idx: usize,
        name: String,
        email: String,
        institution_idx: Option<usize>,
    },
    EditJudge {
        tournament_idx: usize,
        judge_idx: usize,
        name: String,
        email: String,
        institution_idx: Option<usize>,
    },

    // Constraints
    AddConstraint {
        tournament_idx: usize,
        ptype: String,
        pid_idx: usize,
        category_idx: usize,
    },
    RemoveConstraint {
        tournament_idx: usize,
        ptype: String,
        pid_idx: usize,
        constraint_idx: usize,
    },

    // Rooms
    CreateRoom {
        tournament_idx: usize,
        name: String,
        priority: i64,
    },
    DeleteRoom {
        tournament_idx: usize,
        room_idx: usize,
    },
    CreateRoomCategory {
        tournament_idx: usize,
        name: String,
    },
    DeleteRoomCategory {
        tournament_idx: usize,
        category_idx: usize,
    },
    AddRoomToCategory {
        tournament_idx: usize,
        category_idx: usize,
        room_idx: usize,
    },
    RemoveRoomFromCategory {
        tournament_idx: usize,
        category_idx: usize,
        room_idx: usize,
    },

    // Feedback
    AddFeedbackQuestion {
        tournament_idx: usize,
        question_text: String,
        question_type: String,
    },
    DeleteFeedbackQuestion {
        tournament_idx: usize,
        question_idx: usize,
    },

    // Rounds & Draws
    CreateRound {
        tournament_idx: usize,
        name: String,
        category_idx: Option<usize>,
    },
    GenerateDraw {
        tournament_idx: usize,
        round_idx: usize,
    },
    SetDrawPublished {
        tournament_idx: usize,
        round_idx: usize,
        published: bool,
    },
    SetRoundCompleted {
        tournament_idx: usize,
        round_idx: usize,
        completed: bool,
    },
    PublishMotions {
        tournament_idx: usize,
        round_idx: usize,
    },
    PublishResults {
        tournament_idx: usize,
        round_idx: usize,
    },

    // Availability & Eligibility
    UpdateJudgeAvailability {
        tournament_idx: usize,
        round_idx: usize,
        judge_idx: usize,
        available: bool,
    },
    UpdateAllJudgeAvailability {
        tournament_idx: usize,
        round_idx: usize,
        available: bool,
    },
    UpdateTeamEligibility {
        tournament_idx: usize,
        round_idx: usize,
        team_idx: usize,
        eligible: bool,
    },
    UpdateAllTeamEligibility {
        tournament_idx: usize,
        round_idx: usize,
        eligible: bool,
    },

    // Ballots
    SubmitBallot {
        tournament_idx: usize,
        private_url_idx: usize,
        round_idx: usize,
        form: FuzzerBallotForm,
    },
}

#[derive(DefaultMutator, Clone, Debug, Serialize, Deserialize)]
pub struct FuzzerBallotForm {
    pub motion_idx: usize,
    pub teams: Vec<FuzzerBallotTeamEntry>,
    pub expected_version: i64,
}

#[derive(DefaultMutator, Clone, Debug, Serialize, Deserialize)]
pub struct FuzzerBallotTeamEntry {
    pub speaker_indices: Vec<(usize, Option<i32>)>,
    pub points: Option<usize>,
}

impl Action {
    #[tracing::instrument(skip_all)]
    pub async fn run(
        self,
        pool: &Pool<ConnectionManager<SqliteConnection>>,
        client: &mut TestServer,
    ) {
        tracing::info!("Running Action::run");
        match self {
            Action::RegisterUser {
                username,
                email,
                password,
            } => {
                let form = [
                    ("username", username),
                    ("email", email),
                    ("password", password),
                ];
                client.post("/register").form(&form).await;
            }
            Action::LoginUser { user_idx } => {
                let mut conn = pool.get().unwrap();
                if let Some(user_id) = get_id_by_idx!(
                    &mut *conn,
                    users::table.select(users::id).order_by(users::id),
                    user_idx,
                ) {
                    let (username, password): (String, String) = users::table
                        .filter(users::id.eq(user_id))
                        .select((users::username, users::password_hash))
                        .first(&mut *conn)
                        .unwrap();
                    drop(conn);

                    let form = [("username", username), ("password", password)];
                    client.post("/login").form(&form).await;
                }
            }
            Action::CreateTournament { name, abbrv, slug } => {
                let form = [("name", name), ("abbrv", abbrv), ("slug", slug)];
                let res = client.post("/tournaments/create").form(&form).await;
                assert!(
                    !res.status_code().is_server_error(),
                    "{:?}",
                    res.status_code()
                );
            }
            Action::UpdateTournamentConfiguration {
                tournament_idx,
                name,
                abbrv,
                slug,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    drop(conn);
                    let form =
                        [("name", name), ("abbrv", abbrv), ("slug", slug)];
                    client
                        .post(&format!("/tournaments/{}/configuration", tid))
                        .form(&form)
                        .await;
                }
            }
            Action::CreateTeam {
                tournament_idx,
                name,
                institution_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let inst_id = institution_idx
                        .and_then(|idx| {
                            get_id_by_idx!(
                                &mut *conn,
                                institutions::table
                                    .filter(
                                        institutions::tournament_id.eq(&tid),
                                    )
                                    .select(institutions::id)
                                    .order_by(institutions::id),
                                idx,
                            )
                        })
                        .unwrap_or_else(|| "-----".to_string());
                    drop(conn);

                    let form = [("name", name), ("institution_id", inst_id)];
                    client
                        .post(&format!("/tournaments/{}/teams/create", tid))
                        .form(&form)
                        .await;
                }
            }
            Action::EditTeam {
                tournament_idx,
                team_idx,
                name,
                institution_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(team_id) = get_id_by_idx!(
                        &mut *conn,
                        teams::table
                            .filter(teams::tournament_id.eq(&tid))
                            .select(teams::id)
                            .order_by(teams::id),
                        team_idx,
                    ) {
                        let inst_id = institution_idx
                            .and_then(|idx| {
                                get_id_by_idx!(
                                    &mut *conn,
                                    institutions::table
                                        .filter(
                                            institutions::tournament_id
                                                .eq(&tid),
                                        )
                                        .select(institutions::id)
                                        .order_by(institutions::id),
                                    idx,
                                )
                            })
                            .unwrap_or_else(|| "-----".to_string());
                        drop(conn);
                        let form =
                            [("name", name), ("institution_id", inst_id)];
                        client
                            .post(&format!(
                                "/tournaments/{}/teams/{}/edit",
                                tid, team_id
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::CreateJudge {
                tournament_idx,
                name,
                email,
                institution_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let inst_id = institution_idx
                        .and_then(|idx| {
                            get_id_by_idx!(
                                &mut *conn,
                                institutions::table
                                    .filter(
                                        institutions::tournament_id.eq(&tid),
                                    )
                                    .select(institutions::id)
                                    .order_by(institutions::id),
                                idx,
                            )
                        })
                        .unwrap_or_else(|| "-----".to_string());
                    drop(conn);

                    let form = [
                        ("name", name),
                        ("email", email),
                        ("institution_id", inst_id),
                    ];
                    client
                        .post(&format!("/tournaments/{}/judges/create", tid))
                        .form(&form)
                        .await;
                }
            }
            Action::EditJudge {
                tournament_idx,
                judge_idx,
                name,
                email,
                institution_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(judge_id) = get_id_by_idx!(
                        &mut *conn,
                        judges::table
                            .filter(judges::tournament_id.eq(&tid))
                            .select(judges::id)
                            .order_by(judges::id),
                        judge_idx,
                    ) {
                        let inst_id = institution_idx
                            .and_then(|idx| {
                                get_id_by_idx!(
                                    &mut *conn,
                                    institutions::table
                                        .filter(
                                            institutions::tournament_id
                                                .eq(&tid),
                                        )
                                        .select(institutions::id)
                                        .order_by(institutions::id),
                                    idx,
                                )
                            })
                            .unwrap_or_else(|| "-----".to_string());
                        drop(conn);
                        let form = [
                            ("name", name),
                            ("email", email),
                            ("institution_id", inst_id),
                        ];
                        client
                            .post(&format!(
                                "/tournaments/{}/judges/{}/edit",
                                tid, judge_id
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::CreateSpeaker {
                tournament_idx,
                team_idx,
                name,
                email,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(team_id) = get_id_by_idx!(
                        &mut *conn,
                        teams::table
                            .filter(teams::tournament_id.eq(&tid))
                            .select(teams::id)
                            .order_by(teams::id),
                        team_idx,
                    ) {
                        drop(conn);
                        let form = [("name", name), ("email", email)];
                        client
                            .post(&format!(
                                "/tournaments/{}/teams/{}/speakers/create",
                                tid, team_id
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::AddConstraint {
                tournament_idx,
                ptype,
                pid_idx,
                category_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let pid = if ptype == "speaker" {
                        get_id_by_idx!(
                            &mut *conn,
                            speakers::table
                                .filter(speakers::tournament_id.eq(&tid))
                                .select(speakers::id)
                                .order_by(speakers::id),
                            pid_idx,
                        )
                    } else {
                        get_id_by_idx!(
                            &mut *conn,
                            judges::table
                                .filter(judges::tournament_id.eq(&tid))
                                .select(judges::id)
                                .order_by(judges::id),
                            pid_idx,
                        )
                    };

                    if let (Some(pid), Some(cat_id)) = (
                        pid,
                        get_id_by_idx!(
                            &mut *conn,
                            break_categories::table
                                .filter(
                                    break_categories::tournament_id.eq(&tid),
                                )
                                .select(break_categories::id)
                                .order_by(break_categories::id),
                            category_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [("category_id", cat_id)];
                        client.post(&format!("/tournaments/{}/participants/{}/{}/constraints/add", tid, ptype, pid)).form(&form).await;
                    }
                }
            }
            Action::RemoveConstraint {
                tournament_idx,
                ptype,
                pid_idx,
                constraint_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let pid = if ptype == "speaker" {
                        get_id_by_idx!(
                            &mut *conn,
                            speakers::table
                                .filter(speakers::tournament_id.eq(&tid))
                                .select(speakers::id)
                                .order_by(speakers::id),
                            pid_idx,
                        )
                    } else {
                        get_id_by_idx!(
                            &mut *conn,
                            judges::table
                                .filter(judges::tournament_id.eq(&tid))
                                .select(judges::id)
                                .order_by(judges::id),
                            pid_idx,
                        )
                    };

                    if let Some(pid) = pid {
                        if ptype == "speaker" {
                            let query = speaker_room_constraints::table
                                .filter(
                                    speaker_room_constraints::speaker_id
                                        .eq(&pid),
                                )
                                .select(speaker_room_constraints::category_id)
                                .order_by(speaker_room_constraints::pref);

                            if let Some(cat_id) = get_id_by_idx!(
                                &mut *conn,
                                query,
                                constraint_idx
                            ) {
                                drop(conn);
                                let form = [("category_id", cat_id)];
                                client
                                    .post(&format!(
                                        "/tournaments/{}/participants/speaker/{}/constraints/remove",
                                        tid, pid
                                    ))
                                    .form(&form)
                                    .await;
                            }
                        } else {
                            let query = judge_room_constraints::table
                                .filter(
                                    judge_room_constraints::judge_id.eq(&pid),
                                )
                                .select(judge_room_constraints::category_id)
                                .order_by(judge_room_constraints::pref);

                            if let Some(cat_id) = get_id_by_idx!(
                                &mut *conn,
                                query,
                                constraint_idx
                            ) {
                                drop(conn);
                                let form = [("category_id", cat_id)];
                                client
                                    .post(&format!(
                                        "/tournaments/{}/participants/judge/{}/constraints/remove",
                                        tid, pid
                                    ))
                                    .form(&form)
                                    .await;
                            }
                        }
                    }
                }
            }
            Action::CreateRoom {
                tournament_idx,
                name,
                priority,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    drop(conn);
                    let form =
                        [("name", name), ("priority", priority.to_string())];
                    client
                        .post(&format!("/tournaments/{}/rooms/create", tid))
                        .form(&form)
                        .await;
                }
            }
            Action::DeleteRoom {
                tournament_idx,
                room_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(room_id) = get_id_by_idx!(
                        &mut *conn,
                        rooms::table
                            .filter(rooms::tournament_id.eq(&tid))
                            .select(rooms::id)
                            .order_by(rooms::id),
                        room_idx,
                    ) {
                        drop(conn);
                        client
                            .post(&format!(
                                "/tournaments/{}/rooms/{}/delete",
                                tid, room_id
                            ))
                            .await;
                    }
                }
            }
            Action::CreateRoomCategory {
                tournament_idx,
                name,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    drop(conn);
                    let form = [("name", name)];
                    client
                        .post(&format!(
                            "/tournaments/{}/rooms/categories/create",
                            tid
                        ))
                        .form(&form)
                        .await;
                }
            }
            Action::DeleteRoomCategory {
                tournament_idx,
                category_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(cat_id) = get_id_by_idx!(
                        &mut *conn,
                        room_categories::table
                            .filter(room_categories::tournament_id.eq(&tid))
                            .select(room_categories::id)
                            .order_by(room_categories::id),
                        category_idx,
                    ) {
                        drop(conn);
                        client
                            .post(&format!(
                                "/tournaments/{}/rooms/categories/{}/delete",
                                tid, cat_id
                            ))
                            .await;
                    }
                }
            }
            Action::AddRoomToCategory {
                tournament_idx,
                category_idx,
                room_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let (Some(cat_id), Some(room_id)) = (
                        get_id_by_idx!(
                            &mut *conn,
                            room_categories::table
                                .filter(room_categories::tournament_id.eq(&tid))
                                .select(room_categories::id)
                                .order_by(room_categories::id),
                            category_idx,
                        ),
                        get_id_by_idx!(
                            &mut *conn,
                            rooms::table
                                .filter(rooms::tournament_id.eq(&tid))
                                .select(rooms::id)
                                .order_by(rooms::id),
                            room_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [("room_id", room_id)];
                        client
                            .post(&format!(
                                "/tournaments/{}/rooms/categories/{}/add_room",
                                tid, cat_id
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::RemoveRoomFromCategory {
                tournament_idx,
                category_idx,
                room_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let (Some(cat_id), Some(room_id)) = (
                        get_id_by_idx!(
                            &mut *conn,
                            room_categories::table
                                .filter(room_categories::tournament_id.eq(&tid))
                                .select(room_categories::id)
                                .order_by(room_categories::id),
                            category_idx,
                        ),
                        get_id_by_idx!(
                            &mut *conn,
                            rooms::table
                                .filter(rooms::tournament_id.eq(&tid))
                                .select(rooms::id)
                                .order_by(rooms::id),
                            room_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [("room_id", room_id)];
                        client.post(&format!("/tournaments/{}/rooms/categories/{}/remove_room", tid, cat_id)).form(&form).await;
                    }
                }
            }
            Action::AddFeedbackQuestion {
                tournament_idx,
                question_text,
                question_type,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    drop(conn);
                    let form =
                        [("text", question_text), ("kind", question_type)];
                    client
                        .post(&format!(
                            "/tournaments/{}/feedback/manage/add",
                            tid
                        ))
                        .form(&form)
                        .await;
                }
            }
            Action::DeleteFeedbackQuestion {
                tournament_idx,
                question_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(q_id) = get_id_by_idx!(
                        &mut *conn,
                        feedback_questions::table
                            .filter(feedback_questions::tournament_id.eq(&tid))
                            .select(feedback_questions::id)
                            .order_by(feedback_questions::id),
                        question_idx,
                    ) {
                        drop(conn);
                        let form = [("id", q_id)];
                        client
                            .post(&format!(
                                "/tournaments/{}/feedback/manage/delete",
                                tid
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::CreateRound {
                tournament_idx,
                name,
                category_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(cat_id) = category_idx.and_then(|idx| {
                        get_id_by_idx!(
                            &mut *conn,
                            break_categories::table
                                .filter(
                                    break_categories::tournament_id.eq(&tid)
                                )
                                .select(break_categories::id)
                                .order_by(break_categories::id),
                            idx,
                        )
                    }) {
                        drop(conn);
                        let form = [("name", name)];
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/create",
                                tid, cat_id
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::GenerateDraw {
                tournament_idx,
                round_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/draws/create",
                                tid, rid
                            ))
                            .await;
                    }
                }
            }
            Action::SetDrawPublished {
                tournament_idx,
                round_idx,
                published,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        let form =
                            [("val", if published { "true" } else { "false" })];
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/draws/setreleased",
                                tid, rid
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::SetRoundCompleted {
                tournament_idx,
                round_idx,
                completed,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        let form =
                            [("val", if completed { "true" } else { "false" })];
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/complete",
                                tid, rid
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::PublishMotions {
                tournament_idx,
                round_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/motions/publish",
                                tid, rid
                            ))
                            .await;
                    }
                }
            }
            Action::PublishResults {
                tournament_idx,
                round_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/results/publish",
                                tid, rid
                            ))
                            .await;
                    }
                }
            }
            Action::UpdateJudgeAvailability {
                tournament_idx,
                round_idx,
                judge_idx,
                available,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let (Some(rid), Some(jid)) = (
                        get_id_by_idx!(
                            &mut *conn,
                            rounds::table
                                .filter(rounds::tournament_id.eq(&tid))
                                .select(rounds::id)
                                .order_by(rounds::id),
                            round_idx,
                        ),
                        get_id_by_idx!(
                            &mut *conn,
                            judges::table
                                .filter(judges::tournament_id.eq(&tid))
                                .select(judges::id)
                                .order_by(judges::id),
                            judge_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [
                            ("val", if available { "true" } else { "false" }),
                            ("judge_id", &jid),
                        ];
                        client.post(&format!("/tournaments/{}/rounds/{}/update_judge_availability", tid, rid)).form(&form).await;
                    }
                }
            }
            Action::UpdateAllJudgeAvailability {
                tournament_idx,
                round_idx,
                available,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        let form =
                            [("val", if available { "true" } else { "false" })];
                        client.post(&format!("/tournaments/{}/rounds/{}/availability/judges/all", tid, rid)).form(&form).await;
                    }
                }
            }
            Action::UpdateTeamEligibility {
                tournament_idx,
                round_idx,
                team_idx,
                eligible,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let (Some(rid), Some(team_id)) = (
                        get_id_by_idx!(
                            &mut *conn,
                            rounds::table
                                .filter(rounds::tournament_id.eq(&tid))
                                .select(rounds::id)
                                .order_by(rounds::id),
                            round_idx,
                        ),
                        get_id_by_idx!(
                            &mut *conn,
                            teams::table
                                .filter(teams::tournament_id.eq(&tid))
                                .select(teams::id)
                                .order_by(teams::id),
                            team_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [
                            ("val", if eligible { "true" } else { "false" }),
                            ("team_id", &team_id),
                        ];
                        client.post(&format!("/tournaments/{}/rounds/{}/update_team_eligibility", tid, rid)).form(&form).await;
                    }
                }
            }
            Action::UpdateAllTeamEligibility {
                tournament_idx,
                round_idx,
                eligible,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        let form =
                            [("val", if eligible { "true" } else { "false" })];
                        client.post(&format!("/tournaments/{}/rounds/{}/availability/teams/all", tid, rid)).form(&form).await;
                    }
                }
            }
            Action::SubmitBallot {
                tournament_idx,
                private_url_idx,
                round_idx,
                form: f_form,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let judges_with_urls: Vec<(String, String)> = judges::table
                        .filter(judges::tournament_id.eq(&tid))
                        .select((judges::id, judges::private_url))
                        .load(&mut *conn)
                        .unwrap();

                    if !judges_with_urls.is_empty() {
                        let (judge_id, private_url) = &judges_with_urls
                            [private_url_idx % judges_with_urls.len()];

                        if let Some(rid) = get_id_by_idx!(
                            &mut *conn,
                            rounds::table
                                .filter(rounds::tournament_id.eq(&tid))
                                .select(rounds::id)
                                .order_by(rounds::id),
                            round_idx,
                        ) {
                            let bytes = serialize_ballot_form(
                                &mut *conn, &tid, &rid, judge_id, f_form,
                            );
                            drop(conn);
                            client.post(&format!("/tournaments/{}/privateurls/{}/rounds/{}/submit", tid, private_url, rid)).bytes(Bytes::from(bytes)).await;
                        }
                    }
                }
            }
        }
    }
}

fn serialize_ballot_form(
    conn: &mut SqliteConnection,
    _tid: &str,
    rid: &str,
    _judge_id: &str,
    form: FuzzerBallotForm,
) -> Vec<u8> {
    use std::collections::HashMap;

    // We need to fetch the debate structure to know the teams and speakers
    let debate_info: Vec<(String, String, i64, i64)> = teams_of_debate::table
        .inner_join(debates::table)
        .filter(debates::round_id.eq(rid))
        .select((
            teams_of_debate::team_id,
            debates::id,
            teams_of_debate::side,
            teams_of_debate::seq,
        ))
        .load(conn)
        .unwrap();

    if debate_info.is_empty() {
        return Vec::new();
    }

    let m_ids: Vec<String> = motions_of_round::table
        .filter(motions_of_round::round_id.eq(rid))
        .select(motions_of_round::id)
        .order_by(motions_of_round::id)
        .load(conn)
        .unwrap();

    let mut query = HashMap::new();
    if !m_ids.is_empty() {
        query.insert(
            "motion_id".to_string(),
            m_ids[form.motion_idx % m_ids.len()].clone(),
        );
    }
    query.insert(
        "expected_version".to_string(),
        form.expected_version.to_string(),
    );

    for (i, team_entry) in form.teams.into_iter().enumerate() {
        if i >= debate_info.len() {
            break;
        }

        let team_id = &debate_info[i].0;

        if let Some(p) = team_entry.points {
            query.insert(format!("teams[{}][points]", i), p.to_string());
        }

        let team_speakers: Vec<String> = speakers_of_team::table
            .filter(speakers_of_team::team_id.eq(team_id))
            .select(speakers_of_team::speaker_id)
            .order_by(speakers_of_team::speaker_id)
            .load(conn)
            .unwrap();

        for (j, (s_idx, score)) in
            team_entry.speaker_indices.into_iter().enumerate()
        {
            if !team_speakers.is_empty() {
                let s_id = &team_speakers[s_idx % team_speakers.len()];
                query.insert(
                    format!("teams[{}][speakers][{}][id]", i, j),
                    s_id.clone(),
                );
                if let Some(sc) = score {
                    // Convert back to f32 for the form, assuming sc is e.g. score * 10
                    let sc_f = sc as f32 / 10.0;
                    query.insert(
                        format!("teams[{}][speakers][{}][score]", i, j),
                        sc_f.to_string(),
                    );
                }
            }
        }
    }

    serde_qs::to_string(&query).unwrap().into_bytes()
}
