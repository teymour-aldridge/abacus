use std::fs::File;

use abacus::MIGRATIONS;
use abacus::schema::{
    tournament_break_categories, tournament_institutions, tournament_judges,
    tournament_members, tournament_round_tickets, tournament_rounds,
    tournament_speakers, tournament_team_speakers, tournament_teams,
    tournaments, users,
};
use abacus::tournaments::config::{
    PullupMetric, RankableTeamMetric, SpeakerMetric,
};
use argon2::Argon2;
use argon2::PasswordHasher;
use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use chrono::{NaiveDateTime, Utc};
use clap::Parser;
use diesel::dsl::now;
use diesel::prelude::*;
use diesel::{Connection, QueryDsl, RunQueryDsl};
use diesel_migrations::MigrationHarness;
use uuid::Uuid;

use crate::tabbycat_cli_copied::{JudgeRow, TeamRow};

#[derive(Parser)]
pub struct Import {
    database_url: Option<String>,
    #[clap(long, short, action)]
    teams: bool,
    #[clap(long, short, action)]
    judges: bool,
    #[clap(long, short, action)]
    rounds: bool,
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

    let mut conn = diesel::SqliteConnection::establish(&db_url).unwrap();

    conn.run_pending_migrations(MIGRATIONS).unwrap();

    let user_id = if users::table
        .filter(users::username.eq("admin"))
        .count()
        .get_result::<i64>(&mut conn)
        .unwrap()
        == 0
    {
        let uid = Uuid::now_v7().to_string();

        diesel::insert_into(users::table)
            .values((
                users::id.eq(&uid),
                users::email.eq("test@example.com"),
                users::username.eq("admin"),
                users::password_hash.eq({
                    let salt = SaltString::generate(&mut OsRng);

                    let argon2 = Argon2::default();

                    let password_hash = argon2
                        .hash_password("password".as_bytes(), &salt)
                        .unwrap()
                        .to_string();

                    password_hash
                }),
                users::created_at.eq(now),
            ))
            .execute(&mut conn)
            .unwrap();

        uid
    } else {
        users::table
            .filter(users::username.eq("admin"))
            .select(users::id)
            .first::<String>(&mut conn)
            .unwrap()
    };

    if tournaments::table
        .filter(tournaments::name.eq("bp88team"))
        .count()
        .get_result::<i64>(&mut conn)
        .unwrap()
        > 0
    {
        panic!("bp88team tournament already exists!")
    };

    let tournament_id = Uuid::now_v7().to_string();
    diesel::insert_into(tournaments::table)
        .values((
            tournaments::id.eq(&tournament_id),
            tournaments::name.eq("bp88team"),
            tournaments::abbrv.eq("bp88team"),
            tournaments::slug.eq("bp88team"),
            tournaments::created_at.eq(Utc::now().naive_utc()),
            tournaments::teams_per_side.eq(2),
            tournaments::substantive_speakers.eq(2),
            tournaments::reply_speakers.eq(false),
            tournaments::reply_must_speak.eq(true),
            tournaments::max_substantive_speech_index_for_reply.eq(2),
            tournaments::pool_ballot_setup.eq("consensus"),
            tournaments::elim_ballot_setup.eq("consensus"),
            tournaments::elim_ballots_require_speaks.eq(false),
            tournaments::institution_penalty.eq(0),
            tournaments::history_penalty.eq(0),
            tournaments::team_standings_metrics.eq(serde_json::to_string(&[
                RankableTeamMetric::Wins,
                RankableTeamMetric::NTimesAchieved(3),
                RankableTeamMetric::NTimesAchieved(2),
                RankableTeamMetric::NTimesAchieved(1),
                RankableTeamMetric::DrawStrengthByWins,
            ])
            .unwrap()),
            tournaments::speaker_standings_metrics.eq(serde_json::to_string(
                &[SpeakerMetric::Avg, SpeakerMetric::StdDev],
            )
            .unwrap()),
            tournaments::pullup_metrics
                .eq(serde_json::to_string(&[PullupMetric::Random]).unwrap()),
            tournaments::repeat_pullup_penalty.eq(0),
            tournaments::exclude_from_speaker_standings_after.eq(-1),
        ))
        .execute(&mut conn)
        .unwrap();

    diesel::insert_into(tournament_members::table)
        .values((
            tournament_members::id.eq(Uuid::now_v7().to_string()),
            tournament_members::user_id.eq(user_id),
            tournament_members::tournament_id.eq(&tournament_id),
            tournament_members::is_superuser.eq(true),
        ))
        .execute(&mut conn)
        .unwrap();

    if args.teams {
        let mut teams =
            csv::Reader::from_reader(File::open("src/bin/teams.csv").unwrap());
        let headers = teams.headers().unwrap().clone();

        for (i, result) in teams.records().enumerate() {
            let team = result.unwrap();
            let team: TeamRow = team.deserialize(Some(&headers)).unwrap();

            let team_id = Uuid::now_v7().to_string();

            diesel::insert_into(tournament_teams::table)
                .values((
                    tournament_teams::id.eq(&team_id),
                    tournament_teams::tournament_id.eq(&tournament_id),
                    tournament_teams::name.eq(&team.full_name),
                    tournament_teams::institution_id.eq(
                        get_or_create_institution(
                            &mut conn,
                            &tournament_id,
                            &team.institution,
                        ),
                    ),
                    tournament_teams::number.eq(i as i64),
                ))
                .execute(&mut conn)
                .unwrap();

            for speaker in team.speakers {
                let speaker_id = Uuid::now_v7().to_string();

                diesel::insert_into(tournament_speakers::table)
                    .values((
                        tournament_speakers::id.eq(&speaker_id),
                        tournament_speakers::tournament_id.eq(&tournament_id),
                        tournament_speakers::name.eq(&speaker.name),
                        tournament_speakers::email.eq(&speaker.email.unwrap()),
                        tournament_speakers::private_url
                            .eq(Uuid::new_v4().to_string()),
                    ))
                    .execute(&mut conn)
                    .unwrap();

                diesel::insert_into(tournament_team_speakers::table)
                    .values((
                        tournament_team_speakers::team_id.eq(&team_id),
                        tournament_team_speakers::speaker_id.eq(&speaker_id),
                    ))
                    .execute(&mut conn)
                    .unwrap();
            }
        }
    }

    if args.judges {
        let mut teams =
            csv::Reader::from_reader(File::open("src/bin/judges.csv").unwrap());
        let headers = teams.headers().unwrap().clone();

        for (i, result) in teams.records().enumerate() {
            let team = result.unwrap();
            let judge: JudgeRow = team.deserialize(Some(&headers)).unwrap();

            diesel::insert_into(tournament_judges::table)
                .values((
                    tournament_judges::id.eq(Uuid::now_v7().to_string()),
                    tournament_judges::tournament_id.eq(&tournament_id),
                    tournament_judges::name.eq(judge.name),
                    tournament_judges::email.eq(judge
                        .email
                        // todo: allow missing emails for judges
                        .expect("todo: allow missing emails for judges")),
                    tournament_judges::institution_id.eq(
                        get_or_create_institution(
                            &mut conn,
                            &tournament_id,
                            &judge.institution,
                        ),
                    ),
                    tournament_judges::private_url
                        .eq(Uuid::new_v4().to_string()),
                    tournament_judges::number.eq(i as i64),
                ))
                .execute(&mut conn)
                .unwrap();
        }
    }

    if args.rounds {
        let open = Uuid::now_v7().to_string();
        diesel::insert_into(tournament_break_categories::table)
            .values((
                tournament_break_categories::id.eq(&open),
                tournament_break_categories::tournament_id.eq(&tournament_id),
                tournament_break_categories::name.eq("Open"),
                tournament_break_categories::priority.eq(0),
            ))
            .execute(&mut conn)
            .unwrap();

        let rounds = [
            (1, "Round 1A", "P", None::<String>),
            (1, "Round 1B", "P", None::<String>),
            (2, "Round 2", "P", None::<String>),
            (3, "Round 3", "P", None::<String>),
            (4, "Round 4", "P", None::<String>),
            (5, "Round 5", "P", None::<String>),
            (6, "Round 6", "P", None::<String>),
            (7, "Quarterfinals", "E", Some(open.clone())),
            (8, "Semi-finals", "E", Some(open.clone())),
            (9, "Finals", "E", Some(open)),
        ];

        for round in rounds {
            diesel::insert_into(tournament_rounds::table)
                .values((
                    tournament_rounds::id.eq(Uuid::now_v7().to_string()),
                    tournament_rounds::tournament_id.eq(&tournament_id),
                    tournament_rounds::seq.eq(round.0),
                    tournament_rounds::name.eq(round.1),
                    tournament_rounds::kind.eq(round.2),
                    tournament_rounds::break_category.eq(round.3),
                    tournament_rounds::completed.eq(false),
                    tournament_rounds::draw_status.eq("N"),
                    tournament_rounds::draw_released_at
                        .eq(None::<NaiveDateTime>),
                ))
                .execute(&mut conn)
                .unwrap();
        }
    }
}

fn get_or_create_institution(
    conn: &mut SqliteConnection,
    tournament_id: &String,
    inst: &Option<String>,
) -> Option<String> {
    if let Some(inst) = inst {
        match tournament_institutions::table
            .filter(tournament_institutions::name.eq(&inst))
            .select(tournament_institutions::id)
            .first::<String>(conn)
            .optional()
            .unwrap()
        {
            Some(id) => Some(id),
            None => {
                let iid = Uuid::now_v7().to_string();

                diesel::insert_into(tournament_institutions::table)
                    .values((
                        tournament_institutions::id.eq(&iid),
                        tournament_institutions::tournament_id
                            .eq(tournament_id),
                        tournament_institutions::name.eq(&inst),
                        tournament_institutions::code.eq(&inst),
                    ))
                    .execute(conn)
                    .unwrap();

                Some(iid)
            }
        }
    } else {
        None
    }
}

#[allow(warnings)]
mod tabbycat_cli_copied {
    use std::collections::HashMap;

    use itertools::Itertools;
    use serde::{
        Deserialize, Deserializer,
        de::{self, Unexpected},
    };

    #[derive(Deserialize, Debug, Clone)]
    pub struct InstitutionRow {
        pub region: Option<String>,
        // TODO: warn when this is >20 characters (Tabbycat currently applies
        // this restriction) to aid with debugging
        pub short_code: String,
        pub full_name: String,
    }

    fn ret_false() -> bool {
        false
    }

    fn tags_deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Vec<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let str_sequence = String::deserialize(deserializer)?;
        Ok(str_sequence
            .split(',')
            .map(|item| item.to_owned())
            .filter(|item| !item.is_empty())
            .collect())
    }

    fn bool_from_str<'de, D>(deserializer: D) -> Result<bool, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.to_lowercase().trim() {
            "t" | "true" | "1" | "on" | "y" | "yes" => Ok(true),
            "f" | "false" | "0" | "off" | "n" | "no" | "" => Ok(false),
            other => Err(de::Error::invalid_value(
                Unexpected::Str(other),
                &"Must be truthy (t, true, 1, on, y, yes) or falsey (f, false, 0, off, n, no)",
            )),
        }
    }

    fn not_true() -> bool {
        false
    }

    // todo: team institution clashes
    #[derive(Deserialize, Debug, Clone)]
    pub struct TeamRow {
        pub full_name: String,
        /// If not supplied, we truncate the full name.
        pub short_name: Option<String>,
        #[serde(deserialize_with = "tags_deserialize", default = "Vec::new")]
        pub categories: Vec<String>,
        pub code_name: Option<String>,
        pub institution: Option<String>,
        pub seed: Option<u32>,
        pub emoji: Option<String>,
        #[serde(deserialize_with = "bool_from_str", default = "not_true")]
        pub use_institution_prefix: bool,
        #[serde(flatten, deserialize_with = "deserialize_fields_to_vec")]
        pub speakers: Vec<Speaker>,
    }

    #[derive(Deserialize, Debug, Clone)]
    pub struct Clash {
        pub object_1: String,
        pub object_2: String,
    }

    fn deserialize_fields_to_vec<'de, D>(
        deserializer: D,
    ) -> Result<Vec<Speaker>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map: HashMap<String, String> = HashMap::deserialize(deserializer)?;
        let speaker_buckets = {
            let mut buckets: HashMap<u8, HashMap<String, String>> =
                HashMap::new();
            for (key, value) in map.iter() {
                if let Some(iter) = key.strip_prefix("speaker") {
                    // todo: good error messages
                    let mut iter = iter.split('_');
                    let number =
                        iter.next().unwrap().trim().parse::<u8>().unwrap();
                    let field_name = iter.next().unwrap();
                    buckets
                        .entry(number)
                        .and_modify(|map| {
                            map.insert(field_name.to_string(), value.clone());
                        })
                        .or_insert({
                            let mut t = HashMap::new();

                            t.insert(field_name.to_string(), value.clone());
                            t
                        });
                }
            }
            buckets
        };

        Ok(speaker_buckets
            .into_iter()
            .sorted_by_key(|(t, _)| *t)
            .filter_map(|(_, map)| {
                if map.values().all(|key| key.trim().is_empty()) {
                    None
                } else {
                    Some(Speaker {
                        name: map
                            .get("name")
                            .cloned()
                            .expect("error: missing name!"),
                        categories: map
                            .get("categories")
                            .cloned()
                            .map(|t| {
                                t.split(',')
                                    .map(|x| x.to_string())
                                    .filter(|t| !t.trim().is_empty())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or(vec![]),
                        email: map.get("email").cloned(),
                        phone: map.get("phone").cloned(),
                        anonymous: map
                            .get("anonymous")
                            .cloned()
                            .map(|t| t.eq_ignore_ascii_case("true"))
                            .unwrap_or(false),
                        code_name: map.get("code_name").cloned(),
                        url_key: map.get("url_key").cloned(),
                        gender: map.get("gender").map(|gender| {
                            if gender.to_lowercase() == "male" {
                                "M"
                            } else if gender.to_lowercase() == "female" {
                                "F"
                            } else if gender.to_lowercase() == "other" {
                                "O"
                            } else {
                                gender
                            }
                            .to_string()
                        }),
                        pronoun: map.get("pronoun").cloned(),
                    })
                }
            })
            .collect::<Vec<_>>())
    }

    #[derive(Deserialize, Debug, Clone)]
    pub struct Speaker {
        pub name: String,
        pub categories: Vec<String>,
        pub email: Option<String>,
        pub phone: Option<String>,
        pub anonymous: bool,
        pub code_name: Option<String>,
        pub url_key: Option<String>,
        // todo: validate correct
        pub gender: Option<String>,
        // todo: validate length
        pub pronoun: Option<String>,
    }

    #[derive(Deserialize, Debug, Clone)]
    pub struct JudgeRow {
        pub name: String,
        pub institution: Option<String>,
        #[serde(deserialize_with = "tags_deserialize", default = "Vec::new")]
        pub institution_clashes: Vec<String>,
        pub email: Option<String>,
        #[serde(deserialize_with = "bool_from_str", default = "ret_false")]
        pub is_ca: bool,
        #[serde(deserialize_with = "bool_from_str", default = "ret_false")]
        pub is_ia: bool,
        pub base_score: Option<f64>,
        #[serde(deserialize_with = "tags_deserialize", default = "Vec::new")]
        pub availability: Vec<String>,
    }
}
