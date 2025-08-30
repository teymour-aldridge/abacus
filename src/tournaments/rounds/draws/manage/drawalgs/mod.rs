use std::collections::HashMap;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::{Connection, connection::LoadConnection, sqlite::Sqlite};
use rand::SeedableRng;
use uuid::Uuid;

use crate::schema::{
    tournament_debate_teams, tournament_debates, tournament_draws,
    tournament_round_tickets, tournament_team_availability,
};
use crate::tournaments::snapshots::take_snapshot;
use crate::{
    schema::tournament_teams,
    tournaments::{Tournament, config::TeamMetric, rounds::Round, teams::Team},
};

pub mod random;

/// The error messages will be shown on the application page, and therefore
/// should be readable.
pub enum MakeDrawError {
    InvalidConfiguration(String),
    InvalidTeamCount(String),
    AlreadyInProgress,
    TicketExpired,
}

pub struct DrawInput {
    pub tournament: Tournament,
    pub round: Round,
    /// Team metrics necessary to generate the draw.
    pub metrics: HashMap<(String, TeamMetric), f32>,
    pub teams: Vec<Team>,
    pub rng: rand_chacha::ChaCha20Rng,
}

/// Draws a round using the provided draw generation function.
///
/// **Important**: this function is long-running and should always be executed
/// on a background thread (i.e. not the async executor).
pub fn do_draw(
    tournament: Tournament,
    round: Round,
    draw_generator: Box<
        dyn Fn(DrawInput) -> Result<Vec<(Vec<Team>, Vec<Team>)>, MakeDrawError>,
    >,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    override_prior_ticket: bool,
) -> Result<String, MakeDrawError> {
    let ticket_id = conn
        .transaction(|conn| -> Result<Result<_, _>, diesel::result::Error> {
            let previous_ticket_seq = tournament_round_tickets::table
                .filter(
                    tournament_round_tickets::round_id
                        .eq(&round.id)
                        .and(tournament_round_tickets::kind.eq("draw"))
                        .and(tournament_round_tickets::released.eq(false)),
                )
                .select(diesel::dsl::max(tournament_round_tickets::seq))
                .get_result::<Option<i64>>(conn)
                .unwrap();

            if let Some(previous_ticket_seq) = previous_ticket_seq
                && override_prior_ticket
            {
                let id = Uuid::now_v7().to_string();
                diesel::insert_into(tournament_round_tickets::table)
                    .values((
                        tournament_round_tickets::id.eq(&id),
                        tournament_round_tickets::round_id.eq(&round.id),
                        tournament_round_tickets::seq
                            .eq(previous_ticket_seq + 1),
                        tournament_round_tickets::kind.eq("draw"),
                        tournament_round_tickets::acquired.eq(diesel::dsl::now),
                        tournament_round_tickets::released.eq(false),
                    ))
                    .execute(conn)
                    .unwrap();
                return Ok(Ok(id));
            } else if previous_ticket_seq.is_some() && !override_prior_ticket {
                return Ok(Err(MakeDrawError::AlreadyInProgress));
            } else {
                let id = Uuid::now_v7().to_string();
                diesel::insert_into(tournament_round_tickets::table)
                    .values((
                        tournament_round_tickets::id.eq(&id),
                        tournament_round_tickets::round_id.eq(&round.id),
                        tournament_round_tickets::seq.eq(0),
                        tournament_round_tickets::kind.eq("draw"),
                        tournament_round_tickets::acquired.eq(diesel::dsl::now),
                        tournament_round_tickets::released.eq(false),
                    ))
                    .execute(conn)
                    .unwrap();
                return Ok(Ok(id));
            }
        })
        .unwrap()?;

    let available_teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(&tournament.id))
        .inner_join(tournament_team_availability::table)
        .filter(tournament_team_availability::available.eq(true))
        .select(tournament_teams::all_columns)
        .load::<Team>(conn)
        .unwrap();

    let rng = rand_chacha::ChaCha20Rng::from_os_rng();

    let input = DrawInput {
        tournament: tournament.clone(),
        round: round.clone(),
        // todo: compute the metrics
        metrics: HashMap::new(),
        teams: available_teams,
        rng,
    };

    let draw = (draw_generator)(input)?;

    conn.transaction(
        |conn| -> Result<Result<String, MakeDrawError>, diesel::result::Error> {
            let (tickets1, tickets2) = diesel::alias!(
                tournament_round_tickets as tickets1,
                tournament_round_tickets as tickets2
            );
            let ticket_valid = diesel::dsl::select(diesel::dsl::exists(
                tickets1.filter(
                    tickets1.field(tournament_round_tickets::seq).gt(tickets2
                        .select(tickets2.field(tournament_round_tickets::seq))
                        .filter(
                            tickets2
                                .field(tournament_round_tickets::id)
                                .eq(&ticket_id),
                        )
                        .into_boxed()
                        .single_value()
                        .assume_not_null()),
                ),
            ))
            .get_result::<bool>(conn)
            .unwrap();

            let response = if ticket_valid {
                let draw_id = Uuid::now_v7().to_string();
                diesel::insert_into(tournament_draws::table)
                    .values((
                        tournament_draws::id.eq(&draw_id),
                        tournament_draws::tournament_id.eq(&tournament.id),
                        tournament_draws::round_id.eq(&round.id),
                        tournament_draws::status.eq("D"),
                        tournament_draws::released_at.eq(None::<NaiveDateTime>),
                    ))
                    .execute(conn)
                    .unwrap();

                let mut debates = Vec::new();
                let mut debate_teams = Vec::new();

                for room in draw {
                    let debate_id = Uuid::now_v7().to_string();
                    debates.push((
                        tournament_debates::id.eq(debate_id.clone()),
                        tournament_debates::tournament_id.eq(&tournament.id),
                        tournament_debates::draw_id.eq(&draw_id),
                        tournament_debates::room_id.eq(None::<String>),
                    ));
                    for (i, prop_team) in room.0.iter().enumerate() {
                        debate_teams.push((
                            tournament_debate_teams::id
                                .eq(Uuid::now_v7().to_string()),
                            tournament_debate_teams::debate_id
                                .eq(debate_id.clone()),
                            tournament_debate_teams::team_id
                                .eq(prop_team.id.clone()),
                            tournament_debate_teams::side.eq(0),
                            tournament_debate_teams::seq.eq(i as i64),
                        ))
                    }
                    for (i, opp_team) in room.1.iter().enumerate() {
                        debate_teams.push((
                            tournament_debate_teams::id
                                .eq(Uuid::now_v7().to_string()),
                            tournament_debate_teams::debate_id
                                .eq(debate_id.clone()),
                            tournament_debate_teams::team_id
                                .eq(opp_team.id.clone()),
                            tournament_debate_teams::side.eq(1),
                            tournament_debate_teams::seq.eq(i as i64),
                        ))
                    }
                }

                diesel::insert_into(tournament_debates::table)
                    .values(&debates)
                    .execute(conn)
                    .unwrap();

                diesel::insert_into(tournament_debate_teams::table)
                    .values(&debate_teams)
                    .execute(conn)
                    .unwrap();

                diesel::update(
                    tournament_round_tickets::table
                        .filter(tournament_round_tickets::id.eq(&ticket_id)),
                )
                .set(tournament_round_tickets::released.eq(true))
                .execute(conn)
                .unwrap();

                Ok(Ok(draw_id))
            } else {
                Ok(Err(MakeDrawError::TicketExpired))
            };

            diesel::update(
                tournament_round_tickets::table
                    .filter(tournament_round_tickets::id.eq(&ticket_id)),
            )
            .set(tournament_round_tickets::released.eq(true))
            .execute(conn)
            .unwrap();

            take_snapshot(&tournament.id, conn);

            response
        },
    )
    .unwrap()
}
