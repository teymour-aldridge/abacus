-- This file should undo anything in `up.sql`
drop table if exists tournament_speaker_score_entries;

drop table if exists tournament_team_score_entries;

drop table if exists tournament_ballots;

drop table if exists tournament_debate_judges;

drop table if exists tournament_debate_teams;

drop table if exists tournament_debates;

drop table if exists tournament_draws;

drop table if exists tournament_round_motions;

drop table if exists tournament_rounds;

drop table if exists tournament_rooms;

drop table if exists tournament_judge_judge_clash;

drop table if exists tournament_judge_team_clash;

drop table if exists tournament_judges;

drop table if exists tournament_team_speakers;

drop table if exists tournament_speakers;

drop table if exists tournament_teams;

drop table if exists tournament_institutions;

drop table if exists tournament_participants;

drop table if exists tournament_members;

drop table if exists tournament_action_logs;

drop table if exists tournament_snapshots;

drop table if exists tournaments;

drop table if exists users;
