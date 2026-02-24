use std::fs::File;

use abacus::MIGRATIONS;
use abacus::schema::{
    break_categories, institutions, judges, motions_of_round, org, rooms,
    rounds, speakers, speakers_of_team, teams, tournaments, users,
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

use crate::tabbycat_cli_copied::{JudgeRow, RoomRow, TeamRow};

#[derive(Parser)]
pub struct Import {
    database_url: Option<String>,
    #[clap(long, short, action)]
    teams: bool,
    #[clap(long, short, action)]
    judges: bool,
    #[clap(long, short, action)]
    rounds: bool,
    #[clap(long, action)]
    rooms: bool,
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
            tournaments::require_elim_substantive_speaks.eq(false),
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
            tournaments::exclude_from_speaker_standings_after.eq(None::<i64>),
        ))
        .execute(&mut conn)
        .unwrap();

    diesel::insert_into(org::table)
        .values((
            org::id.eq(Uuid::now_v7().to_string()),
            org::user_id.eq(user_id),
            org::tournament_id.eq(&tournament_id),
            org::is_superuser.eq(true),
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

            diesel::insert_into(teams::table)
                .values((
                    teams::id.eq(&team_id),
                    teams::tournament_id.eq(&tournament_id),
                    teams::name.eq(&team.full_name),
                    teams::institution_id.eq(get_or_create_institution(
                        &mut conn,
                        &tournament_id,
                        &team.institution,
                    )),
                    teams::number.eq(i as i64),
                ))
                .execute(&mut conn)
                .unwrap();

            for speaker in team.speakers {
                let speaker_id = Uuid::now_v7().to_string();

                diesel::insert_into(speakers::table)
                    .values((
                        speakers::id.eq(&speaker_id),
                        speakers::tournament_id.eq(&tournament_id),
                        speakers::name.eq(&speaker.name),
                        speakers::email.eq(&speaker.email.unwrap()),
                        speakers::private_url.eq(Uuid::new_v4().to_string()),
                    ))
                    .execute(&mut conn)
                    .unwrap();

                diesel::insert_into(speakers_of_team::table)
                    .values((
                        speakers_of_team::id.eq(Uuid::now_v7().to_string()),
                        speakers_of_team::team_id.eq(&team_id),
                        speakers_of_team::speaker_id.eq(&speaker_id),
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

            diesel::insert_into(judges::table)
                .values((
                    judges::id.eq(Uuid::now_v7().to_string()),
                    judges::tournament_id.eq(&tournament_id),
                    judges::name.eq(judge.name),
                    judges::email.eq(judge
                        .email
                        // todo: allow missing emails for judges
                        .expect("todo: allow missing emails for judges")),
                    judges::institution_id.eq(get_or_create_institution(
                        &mut conn,
                        &tournament_id,
                        &judge.institution,
                    )),
                    judges::private_url.eq(Uuid::new_v4().to_string()),
                    judges::number.eq(i as i64),
                ))
                .execute(&mut conn)
                .unwrap();
        }
    }

    if args.rooms {
        let mut rooms =
            csv::Reader::from_reader(File::open("src/bin/rooms.csv").unwrap());
        let headers = rooms.headers().unwrap().clone();

        for (i, result) in rooms.records().enumerate() {
            let room_rec = result.unwrap();
            let room: RoomRow = room_rec.deserialize(Some(&headers)).unwrap();

            diesel::insert_into(rooms::table)
                .values((
                    rooms::id.eq(Uuid::now_v7().to_string()),
                    rooms::tournament_id.eq(&tournament_id),
                    rooms::name.eq(room.name),
                    rooms::priority.eq(room.priority),
                    rooms::number.eq(i as i64),
                ))
                .execute(&mut conn)
                .unwrap();
        }
    }

    if args.rounds {
        let open = Uuid::now_v7().to_string();
        diesel::insert_into(break_categories::table)
            .values((
                break_categories::id.eq(&open),
                break_categories::tournament_id.eq(&tournament_id),
                break_categories::name.eq("Open"),
                break_categories::priority.eq(0),
            ))
            .execute(&mut conn)
            .unwrap();

        let rounds = [
            (
                1,
                "Round 1A",
                "P",
                None::<String>,
                "This House would require all world maps to be drawn using equal-area projections",
            ),
            (
                1,
                "Round 1B",
                "P",
                None::<String>,
                "This House believes that tomatoes are a fruit",
            ),
            (
                2,
                "Round 2",
                "P",
                None::<String>,
                "This House believes that Harry Potter should have married Luna Lovegood",
            ),
            (
                3,
                "Round 3",
                "P",
                None::<String>,
                "This House regrets the existence of Valentine's Day",
            ),
            (
                4,
                "Round 4",
                "P",
                None::<String>,
                "This House would stop using cash",
            ),
            (
                5,
                "Round 5",
                "P",
                None::<String>,
                "This House would abolish timeouts in all team sports",
            ),
            (
                6,
                "Round 6",
                "P",
                None::<String>,
                "This House would make Esperanto mandatory in all schools in the world",
            ),
            (
                7,
                "Quarterfinals",
                "E",
                Some(open.clone()),
                "This House would require every citizen to watch a film produced in their country every year",
            ),
            (
                8,
                "Semi-finals",
                "E",
                Some(open.clone()),
                "This House would make an exception to laws forbidding indecent exposure for hot days",
            ),
            (
                9,
                "Finals",
                "E",
                Some(open),
                "This House would not buy Apple",
            ),
        ];

        for round in rounds {
            let round_id = Uuid::now_v7().to_string();
            diesel::insert_into(rounds::table)
                .values((
                    rounds::id.eq(&round_id),
                    rounds::tournament_id.eq(&tournament_id),
                    rounds::seq.eq(round.0),
                    rounds::name.eq(round.1),
                    rounds::kind.eq(round.2),
                    rounds::break_category.eq(round.3),
                    rounds::completed.eq(false),
                    rounds::draw_status.eq("none"),
                    rounds::draw_released_at.eq(None::<NaiveDateTime>),
                ))
                .execute(&mut conn)
                .unwrap();

            diesel::insert_into(motions_of_round::table)
                .values((
                    motions_of_round::id.eq(Uuid::now_v7().to_string()),
                    motions_of_round::tournament_id.eq(&tournament_id),
                    motions_of_round::round_id.eq(&round_id),
                    motions_of_round::infoslide.eq(None::<String>),
                    motions_of_round::motion.eq(round.4),
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
        match institutions::table
            .filter(institutions::name.eq(&inst))
            .select(institutions::id)
            .first::<String>(conn)
            .optional()
            .unwrap()
        {
            Some(id) => Some(id),
            None => {
                let iid = Uuid::now_v7().to_string();

                diesel::insert_into(institutions::table)
                    .values((
                        institutions::id.eq(&iid),
                        institutions::tournament_id.eq(tournament_id),
                        institutions::name.eq(&inst),
                        institutions::code.eq(&inst),
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

    #[derive(Deserialize, Debug, Clone)]
    pub struct RoomRow {
        pub name: String,
        pub priority: i64,
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
