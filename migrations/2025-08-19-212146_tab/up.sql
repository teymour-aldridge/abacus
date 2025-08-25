-- Your SQL goes here
create table if not exists users (
    id text primary key not null,
    email text not null unique,
    username text not null unique,
    password_hash text not null,
    created_at timestamp not null
);

create table if not exists tournaments (
    id text primary key not null,
    name text not null,
    abbrv text not null,
    slug text not null unique,
    created_at timestamp not null,

    -- Configuration

    -- 1 for Australs/WSDC, 2 for BP
    teams_per_side integer not null,
    -- Speakers/team in each debate (2 for BP, 3 for Australs)
    substantive_speakers integer not null,
    reply_speakers boolean not null default 'f',
    reply_must_speak boolean not null default 't',

    -- e.g. "1" => must be first speaker, "2" => must be first OR second speaker
    max_substantive_speech_index_for_reply integer,

    -- individual or consensus ballots for the pool
    pool_ballot_setup text not null check(pool_ballot_setup in ('consensus', 'individual')),

    -- individual or consensus ballots for the elimination rounds
    elim_ballot_setup text not null check (elim_ballot_setup in ('consensus', 'individual')),
    -- whether elimination ballots require speaker scores
    elim_ballots_require_speaks boolean not null,

    -- DRAW RULES
    institution_penalty integer,
    history_penalty integer,

    -- STANDINGS
    -- metrics, e.g. ["wins", "ballots", "atss"]
    team_standings_metrics text not null
        check(json_valid(team_standings_metrics) = 1
            and json_type(team_standings_metrics) = 'array'),
    -- metrics, e.g. ["average", "stddev"]
    speaker_standings_metrics text not null
        check (json_valid(speaker_standings_metrics)
            and json_type(speaker_standings_metrics) = 'array'),
    -- number of rounds that can be missed before a speaker is omitted from the
    -- tab
    exclude_from_speaker_standings_after integer
);

-- A snapshot of a tournament at a given point in time.
create table if not exists tournament_snapshots (
    id text not null primary key,
    created_at timestamp not null,
    -- These can be deleted (after a certain number of days, or after a database
    -- migration).
    contents text,
    tournament_id text not null references tournaments (id),
    prev text references tournament_snapshots (id),
    schema_id text not null
);

create table if not exists tournament_action_logs (
    id text not null primary key,
    snapshot_id text not null references tournament_snapshots (id),
    message text
);

-- Question: should this change with the state of the application? Perhaps not?
create table if not exists tournament_members (
    id text primary key not null,
    user_id text not null references users (id),
    tournament_id text not null references tournaments (id),
    -- assigns all the permissions
    is_superuser boolean not null,
    -- assigns CA permissions (TODO: full permission system)
    is_ca boolean not null,
    is_equity boolean not null
);

create table if not exists tournament_participants (
    id text not null primary key,
    tournament_id text not null references tournaments (id),
    private_url text not null unique,
    check (10 <= length(private_url))
);

create table if not exists tournament_institutions (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null,
    code text not null
);

create table if not exists tournament_teams (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null,
    institution_id text references tournament_institutions(id)
);

create table if not exists tournament_speakers (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null unique,
    email text not null,
    participant_id text not null references tournament_participants(id)
);

create table if not exists tournament_team_speakers (
    team_id text not null references tournament_teams (id),
    speaker_id text not null references tournament_speakers (id)
);

create table if not exists tournament_judges (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null,
    institution_id text references tournament_institutions (id),
    participant_id text not null references tournament_participants(id)
);

create table if not exists tournament_judge_team_clash (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    judge_id text not null references tournament_judges (id),
    team_id text not null references tournament_teams (id),
    unique (tournament_id, judge_id, team_id)
);

create table if not exists tournament_judge_judge_clash (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    judge1_id text not null references tournament_judges(id),
    judge2_id text not null references tournament_judges(id),
    check (judge1_id != judge2_id),
    unique (tournament_id, judge1_id, judge2_id)
);

create table if not exists tournament_rooms (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null,
    url text not null,
    priority integer not null check (priority >= 0),
    unique (tournament_id, name)
);

create table if not exists tournament_rounds (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    seq integer not null,
    name text not null,
    kind text not null check (kind in ('E', 'P')),
    unique (tournament_id, seq),
    unique (tournament_id, name)
);

create table if not exists tournament_round_motions (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    round_id text not null references tournament_rounds(id),
    infoslide text,
    motion text not null
);

create table if not exists tournament_draws (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    round_id text not null references tournament_rounds(id),
    status text not null default 'D' check (status in ('D', 'C', 'R')),
    released_at timestamp
);

create table if not exists tournament_debates (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    draw_id text not null references tournament_draws(id),
    room_id text references tournament_rooms(id),
    motion_id text not null references tournament_round_motions(id)
);

create table if not exists tournament_debate_teams (
    debate_id text not null references tournament_debates(id),
    team_id text not null references tournament_teams(id),
    side integer not null check (side >= 0),
    seq integer not null check (seq >= 0),
    unique (debate_id, team_id)
);

create table if not exists tournament_debate_judges (
    debate_id text not null references tournament_debates(id),
    judge_id text not null references tournament_judges(id),
    status text not null check (status in ('C', 'P', 'T')),
    primary key (debate_id, judge_id)
);

-- an individual ballot from an adjudicator
create table if not exists tournament_ballots (
    id text primary key not null,
    tournament_id text not null references tournaments(id),
    debate_id text not null references tournament_debates(id),
    judge_id text not null references tournament_judges(id),
    submitted_at timestamp not null default CURRENT_TIMESTAMP,

    -- version control
    version integer not null check (version >= 0),
    change text,
    editor_id text references users(id),
    check ((editor_id is null) = (change is null)),
    check ((editor_id is null) = (version = 0)),

    unique (debate_id, version, judge_id)
);

-- Team scores are required. Speaker scores are only required in some (most)
-- formats.

create table if not exists tournament_team_score_entries (
    ballot_id text not null references tournament_ballots(id),
    team_id text not null references tournament_teams(id),
    score integer not null,
    primary key (ballot_id, team_id)
);

create table if not exists tournament_speaker_score_entries (
    ballot_id text not null references tournament_ballots(id),
    team_id text not null references tournament_teams(id),
    speaker_id text not null references tournament_speakers(id),
    speaker_position integer not null,
    score float not null,
    primary key (ballot_id, team_id, speaker_id)
);
