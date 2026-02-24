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

    -- CONFIGURATION: WHAT DATA SHOULD BE PUBLIC?

    -- whether to publish the team tab
    team_tab_public boolean not null default 'f',
    -- whether to publish the speaker tab
    speaker_tab_public boolean not null default 'f',
    -- whether to publish the standings (i.e. the ranking of teams
    -- based on team points, but NOT any other metrics)
    standings_public boolean not null default 'f',
    -- whether to show the results for completed (and non-silent) rounds on the
    -- home page
    show_round_results boolean not null default 't',
    -- whether to show draws publicly
    show_draws boolean not null default 't',

    -- CONFIGURATION: WHAT ARE THE FORMAT RULES?

    -- 1 for Australs/WSDC, 2 for BP
    teams_per_side integer not null,
    -- Speakers/team in each debate (2 for BP, 3 for Australs)
    substantive_speakers integer not null,
    reply_speakers boolean not null default 'f',
    reply_must_speak boolean not null default 't',
    -- e.g. "1" => must be first speaker, "2" => must be first OR second speaker
    max_substantive_speech_index_for_reply integer,


    -- CONFIGURATION: VOTING

    -- individual or consensus ballots for the pool
    pool_ballot_setup text not null check(pool_ballot_setup in ('consensus', 'individual')),
    -- individual or consensus ballots for the elimination rounds
    elim_ballot_setup text not null check (elim_ballot_setup in ('consensus', 'individual')),
    -- whether ballots who are not in the majority should be included when
    -- computing average speaks for a given round
    margin_includes_dissenters boolean not null default 't',

    -- CONFIGURATION: HOW ARE DEBATES SCORED?

    -- whether speaks are required for each preliminary round
    require_prelim_substantive_speaks boolean not null default 't',
    require_prelim_speaker_order boolean not null default 't'
        check (not require_prelim_substantive_speaks or require_prelim_speaker_order),

    -- whether speaks are required for each elimination round
    require_elim_substantive_speaks boolean not null default 'f',
    require_elim_speaker_order boolean not null default 't'
        check (not require_elim_substantive_speaks or require_elim_speaker_order),

    substantive_speech_min_speak float default 50.0,
    substantive_speech_max_speak float default 99.0,
    substantive_speech_step float default 1.0,

    reply_speech_min_speak float,
    reply_speech_max_speak float,

    -- CONFIGURATION: DRAW RULES

    -- The penalty applied when teams from the same institution are assigned
    -- to debate against each other on the draw. Higher values make it less
    -- likely that teams from the same institution will debate against each
    -- other.
    institution_penalty integer not null,
    history_penalty integer not null,
    pullup_metrics text not null
        check(json_valid(pullup_metrics) = 1
            and json_type(pullup_metrics) = 'array'),
    repeat_pullup_penalty integer not null check (repeat_pullup_penalty >= 0),

    -- CONFIGURATION: STANDINGS
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
create table if not exists snapshots (
    id text not null primary key,
    created_at timestamp not null,
    -- These can be deleted (after a certain number of days, or after a database
    -- migration).
    contents text,
    tournament_id text not null references tournaments (id),
    prev text references snapshots (id),
    schema_id text not null
);

create table if not exists action_logs (
    id text not null primary key,
    snapshot_id text not null references snapshots (id),
    message text
);

create table if not exists org (
    id text primary key not null,
    user_id text not null references users (id),
    tournament_id text not null references tournaments (id),
    is_superuser boolean not null default 0
);

create table if not exists groups (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null
);

create table if not exists permissions_of_group (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    group_id text not null references groups (id),
    permission text not null unique,
    unique (group_id, permission)
);

create table if not exists members_of_group (
    id text primary key not null,
    member_id text not null references org (id),
    group_id text not null references groups (id),
    unique (member_id, group_id)
);

create table if not exists institutions (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null,
    code text not null
);

create table if not exists teams (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null,
    institution_id text references institutions(id),
    number integer not null
);

create table if not exists speakers (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null unique,
    email text not null,
    private_url text not null unique
);

create table if not exists speakers_of_team (
    id text primary key not null,
    team_id text not null references teams (id),
    speaker_id text not null references speakers (id),
    unique (team_id, speaker_id)
);

create table if not exists judges (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null,
    email text not null,
    institution_id text references institutions (id),
    private_url text not null unique,
    number integer not null check (number >= 0),
    unique (tournament_id, number)
);

create table if not exists team_clashes_of_judge (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    judge_id text not null references judges (id),
    team_id text not null references teams (id),
    unique (tournament_id, judge_id, team_id)
);

create table if not exists judge_clashes_of_judge (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    judge1_id text not null references judges(id),
    judge2_id text not null references judges(id),
    check (judge1_id != judge2_id),
    unique (tournament_id, judge1_id, judge2_id)
);

create table if not exists rooms (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null,
    url text,
    priority integer not null check (priority >= 0),
    number integer not null check (number >= 0),
    unique (tournament_id, name),
    unique (tournament_id, number)
);

create table if not exists room_categories (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    private_name text not null,
    public_name text not null,
    description text not null
);

create table if not exists rooms_of_category (
    id text primary key not null,
    category_id text not null references room_categories (id),
    room_id text not null references rooms (id),
    unique (category_id, room_id)
);

-- Room constraints.
--
-- The way room constraints are handled in other software (e.g. Tabbycat) seems
-- to be particularly broken.
--
-- Interface per user:
-- - "1st preference"
-- - "2nd preference"
-- - ...
-- - "kth preference"
--
-- Then have a ranking of priorities:
-- - "participant 1"
-- - "participant 2"
-- - ...
-- - "participant k"
create table if not exists speaker_room_constraints (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    speaker_id text not null references speakers (id),
    category_id text not null references rooms_of_category (id),
    -- the importance of this constraint (lower = more important)
    pref integer not null check (pref > 0),
    unique (speaker_id, category_id),
    unique (speaker_id, pref)
);

create table if not exists judge_room_constraints (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    judge_id text not null references judges (id),
    category_id text not null references rooms_of_category (id),
    -- the importance of this constraint (lower = more important)
    pref integer not null check (pref > 0),
    unique (judge_id, category_id),
    unique (judge_id, pref)
);

create table if not exists break_categories (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    name text not null,
    priority integer not null,
    check (priority >= 0)
);

create table if not exists rounds (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    seq integer not null,
    name text not null,
    kind text not null check (kind in ('E', 'P')),
    break_category text references break_categories (id),
    completed boolean not null,
    draw_status text not null default 'none' check (draw_status in ('none', 'draft', 'confirmed', 'released_teams', 'released_full')),
    draw_released_at timestamp,
    motions_released_at timestamp,
    results_published_at timestamp,
    unique (tournament_id, name)
);

create table if not exists motions_of_round (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    round_id text not null references rounds(id),
    infoslide text,
    motion text not null,
    published_at timestamp
);

-- When generating draws, we have a ticketing system. This allows us to avoid
-- running the (sometimes) long-running draw or adjumo operations inside a
-- transaction (in SQLite this locks other processes and prevents them from
-- making progress).
--
-- The `acquired` field is useful for ensuring that tickets cannot be generated
-- too often (this prevents accidental DoS attacks).
--
-- TODO: when performing a restore operation we should retain old round tickets
-- and create a new one with a higher index than the pre-existing ones.
create table if not exists tickets_of_round (
    id text not null primary key,
    round_id text not null references rounds (id),
    seq integer not null,
    kind text not null check (kind in ('draw', 'adjumo')),
    -- when the ticket was created (useful for rate limiting)
    acquired timestamp not null default CURRENT_TIMESTAMP,
    -- denotes whether the process which acquired the ticket subsequently
    -- released it
    released boolean not null,
    -- if a process encounters an error, it logs the text here
    error text,
    unique (round_id, seq)
);

create table if not exists team_availability (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    round_id text not null references rounds (id),
    team_id text not null references teams (id),
    available bool not null default 'f',
    unique (round_id, team_id)
);

-- Eligibility, as specified by judges.
create table if not exists judge_stated_eligibility (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    round_id text not null references rounds (id),
    judge_id text not null references judges (id),
    available bool not null default 'f'
);

-- The actual judge eligibility.
create table if not exists judge_availability (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    round_id text not null references rounds (id),
    judge_id text not null references judges (id),
    available bool not null default 'f',
    comment text,
    unique (round_id, judge_id)
);

create table if not exists debates (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    round_id text not null references rounds(id),
    room_id text references rooms(id),
    -- unique ID (starting from zero) assigned to each debate
    number integer not null check (number >= 0),
    status text not null check (status in ('confirmed', 'draft', 'conflict')),
    unique (tournament_id, round_id, number)
);

create table if not exists teams_of_debate (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    debate_id text not null references debates(id),
    team_id text not null references teams(id),
    side integer not null check (side >= 0),
    seq integer not null check (seq >= 0),
    unique (debate_id, team_id)
);

create table if not exists judges_of_debate (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    debate_id text not null references debates(id),
    judge_id text not null references judges(id),
    status text not null check (status in ('C', 'P', 'T')),
    unique (debate_id, judge_id)
);

-- Note: the standings are (re)computed whenever a round is confirmed.
create table if not exists team_standings (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    team_id text not null references teams (id),
    rank integer not null
);

create table if not exists speaker_standings (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    speaker_id text not null references speakers (id),
    rank integer not null
);

create table if not exists team_metrics (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    team_id text not null references teams (id),
    metric_kind text not null,
    metric_value float not null,
    unique (tournament_id, team_id, metric_kind)
);

create table if not exists speaker_metrics (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    speaker_id text not null references speakers (id),
    metric_kind text not null,
    metric_value float not null,
    unique (tournament_id, speaker_id, metric_kind)
);

-- It is the responsibility of the appplication to ensure that
-- `agg_team_results_of_debate` and `agg_speaker_results_of_debate`
-- are (a) created and (b) updated as and when new ballots come in.
--
-- The correct behaviour is that either
-- 1. when there is a ballot conflict, then no results rows exist for a debate
--    ID
-- 2. when the ballots do not conflict with each other, the results are then
--    created
--
-- This requires logic to maintain this invariant whenever a new ballot is
-- submitted, or an administrator manually edits ballots.
create table if not exists agg_team_results_of_debate (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    debate_id text not null references debates (id),
    team_id text not null references teams (id),
    points integer
);

create table if not exists agg_speaker_results_of_debate (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    debate_id text not null references debates (id),
    speaker_id text not null references speakers (id),
    team_id text not null references teams (id),
    position integer not null,
    score float
);

-- TODO: in the future we will (eventually) want to add support for paper
-- ballots. It might make sense to do so as an optional extension service
-- (i.e. not part of the core application).

-- an individual ballot from an adjudicator
create table if not exists ballots (
    id text primary key not null,
    tournament_id text not null references tournaments(id),
    debate_id text not null references debates(id),
    judge_id text not null references judges(id),
    submitted_at timestamp not null default CURRENT_TIMESTAMP,
    motion_id text not null references motions_of_round (id),

    -- version control
    version integer not null check (version >= 0),
    change text,
    editor_id text references users(id),
    check ((editor_id is null) = (change is null)),
    check ((editor_id is null) = (version = 0)),

    unique (debate_id, version, judge_id)
);

-- Ballot parts for in-rounds.

-- This table might seem redundant (which it is when speaker scores are
-- supplied) but it is relevant for formats which do not use speaker scores.
create table if not exists team_ranks_of_ballot (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    ballot_id text not null references ballots (id),
    team_id text not null references teams (id),
    points integer not null check (points >= 0),
    unique (ballot_id, team_id)
);

create table if not exists speaker_scores_of_ballot (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    ballot_id text not null references ballots(id),
    team_id text not null references teams(id),
    speaker_id text not null references speakers(id),
    speaker_position integer not null,
    score float,
    unique (ballot_id, team_id, speaker_id, speaker_position)
);

create table if not exists feedback_of_judges (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    debate_id text not null references debates (id),
    judge_id text not null references judges (id),
    target_judge_id text not null references judges_of_debate (id),
    foreign key (debate_id, judge_id) references judges_of_debate (debate_id, judge_id),
    foreign key (debate_id, target_judge_id) references judges_of_debate (debate_id, judge_id)
);

create table if not exists feedback_of_teams (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    debate_id text not null references debates (id),
    team_id text not null,
    target_judge_id text not null references judges_of_debate (id),
    foreign key (debate_id, team_id) references agg_team_results_of_debate (debate_id, team_id),
    foreign key (debate_id, target_judge_id) references judges_of_debate (debate_id, judge_id)
);

create table if not exists feedback_questions (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    question text not null,
    kind text not null check (json_valid(kind) = 1 and json_type(kind) = 'object'),
    seq integer not null,
    for_judges boolean not null,
    for_teams boolean not null
);

create table if not exists answers_of_feedback_from_judges (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    feedback_id text not null,
    question_id text not null references feedback_questions (id),
    answer text not null
);

create table if not exists answers_of_feedback_from_teams (
    id text primary key not null,
    tournament_id text not null references tournaments (id),
    feedback_id text not null,
    question_id text not null references feedback_questions (id),
    answer text not null
);
