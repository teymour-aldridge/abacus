use std::collections::HashMap;
use std::panic::{UnwindSafe, catch_unwind};

use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::{connection::LoadConnection, sqlite::Sqlite};
use rand::SeedableRng;
use uuid::Uuid;

use crate::schema::{
    debates, judges_of_debate, rounds, team_availability, teams_of_debate,
    tickets_of_round,
};
use crate::tournaments::rounds::draws::manage::drawalgs::general::TeamsOfRoom;
use crate::tournaments::snapshots::take_snapshot;
use crate::tournaments::standings::compute::TeamStandings;
use crate::tournaments::standings::compute::history::TeamHistory;
use crate::{
    schema::teams,
    tournaments::{
        Tournament, config::RankableTeamMetric, rounds::Round, teams::Team,
    },
};

pub mod general;
pub mod random;

/// The error messages will be shown on the application page, and therefore
/// should be readable.
#[derive(Debug)]
pub enum MakeDrawError {
    InvalidConfiguration(String),
    InvalidTeamCount(String),
    AlreadyInProgress,
    TicketExpired,
    Panic,
}

pub struct DrawInput {
    pub tournament: Tournament,
    pub round: Round,
    /// Team metrics necessary to generate the draw.
    pub metrics: HashMap<(String, RankableTeamMetric), f32>,
    pub teams: Vec<Team>,
    pub standings: TeamStandings,
    pub history: TeamHistory,
    pub rng: rand_chacha::ChaCha20Rng,
}

/// Draws a round using the provided draw generation function.
///
/// **Important**: this function is long-running and should always be executed
/// on a background thread (i.e. not the async executor).
#[tracing::instrument(skip(draw_generator, conn))]
pub fn do_draw(
    tournament: Tournament,
    round: &Round,
    draw_generator: Box<
        dyn Fn(DrawInput) -> Result<Vec<TeamsOfRoom>, MakeDrawError>
            + UnwindSafe,
    >,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
    force: bool,
) -> Result<(), MakeDrawError> {
    let ticket_id = conn
        .transaction(|conn| -> Result<Result<_, _>, diesel::result::Error> {
            diesel::delete(
                tickets_of_round::table
                    .filter(tickets_of_round::released.eq(true)),
            )
            .execute(conn)
            .unwrap();

            let previous_ticket_seq = tickets_of_round::table
                .filter(
                    tickets_of_round::round_id
                        .eq(&round.id)
                        .and(tickets_of_round::kind.eq("draw"))
                        .and(tickets_of_round::released.eq(false)),
                )
                .select(diesel::dsl::max(tickets_of_round::seq))
                .get_result::<Option<i64>>(conn)
                .unwrap();

            if let Some(previous_ticket_seq) = previous_ticket_seq
                && force
            {
                let id = Uuid::now_v7().to_string();
                diesel::insert_into(tickets_of_round::table)
                    .values((
                        tickets_of_round::id.eq(&id),
                        tickets_of_round::round_id.eq(&round.id),
                        tickets_of_round::seq.eq(previous_ticket_seq + 1),
                        tickets_of_round::kind.eq("draw"),
                        tickets_of_round::acquired.eq(diesel::dsl::now),
                        tickets_of_round::released.eq(false),
                    ))
                    .execute(conn)
                    .unwrap();
                Ok(Ok(id))
            } else if previous_ticket_seq.is_some() && !force {
                return Ok(Err(MakeDrawError::AlreadyInProgress));
            } else {
                let id = Uuid::now_v7().to_string();
                diesel::insert_into(tickets_of_round::table)
                    .values((
                        tickets_of_round::id.eq(&id),
                        tickets_of_round::round_id.eq(&round.id),
                        tickets_of_round::seq.eq(0),
                        tickets_of_round::kind.eq("draw"),
                        tickets_of_round::acquired.eq(diesel::dsl::now),
                        tickets_of_round::released.eq(false),
                    ))
                    .execute(conn)
                    .unwrap();
                return Ok(Ok(id));
            }
        })
        .unwrap()?;

    tracing::info!("Obtained ticket {} for draw", ticket_id);

    let available_teams = teams::table
        .filter(teams::tournament_id.eq(&tournament.id))
        .inner_join(team_availability::table)
        .filter(
            team_availability::available
                .eq(true)
                .and(team_availability::round_id.eq(&round.id)),
        )
        .select(teams::all_columns)
        .load::<Team>(conn)
        .unwrap();

    tracing::info!("Found {} available teams", available_teams.len());

    let standings = TeamStandings::fetch(&tournament.id, conn);
    let history = TeamHistory::fetch(&tournament.id, conn);

    let rng = rand_chacha::ChaCha20Rng::from_os_rng();

    let input = DrawInput {
        tournament: tournament.clone(),
        round: round.clone(),
        // todo: compute the metrics
        metrics: HashMap::new(),
        teams: available_teams,
        rng,
        standings,
        history,
    };

    let generated = match catch_unwind(move || (draw_generator)(input)) {
        Ok(generated) => generated,
        Err(e) => {
            tracing::error!("Draw generator panicked: {e:?}");
            diesel::update(
                tickets_of_round::table
                    .filter(tickets_of_round::id.eq(&ticket_id)),
            )
            .set(tickets_of_round::released.eq(true))
            .execute(conn)
            .unwrap();
            return Err(MakeDrawError::Panic);
        }
    };

    let draw = match generated {
        Ok(generated) => generated,
        Err(failed) => {
            tracing::error!("Draw generator failed: {failed:?}");
            diesel::update(
                tickets_of_round::table
                    .filter(tickets_of_round::id.eq(&ticket_id)),
            )
            .set(tickets_of_round::released.eq(true))
            .execute(conn)
            .unwrap();

            return Err(failed);
        }
    };

    conn.transaction(
        |conn| -> Result<Result<(), MakeDrawError>, diesel::result::Error> {
            let (tickets1, tickets2) = diesel::alias!(
                tickets_of_round as tickets1,
                tickets_of_round as tickets2
            );
            let exists_active_ticket_with_higher_seq =
                diesel::dsl::select(diesel::dsl::exists(
                    tickets1
                        .filter(
                            tickets1.field(tickets_of_round::seq).gt(
                                tickets2
                                    .select(
                                        tickets2.field(
                                            tickets_of_round::seq,
                                        ),
                                    )
                                    .filter(
                                        tickets2
                                            .field(tickets_of_round::id)
                                            .eq(&ticket_id),
                                    )
                                    .into_boxed()
                                    .single_value()
                                    .assume_not_null(),
                            ),
                        )
                        .filter(
                            tickets1
                                .field(tickets_of_round::id)
                                .ne(&ticket_id),
                        ),
                ))
                .get_result::<bool>(conn)
                .unwrap();

            let response = if !exists_active_ticket_with_higher_seq {
                if force {
                    tracing::info!("Force flag set, deleting existing debates for round: {}", round.id);
                    diesel::delete(
                        judges_of_debate::table.filter(
                            judges_of_debate::debate_id.eq_any(
                                debates::table
                                    .filter(
                                        debates::round_id
                                            .eq(&round.id),
                                    )
                                    .select(debates::id),
                            ),
                        ),
                    )
                    .execute(conn)
                    .unwrap();
                    diesel::delete(
                        teams_of_debate::table.filter(
                            teams_of_debate::debate_id.eq_any(
                                debates::table
                                    .filter(
                                        debates::round_id
                                            .eq(&round.id),
                                    )
                                    .select(debates::id),
                            ),
                        ),
                    )
                    .execute(conn)
                    .unwrap();
                    diesel::delete(
                        debates::table
                            .filter(debates::round_id.eq(&round.id)),
                    )
                    .execute(conn)
                    .unwrap();
                }

                diesel::update(
                    rounds::table
                        .filter(rounds::id.eq(&round.id)),
                )
                .set((
                    rounds::draw_status.eq("draft"),
                    rounds::draw_released_at
                        .eq(None::<NaiveDateTime>),
                ))
                .execute(conn)
                .unwrap();

                let mut debates = Vec::new();
                let mut debate_teams = Vec::new();

                let mut debate_no = 1;
                for room in draw {
                    let debate_id = Uuid::now_v7().to_string();
                    debates.push((
                        debates::id.eq(debate_id.clone()),
                        debates::tournament_id.eq(&tournament.id),
                        debates::round_id.eq(&round.id),
                        debates::room_id.eq(None::<String>),
                        debates::status.eq("draft"),
                        debates::number.eq({
                            let ret = debate_no;
                            debate_no += 1;
                            ret
                        }),
                    ));
                    for (i, prop_team) in room.0.iter().enumerate() {
                        debate_teams.push((
                            teams_of_debate::id
                                .eq(Uuid::now_v7().to_string()),
                            teams_of_debate::debate_id
                                .eq(debate_id.clone()),
                            teams_of_debate::team_id
                                .eq(prop_team.id.clone()),
                            teams_of_debate::side.eq(0),
                            teams_of_debate::seq.eq(i as i64),
                            teams_of_debate::tournament_id.eq(tournament.id.clone())
                        ))
                    }
                    for (i, opp_team) in room.1.iter().enumerate() {
                        debate_teams.push((
                            teams_of_debate::id
                                .eq(Uuid::now_v7().to_string()),
                            teams_of_debate::debate_id
                                .eq(debate_id.clone()),
                            teams_of_debate::team_id
                                .eq(opp_team.id.clone()),
                            teams_of_debate::side.eq(1),
                            teams_of_debate::seq.eq(i as i64),
                            teams_of_debate::tournament_id.eq(tournament.id.clone())
                        ))
                    }
                }

                let n = diesel::insert_into(debates::table)
                    .values(&debates)
                    .execute(conn)
                    .unwrap();
                assert_eq!(n, debates.len());

                let n = diesel::insert_into(teams_of_debate::table)
                    .values(&debate_teams)
                    .execute(conn)
                    .unwrap();
                assert_eq!(n, debate_teams.len());

                diesel::update(
                    tickets_of_round::table
                        .filter(tickets_of_round::id.eq(&ticket_id)),
                )
                .set(tickets_of_round::released.eq(true))
                .execute(conn)
                .unwrap();

                Ok(Ok(()))
            } else {
                Ok(Err(MakeDrawError::TicketExpired))
            };

            diesel::update(
                tickets_of_round::table
                    .filter(tickets_of_round::id.eq(&ticket_id)),
            )
            .set(tickets_of_round::released.eq(true))
            .execute(conn)
            .unwrap();

            tracing::info!("Released ticket {} for draw", ticket_id);

            take_snapshot(&tournament.id, conn);

            response
        },
    )
    .unwrap()
}
