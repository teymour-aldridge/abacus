-- This file should undo anything in `up.sql`
drop table if exists answers_of_feedback_from_teams;

drop table if exists answers_of_feedback_from_judges;

drop table if exists feedback_questions;

drop table if exists feedback_of_teams;

drop table if exists feedback_of_judges;

drop table if exists speaker_scores_of_ballot;

drop table if exists team_ranks_of_ballot;

drop table if exists ballots;

drop table if exists agg_speaker_results_of_debate;

drop table if exists agg_team_results_of_debate;

drop table if exists speaker_metrics;

drop table if exists team_metrics;

drop table if exists speaker_standings;

drop table if exists team_standings;

drop table if exists judges_of_debate;

drop table if exists teams_of_debate;

drop table if exists debates;

drop table if exists judge_availability;

drop table if exists judge_stated_eligibility;

drop table if exists team_availability;

drop table if exists tickets_of_round;

drop table if exists motions_of_round;

drop table if exists rounds;

drop table if exists break_categories;

drop table if exists judge_room_constraints;

drop table if exists speaker_room_constraints;

drop table if exists rooms_of_category;

drop table if exists room_categories;

drop table if exists rooms;

drop table if exists judge_clashes_of_judge;

drop table if exists team_clashes_of_judge;

drop table if exists judges;

drop table if exists speakers_of_team;

drop table if exists speakers;

drop table if exists teams;

drop table if exists institutions;

drop table if exists members_of_group;

drop table if exists permissions_of_group;

drop table if exists groups;

drop table if exists org;

drop table if exists snapshots;

drop table if exists action_logs;

drop table if exists tournaments;

drop table if exists users;
