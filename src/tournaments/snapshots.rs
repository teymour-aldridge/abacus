use chrono::{NaiveDateTime, Utc};
use diesel::{prelude::*, sql_types::Text};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{schema::tournament_snapshots, state::LockedConn};

#[derive(Queryable, Serialize, Deserialize)]
pub struct Snapshot {
    id: String,
    created_at: NaiveDateTime,
    contents: Option<String>,
    tournament_id: String,
    prev: Option<String>,
    schema_id: String,
}

/// This struct can be deserialized
pub struct SnapshotData {}

/// Creates a new snapshot.
///
/// I would expect this function to be sufficiently slow that it should be run
/// on a worker thread (so as to not block the async executor).
///
/// This implementation is naive -- it would probably make more sense to store
/// this as something like
///
/// ```ignore
/// pub struct Changeset(Vec<Change>);
///
/// pub struct Change {
///     obj_id: String,
///     table_name: String,
///     before: State,
///     after: State
/// }
///
/// pub struct State {
///     /// Does not exit
///     Dne,
///     Object(serde_json::Value)
/// }
/// ```
///
/// Callers would then need to specify the exact change.
pub fn take_snapshot(tid: &str, mut conn: LockedConn<'_>) -> String {
    let latest = tournament_snapshots::table
        .order_by(tournament_snapshots::created_at.desc())
        .first::<Snapshot>(&mut *conn)
        .optional()
        .unwrap();

    let snapshot = diesel::sql_query(
        r#"
        SELECT
            json_object(
            'tournament_ballots', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'debate_id', debate_id,
                    'judge_id', judge_id,
                    'submitted_at', submitted_at,
                    'version', version,
                    'change', change,
                    'editor_id', editor_id
                )
            ) FROM tournament_ballots WHERE tournament_id = $1),
            'tournament_debate_judges', (SELECT json_group_array(
                json_object(
                    'debate_id', debate_id,
                    'judge_id', judge_id,
                    'status', status
                )
            ) FROM tournament_debate_judges WHERE debate_id IN (
                SELECT id FROM tournament_debates WHERE tournament_id = $1
            )),
            'tournament_break_categories', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'name', name,
                    'priority', priority
                )
            ) FROM tournament_break_categories WHERE tournament_id = $1),
            'tournament_debate_teams', (SELECT json_group_array(
                json_object(
                    'rowid', rowid,
                    'debate_id', debate_id,
                    'team_id', team_id,
                    'side', side,
                    'seq', seq
                )
            ) FROM tournament_debate_teams WHERE debate_id IN (
                SELECT id FROM tournament_debates WHERE tournament_id = $1
            )),
            'tournament_debates', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'draw_id', draw_id,
                    'room_id', room_id,
                    'motion_id', motion_id
                )
            ) FROM tournament_debates WHERE tournament_id = $1),
            'tournament_draws', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'round_id', round_id,
                    'status', status,
                    'released_at', released_at
                )
            ) FROM tournament_draws WHERE tournament_id = $1),
            'tournament_institutions', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'name', name,
                    'code', code
                )
            ) FROM tournament_institutions WHERE tournament_id = $1),
            'tournament_judge_judge_clash', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'judge1_id', judge1_id,
                    'judge2_id', judge2_id
                )
            ) FROM tournament_judge_judge_clash WHERE tournament_id = $1),
            'tournament_judge_team_clash', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'judge_id', judge_id,
                    'team_id', team_id
                )
            ) FROM tournament_judge_team_clash WHERE tournament_id = $1),
            'tournament_judges', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'name', name,
                    'institution_id', institution_id,
                    'participant_id', participant_id
                )
            ) FROM tournament_judges WHERE tournament_id = $1),
            'tournament_members', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'user_id', user_id,
                    'tournament_id', tournament_id,
                    'is_superuser', is_superuser,
                    'is_ca', is_ca,
                    'is_equity', is_equity
                )
            ) FROM tournament_members WHERE tournament_id = $1),
            'tournament_participants', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'private_url', private_url
                )
            ) FROM tournament_participants WHERE tournament_id = $1),
            'tournament_rooms', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'name', name,
                    'url', url,
                    'priority', priority
                )
            ) FROM tournament_rooms WHERE tournament_id = $1),
            'tournament_round_motions', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'round_id', round_id,
                    'infoslide', infoslide,
                    'motion', motion
                )
            ) FROM tournament_round_motions WHERE tournament_id = $1),
            'tournament_rounds', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'seq', seq,
                    'name', name,
                    'break_category', break_category,
                    'kind', kind
                )
            ) FROM tournament_rounds WHERE tournament_id = $1),
            'tournament_snapshots', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'created_at', created_at,
                    'contents', contents,
                    'tournament_id', tournament_id,
                    'prev', prev
                )
            ) FROM tournament_snapshots WHERE tournament_id = $1),
            'tournament_speaker_score_entries', (SELECT json_group_array(
                json_object(
                    'ballot_id', ballot_id,
                    'team_id', team_id,
                    'speaker_id', speaker_id,
                    'speaker_position', speaker_position,
                    'score', score
                )
            ) FROM tournament_speaker_score_entries WHERE ballot_id IN (
                SELECT id FROM tournament_ballots WHERE tournament_id = $1
            )),
            'tournament_speakers', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'name', name,
                    'email', email,
                    'participant_id', participant_id
                )
            ) FROM tournament_speakers WHERE tournament_id = $1),
            'tournament_team_score_entries', (SELECT json_group_array(
                json_object(
                    'ballot_id', ballot_id,
                    'team_id', team_id,
                    'score', score
                )
            ) FROM tournament_team_score_entries WHERE ballot_id IN (
                SELECT id FROM tournament_ballots WHERE tournament_id = $1
            )),
            'tournament_team_speakers', (SELECT json_group_array(
                json_object(
                    'rowid', rowid,
                    'team_id', team_id,
                    'speaker_id', speaker_id
                )
            ) FROM tournament_team_speakers WHERE team_id IN (
                SELECT id FROM tournament_teams WHERE tournament_id = $1
            )),
            'tournament_teams', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'tournament_id', tournament_id,
                    'name', name,
                    'institution_id', institution_id
                )
            ) FROM tournament_teams WHERE tournament_id = $1),
            'tournaments', (SELECT json_group_array(
                json_object(
                    'id', id,
                    'name', name,
                    'abbrv', abbrv,
                    'slug', slug,
                    'created_at', created_at,
                    'teams_per_side', teams_per_side,
                    'substantive_speakers', substantive_speakers,
                    'reply_speakers', reply_speakers,
                    'reply_must_speak', reply_must_speak,
                    'max_substantive_speech_index_for_reply',
                    max_substantive_speech_index_for_reply,
                    'pool_ballot_setup', pool_ballot_setup,
                    'elim_ballot_setup', elim_ballot_setup,
                    'elim_ballots_require_speaks', elim_ballots_require_speaks,
                    'institution_penalty', institution_penalty,
                    'history_penalty', history_penalty,
                    'team_standings_metrics', team_standings_metrics,
                    'speaker_standings_metrics', speaker_standings_metrics,
                    'exclude_from_speaker_standings_after',
                    exclude_from_speaker_standings_after
                )
            ) FROM tournaments WHERE id = $1)
            );
        "#,
    )
    .bind::<Text, _>(tid)
    .get_result::<JsonResult>(&mut *conn)
    .unwrap();

    diesel::table! {
        __diesel_schema_migrations (version) {
            version -> VarChar,
            run_on -> Timestamp,
        }
    }

    let schema_version = __diesel_schema_migrations::table
        .order_by(__diesel_schema_migrations::version.desc())
        .select(__diesel_schema_migrations::version)
        .first::<String>(&mut *conn)
        .unwrap();

    let other = Uuid::now_v7().to_string();
    diesel::insert_into(tournament_snapshots::table)
        .values((
            tournament_snapshots::id.eq(other.clone()),
            tournament_snapshots::created_at.eq(Utc::now().naive_utc()),
            tournament_snapshots::contents.eq(Some(snapshot.json)),
            tournament_snapshots::prev.eq(latest.map(|latest| latest.id)),
            tournament_snapshots::schema_id.eq(schema_version),
        ))
        .execute(&mut *conn)
        .unwrap();

    other
}

#[derive(QueryableByName)]
struct JsonResult {
    #[diesel(sql_type = Text)]
    json: String,
}
