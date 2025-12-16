//! Simulates rounds.

use std::collections::HashSet;

use abacus::{
    schema::{
        tournament_ballots, tournament_debate_judges, tournament_debates,
        tournament_judges, tournament_round_motions, tournament_rounds,
        tournament_speaker_score_entries, tournament_team_availability,
        tournament_teams, tournaments,
    },
    tournaments::{
        Tournament,
        participants::{DebateJudge, Judge},
        rounds::{
            Motion, Round, TournamentRounds,
            ballots::{
                BallotRepr,
                aggregate::{BallotAggregationMethod, aggregate_ballot_set},
            },
            draws::{
                Debate, DebateRepr,
                manage::drawalgs::{self, do_draw},
            },
        },
        teams::Team,
    },
};
use clap::Parser;
use diesel::{
    Connection,
    connection::{AnsiTransactionManager, LoadConnection, TransactionManager},
    prelude::*,
    sqlite::Sqlite,
};
use itertools::Itertools;
use rand::{Rng, distr::Uniform};
use uuid::Uuid;

#[derive(Parser)]
pub struct Import {
    #[clap(long, value_delimiter = ',')]
    rounds: Option<Vec<i64>>,
    #[clap(short, long)]
    database_url: Option<String>,
}

fn main() {
    let args = Import::parse();
    let db_url = if let Some(url) = args.database_url {
        url
    } else {
        std::env::var("DATABASE_URL").expect(
            "please either set `DATABASE_URL` or pass the `--database-url` flag",
        )
    };

    tracing_subscriber::fmt::init();

    let mut conn = diesel::SqliteConnection::establish(&db_url).unwrap();
    AnsiTransactionManager::begin_transaction(&mut conn).unwrap();

    let tournament = tournaments::table
        .filter(tournaments::name.eq("bp88team"))
        .first::<Tournament>(&mut conn)
        .expect("couldn't get test tournament (with name bp88team) -- have you imported the test data?");

    let all_tournament_rounds =
        TournamentRounds::fetch(&tournament.id, &mut conn).unwrap();

    let rounds_to_simulate_seqs = if let Some(mut r) = args.rounds {
        r.sort();
        for window in r.windows(2) {
            if window[1] != window[0] + 1 {
                panic!(
                    "Requested rounds must be consecutive. Found {} and {}",
                    window[0], window[1]
                );
            }
        }

        let first_seq = r[0];
        if first_seq > 1 {
            let prev_seq = first_seq - 1;
            let prev_rounds =
                Round::of_seq(prev_seq, &tournament.id, &mut conn);

            let all_prev_completed =
                prev_rounds.iter().all(|round| round.completed);
            if !all_prev_completed {
                panic!(
                    "Cannot simulate round {}: previous round {} is not completed.",
                    first_seq, prev_seq
                );
            }
        }

        Some(r.into_iter().collect::<HashSet<_>>())
    } else {
        None
    };

    for rounds in all_tournament_rounds.prelims_grouped_by_seq() {
        if let Some(ref target_seqs) = rounds_to_simulate_seqs {
            if rounds.is_empty() {
                continue;
            }
            if !target_seqs.contains(&rounds[0].seq) {
                continue;
            }
        }

        simulate_concurrent_in_rounds(
            &tournament,
            rounds.as_slice(),
            &mut conn,
        );
    }

    AnsiTransactionManager::commit_transaction(&mut conn).unwrap();
}

fn simulate_concurrent_in_rounds(
    tournament: &Tournament,
    rounds: &[Round],
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    tracing::info!(
        "Simulating rounds {}",
        rounds.iter().map(|round| round.name.clone()).join(",")
    );

    let teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(&tournament.id))
        .load::<Team>(conn)
        .unwrap();
    tracing::info!(
        "There are {} teams and {} rounds.",
        teams.len(),
        rounds.len()
    );

    let chunks = teams.iter().chunks(teams.len() / rounds.len());

    for (round, teams) in rounds.iter().zip(chunks.into_iter()) {
        for team in teams {
            diesel::insert_into(tournament_team_availability::table)
                .values((
                    tournament_team_availability::id
                        .eq(Uuid::now_v7().to_string()),
                    tournament_team_availability::round_id.eq(&round.id),
                    tournament_team_availability::team_id.eq(&team.id),
                    tournament_team_availability::available.eq(true),
                ))
                .on_conflict((
                    tournament_team_availability::round_id,
                    tournament_team_availability::team_id,
                ))
                .do_update()
                .set(tournament_team_availability::available.eq(true))
                .execute(conn)
                .unwrap();
        }
    }

    for round in rounds {
        do_draw(
            tournament.clone(),
            &round,
            Box::new(drawalgs::general::make_draw),
            conn,
            true,
        )
        .expect("failed to create draw");
    }

    for round in rounds {
        let debates = tournament_debates::table
            .filter(tournament_debates::round_id.eq(&round.id))
            .load::<Debate>(conn)
            .unwrap();
        let motion = tournament_round_motions::table
            .filter(tournament_round_motions::round_id.eq(&round.id))
            .first::<Motion>(conn)
            .unwrap();

        // Randomly allocate judges onto panels. Aims to assign three judges to
        // a panel.
        let _allocate_judges = for debate in debates.iter() {
            let free_judges = tournament_judges::table
                .filter(
                    tournament_judges::id.ne_all(
                        tournament_debate_judges::table
                            .filter(
                                tournament_debate_judges::debate_id.eq_any(
                                    tournament_debates::table
                                        .filter(
                                            tournament_debates::round_id
                                                .eq(&round.id),
                                        )
                                        .select(tournament_debates::id),
                                ),
                            )
                            .select(tournament_debate_judges::judge_id),
                    ),
                )
                .limit(3)
                .load::<Judge>(conn)
                .unwrap();

            for (i, j) in free_judges.iter().enumerate() {
                diesel::insert_into(tournament_debate_judges::table)
                    .values((
                        tournament_debate_judges::debate_id.eq(&debate.id),
                        tournament_debate_judges::judge_id.eq(&j.id),
                        tournament_debate_judges::status.eq(if i == 0 {
                            "C"
                        } else {
                            "P"
                        }),
                    ))
                    .execute(conn)
                    .unwrap();
            }
        };

        let _add_ballots = for debate in debates.iter() {
            let judges = tournament_debate_judges::table
                .filter(tournament_debate_judges::debate_id.eq(&debate.id))
                .load::<DebateJudge>(conn)
                .unwrap();

            let repr = DebateRepr::fetch(&debate.id, conn);

            let create_ballot_for_judge = |judge: &DebateJudge,
                                           speaks: &[i64],
                                           conn: &mut _|
             -> String {
                let ballot_id = Uuid::now_v7().to_string();
                diesel::insert_into(tournament_ballots::table)
                    .values((
                        tournament_ballots::id.eq(&ballot_id),
                        tournament_ballots::tournament_id.eq(&tournament.id),
                        tournament_ballots::debate_id.eq(&debate.id),
                        tournament_ballots::judge_id.eq(&judge.judge_id),
                        tournament_ballots::submitted_at.eq(diesel::dsl::now),
                        tournament_ballots::motion_id.eq(&motion.id),
                        tournament_ballots::version.eq(0),
                    ))
                    .execute(conn)
                    .unwrap();

                let mut speaker_scores = Vec::new();
                for i in 0..=1 {
                    for j in 0..=1 {
                        let team = repr.team_of_side_and_seq(i, j);
                        let speakers =
                            repr.speakers_of_team.get(&team.team_id).unwrap();
                        let first = &speakers[0];
                        let second = &speakers[1];

                        let (speaker_1_speak, speaker_2_speak) = match (i, j) {
                            (0, 0) => (speaks[0], speaks[1]),
                            (1, 0) => (speaks[2], speaks[3]),
                            (0, 1) => (speaks[4], speaks[5]),
                            (1, 1) => (speaks[6], speaks[7]),
                            // todo: WSDC
                            _ => unreachable!(),
                        };

                        speaker_scores.push((
                            tournament_speaker_score_entries::id
                                .eq(Uuid::now_v7().to_string()),
                            tournament_speaker_score_entries::ballot_id
                                .eq(&ballot_id),
                            tournament_speaker_score_entries::team_id
                                .eq(&team.team_id),
                            tournament_speaker_score_entries::speaker_id
                                .eq(first.id.clone()),
                            tournament_speaker_score_entries::speaker_position
                                .eq(0),
                            tournament_speaker_score_entries::score
                                .eq(speaker_1_speak as f32),
                        ));
                        speaker_scores.push((
                            tournament_speaker_score_entries::id
                                .eq(Uuid::now_v7().to_string()),
                            tournament_speaker_score_entries::ballot_id
                                .eq(&ballot_id),
                            tournament_speaker_score_entries::team_id
                                .eq(&team.team_id),
                            tournament_speaker_score_entries::speaker_id
                                .eq(second.id.clone()),
                            tournament_speaker_score_entries::speaker_position
                                .eq(1),
                            tournament_speaker_score_entries::score
                                .eq(speaker_2_speak as f32),
                        ));
                    }
                }

                diesel::insert_into(tournament_speaker_score_entries::table)
                    .values(speaker_scores)
                    .execute(conn)
                    .unwrap();
                ballot_id
            };

            let speaks = loop {
                let speaks: Vec<i64> = (0..8)
                    .map(|_| {
                        rand::rng()
                            .sample(Uniform::new_inclusive(50, 99).unwrap())
                    })
                    .collect();
                if speaks
                    .chunks(2)
                    .map(|pair| pair.iter().sum::<i64>())
                    .fold(HashSet::new(), |mut acc, sum| {
                        acc.insert(sum);
                        acc
                    })
                    .len()
                    == 4
                {
                    break speaks;
                }
            };

            let mut reprs = Vec::new();
            for judge in judges {
                let ballot_id =
                    (create_ballot_for_judge)(&judge, &speaks, conn);
                reprs.push(BallotRepr::fetch(&ballot_id, conn));
            }

            aggregate_ballot_set(
                &reprs,
                BallotAggregationMethod::Consensus,
                tournament,
                &repr,
                conn,
            );
        };

        diesel::update(
            tournament_rounds::table
                .filter(tournament_rounds::id.eq(&round.id)),
        )
        .set((
            tournament_rounds::completed.eq(true),
            tournament_rounds::draw_released_at.eq(diesel::dsl::now),
            // todo: the pages showing results should probably check if the
            // time set here is in the future
            //
            // users should not be able to set times in the future because it
            // will become very awkward (i.e. this is a time set internally
            // by the application when round results are published)
            tournament_rounds::results_published_at.eq(diesel::dsl::now),
        ))
        .execute(&mut *conn)
        .unwrap();
    }
}
