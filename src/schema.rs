// @generated automatically by Diesel CLI.

diesel::table! {
    action_logs (id) {
        id -> Text,
        snapshot_id -> Text,
        message -> Nullable<Text>,
    }
}

diesel::table! {
    agg_speaker_results_of_debate (id) {
        id -> Text,
        tournament_id -> Text,
        debate_id -> Text,
        speaker_id -> Text,
        team_id -> Text,
        position -> BigInt,
        score -> Nullable<Float>,
    }
}

diesel::table! {
    agg_team_results_of_debate (id) {
        id -> Text,
        tournament_id -> Text,
        debate_id -> Text,
        team_id -> Text,
        points -> Nullable<BigInt>,
    }
}

diesel::table! {
    answers_of_feedback_from_judges (id) {
        id -> Text,
        tournament_id -> Text,
        feedback_id -> Text,
        question_id -> Text,
        answer -> Text,
    }
}

diesel::table! {
    answers_of_feedback_from_teams (id) {
        id -> Text,
        tournament_id -> Text,
        feedback_id -> Text,
        question_id -> Text,
        answer -> Text,
    }
}

diesel::table! {
    ballots (id) {
        id -> Text,
        tournament_id -> Text,
        debate_id -> Text,
        judge_id -> Text,
        submitted_at -> Timestamp,
        motion_id -> Text,
        version -> BigInt,
        change -> Nullable<Text>,
        editor_id -> Nullable<Text>,
    }
}

diesel::table! {
    break_categories (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        priority -> BigInt,
    }
}

diesel::table! {
    debates (id) {
        id -> Text,
        tournament_id -> Text,
        round_id -> Text,
        room_id -> Nullable<Text>,
        number -> BigInt,
        status -> Text,
    }
}

diesel::table! {
    feedback_of_judges (id) {
        id -> Text,
        tournament_id -> Text,
        debate_id -> Text,
        judge_id -> Text,
        target_judge_id -> Text,
    }
}

diesel::table! {
    feedback_of_teams (id) {
        id -> Text,
        tournament_id -> Text,
        debate_id -> Text,
        team_id -> Text,
        target_judge_id -> Text,
    }
}

diesel::table! {
    feedback_questions (id) {
        id -> Text,
        tournament_id -> Text,
        question -> Text,
        kind -> Text,
        seq -> BigInt,
        for_judges -> Bool,
        for_teams -> Bool,
    }
}

diesel::table! {
    groups (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
    }
}

diesel::table! {
    institutions (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        code -> Text,
    }
}

diesel::table! {
    judge_availability (id) {
        id -> Text,
        tournament_id -> Text,
        round_id -> Text,
        judge_id -> Text,
        available -> Bool,
        comment -> Nullable<Text>,
    }
}

diesel::table! {
    judge_clashes_of_judge (id) {
        id -> Text,
        tournament_id -> Text,
        judge1_id -> Text,
        judge2_id -> Text,
    }
}

diesel::table! {
    judge_room_constraints (id) {
        id -> Text,
        tournament_id -> Text,
        judge_id -> Text,
        category_id -> Text,
        pref -> BigInt,
    }
}

diesel::table! {
    judge_stated_eligibility (id) {
        id -> Text,
        tournament_id -> Text,
        round_id -> Text,
        judge_id -> Text,
        available -> Bool,
    }
}

diesel::table! {
    judges (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        email -> Text,
        institution_id -> Nullable<Text>,
        private_url -> Text,
        number -> BigInt,
    }
}

diesel::table! {
    judges_of_debate (id) {
        id -> Text,
        tournament_id -> Text,
        debate_id -> Text,
        judge_id -> Text,
        status -> Text,
    }
}

diesel::table! {
    members_of_group (id) {
        id -> Text,
        member_id -> Text,
        group_id -> Text,
    }
}

diesel::table! {
    motions_of_round (id) {
        id -> Text,
        tournament_id -> Text,
        round_id -> Text,
        infoslide -> Nullable<Text>,
        motion -> Text,
        published_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    org (id) {
        id -> Text,
        user_id -> Text,
        tournament_id -> Text,
        is_superuser -> Bool,
    }
}

diesel::table! {
    permissions_of_group (id) {
        id -> Text,
        tournament_id -> Text,
        group_id -> Text,
        permission -> Text,
    }
}

diesel::table! {
    room_categories (id) {
        id -> Text,
        tournament_id -> Text,
        private_name -> Text,
        public_name -> Text,
        description -> Text,
    }
}

diesel::table! {
    rooms (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        url -> Nullable<Text>,
        priority -> BigInt,
        number -> BigInt,
    }
}

diesel::table! {
    rooms_of_category (id) {
        id -> Text,
        category_id -> Text,
        room_id -> Text,
    }
}

diesel::table! {
    rounds (id) {
        id -> Text,
        tournament_id -> Text,
        seq -> BigInt,
        name -> Text,
        kind -> Text,
        break_category -> Nullable<Text>,
        completed -> Bool,
        draw_status -> Text,
        draw_released_at -> Nullable<Timestamp>,
        motions_released_at -> Nullable<Timestamp>,
        results_published_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    snapshots (id) {
        id -> Text,
        created_at -> Timestamp,
        contents -> Nullable<Text>,
        tournament_id -> Text,
        prev -> Nullable<Text>,
        schema_id -> Text,
    }
}

diesel::table! {
    speaker_metrics (id) {
        id -> Text,
        tournament_id -> Text,
        speaker_id -> Text,
        metric_kind -> Text,
        metric_value -> Float,
    }
}

diesel::table! {
    speaker_room_constraints (id) {
        id -> Text,
        tournament_id -> Text,
        speaker_id -> Text,
        category_id -> Text,
        pref -> BigInt,
    }
}

diesel::table! {
    speaker_scores_of_ballot (id) {
        id -> Text,
        tournament_id -> Text,
        ballot_id -> Text,
        team_id -> Text,
        speaker_id -> Text,
        speaker_position -> BigInt,
        score -> Nullable<Float>,
    }
}

diesel::table! {
    speaker_standings (id) {
        id -> Text,
        tournament_id -> Text,
        speaker_id -> Text,
        rank -> BigInt,
    }
}

diesel::table! {
    speakers (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        email -> Text,
        private_url -> Text,
    }
}

diesel::table! {
    speakers_of_team (id) {
        id -> Text,
        team_id -> Text,
        speaker_id -> Text,
    }
}

diesel::table! {
    team_availability (id) {
        id -> Text,
        tournament_id -> Text,
        round_id -> Text,
        team_id -> Text,
        available -> Bool,
    }
}

diesel::table! {
    team_clashes_of_judge (id) {
        id -> Text,
        tournament_id -> Text,
        judge_id -> Text,
        team_id -> Text,
    }
}

diesel::table! {
    team_metrics (id) {
        id -> Text,
        tournament_id -> Text,
        team_id -> Text,
        metric_kind -> Text,
        metric_value -> Float,
    }
}

diesel::table! {
    team_ranks_of_ballot (id) {
        id -> Text,
        tournament_id -> Text,
        ballot_id -> Text,
        team_id -> Text,
        points -> BigInt,
    }
}

diesel::table! {
    team_standings (id) {
        id -> Text,
        tournament_id -> Text,
        team_id -> Text,
        rank -> BigInt,
    }
}

diesel::table! {
    teams (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        institution_id -> Nullable<Text>,
        number -> BigInt,
    }
}

diesel::table! {
    teams_of_debate (id) {
        id -> Text,
        tournament_id -> Text,
        debate_id -> Text,
        team_id -> Text,
        side -> BigInt,
        seq -> BigInt,
    }
}

diesel::table! {
    tickets_of_round (id) {
        id -> Text,
        round_id -> Text,
        seq -> BigInt,
        kind -> Text,
        acquired -> Timestamp,
        released -> Bool,
        error -> Nullable<Text>,
    }
}

diesel::table! {
    tournaments (id) {
        id -> Text,
        name -> Text,
        abbrv -> Text,
        slug -> Text,
        created_at -> Timestamp,
        team_tab_public -> Bool,
        speaker_tab_public -> Bool,
        standings_public -> Bool,
        show_round_results -> Bool,
        show_draws -> Bool,
        teams_per_side -> BigInt,
        substantive_speakers -> BigInt,
        reply_speakers -> Bool,
        reply_must_speak -> Bool,
        max_substantive_speech_index_for_reply -> Nullable<BigInt>,
        pool_ballot_setup -> Text,
        elim_ballot_setup -> Text,
        margin_includes_dissenters -> Bool,
        require_prelim_substantive_speaks -> Bool,
        require_prelim_speaker_order -> Bool,
        require_elim_substantive_speaks -> Bool,
        require_elim_speaker_order -> Bool,
        substantive_speech_min_speak -> Nullable<Float>,
        substantive_speech_max_speak -> Nullable<Float>,
        substantive_speech_step -> Nullable<Float>,
        reply_speech_min_speak -> Nullable<Float>,
        reply_speech_max_speak -> Nullable<Float>,
        institution_penalty -> BigInt,
        history_penalty -> BigInt,
        pullup_metrics -> Text,
        repeat_pullup_penalty -> BigInt,
        team_standings_metrics -> Text,
        speaker_standings_metrics -> Text,
        exclude_from_speaker_standings_after -> Nullable<BigInt>,
    }
}

diesel::table! {
    users (id) {
        id -> Text,
        email -> Text,
        username -> Text,
        password_hash -> Text,
        created_at -> Timestamp,
    }
}

diesel::joinable!(action_logs -> snapshots (snapshot_id));
diesel::joinable!(agg_speaker_results_of_debate -> debates (debate_id));
diesel::joinable!(agg_speaker_results_of_debate -> speakers (speaker_id));
diesel::joinable!(agg_speaker_results_of_debate -> teams (team_id));
diesel::joinable!(agg_speaker_results_of_debate -> tournaments (tournament_id));
diesel::joinable!(agg_team_results_of_debate -> debates (debate_id));
diesel::joinable!(agg_team_results_of_debate -> teams (team_id));
diesel::joinable!(agg_team_results_of_debate -> tournaments (tournament_id));
diesel::joinable!(answers_of_feedback_from_judges -> feedback_questions (question_id));
diesel::joinable!(answers_of_feedback_from_judges -> tournaments (tournament_id));
diesel::joinable!(answers_of_feedback_from_teams -> feedback_questions (question_id));
diesel::joinable!(answers_of_feedback_from_teams -> tournaments (tournament_id));
diesel::joinable!(ballots -> debates (debate_id));
diesel::joinable!(ballots -> judges (judge_id));
diesel::joinable!(ballots -> motions_of_round (motion_id));
diesel::joinable!(ballots -> tournaments (tournament_id));
diesel::joinable!(ballots -> users (editor_id));
diesel::joinable!(break_categories -> tournaments (tournament_id));
diesel::joinable!(debates -> rooms (room_id));
diesel::joinable!(debates -> rounds (round_id));
diesel::joinable!(debates -> tournaments (tournament_id));
diesel::joinable!(feedback_of_judges -> debates (debate_id));
diesel::joinable!(feedback_of_judges -> judges (judge_id));
diesel::joinable!(feedback_of_judges -> tournaments (tournament_id));
diesel::joinable!(feedback_of_teams -> debates (debate_id));
diesel::joinable!(feedback_of_teams -> tournaments (tournament_id));
diesel::joinable!(feedback_questions -> tournaments (tournament_id));
diesel::joinable!(groups -> tournaments (tournament_id));
diesel::joinable!(institutions -> tournaments (tournament_id));
diesel::joinable!(judge_availability -> judges (judge_id));
diesel::joinable!(judge_availability -> rounds (round_id));
diesel::joinable!(judge_availability -> tournaments (tournament_id));
diesel::joinable!(judge_clashes_of_judge -> tournaments (tournament_id));
diesel::joinable!(judge_room_constraints -> judges (judge_id));
diesel::joinable!(judge_room_constraints -> rooms_of_category (category_id));
diesel::joinable!(judge_room_constraints -> tournaments (tournament_id));
diesel::joinable!(judge_stated_eligibility -> judges (judge_id));
diesel::joinable!(judge_stated_eligibility -> rounds (round_id));
diesel::joinable!(judge_stated_eligibility -> tournaments (tournament_id));
diesel::joinable!(judges -> institutions (institution_id));
diesel::joinable!(judges -> tournaments (tournament_id));
diesel::joinable!(judges_of_debate -> debates (debate_id));
diesel::joinable!(judges_of_debate -> judges (judge_id));
diesel::joinable!(judges_of_debate -> tournaments (tournament_id));
diesel::joinable!(members_of_group -> groups (group_id));
diesel::joinable!(members_of_group -> org (member_id));
diesel::joinable!(motions_of_round -> rounds (round_id));
diesel::joinable!(motions_of_round -> tournaments (tournament_id));
diesel::joinable!(org -> tournaments (tournament_id));
diesel::joinable!(org -> users (user_id));
diesel::joinable!(permissions_of_group -> groups (group_id));
diesel::joinable!(permissions_of_group -> tournaments (tournament_id));
diesel::joinable!(room_categories -> tournaments (tournament_id));
diesel::joinable!(rooms -> tournaments (tournament_id));
diesel::joinable!(rooms_of_category -> room_categories (category_id));
diesel::joinable!(rooms_of_category -> rooms (room_id));
diesel::joinable!(rounds -> break_categories (break_category));
diesel::joinable!(rounds -> tournaments (tournament_id));
diesel::joinable!(snapshots -> tournaments (tournament_id));
diesel::joinable!(speaker_metrics -> speakers (speaker_id));
diesel::joinable!(speaker_metrics -> tournaments (tournament_id));
diesel::joinable!(speaker_room_constraints -> rooms_of_category (category_id));
diesel::joinable!(speaker_room_constraints -> speakers (speaker_id));
diesel::joinable!(speaker_room_constraints -> tournaments (tournament_id));
diesel::joinable!(speaker_scores_of_ballot -> ballots (ballot_id));
diesel::joinable!(speaker_scores_of_ballot -> speakers (speaker_id));
diesel::joinable!(speaker_scores_of_ballot -> teams (team_id));
diesel::joinable!(speaker_scores_of_ballot -> tournaments (tournament_id));
diesel::joinable!(speaker_standings -> speakers (speaker_id));
diesel::joinable!(speaker_standings -> tournaments (tournament_id));
diesel::joinable!(speakers -> tournaments (tournament_id));
diesel::joinable!(speakers_of_team -> speakers (speaker_id));
diesel::joinable!(speakers_of_team -> teams (team_id));
diesel::joinable!(team_availability -> rounds (round_id));
diesel::joinable!(team_availability -> teams (team_id));
diesel::joinable!(team_availability -> tournaments (tournament_id));
diesel::joinable!(team_clashes_of_judge -> judges (judge_id));
diesel::joinable!(team_clashes_of_judge -> teams (team_id));
diesel::joinable!(team_clashes_of_judge -> tournaments (tournament_id));
diesel::joinable!(team_metrics -> teams (team_id));
diesel::joinable!(team_metrics -> tournaments (tournament_id));
diesel::joinable!(team_ranks_of_ballot -> ballots (ballot_id));
diesel::joinable!(team_ranks_of_ballot -> teams (team_id));
diesel::joinable!(team_ranks_of_ballot -> tournaments (tournament_id));
diesel::joinable!(team_standings -> teams (team_id));
diesel::joinable!(team_standings -> tournaments (tournament_id));
diesel::joinable!(teams -> institutions (institution_id));
diesel::joinable!(teams -> tournaments (tournament_id));
diesel::joinable!(teams_of_debate -> debates (debate_id));
diesel::joinable!(teams_of_debate -> teams (team_id));
diesel::joinable!(teams_of_debate -> tournaments (tournament_id));
diesel::joinable!(tickets_of_round -> rounds (round_id));

diesel::allow_tables_to_appear_in_same_query!(
    action_logs,
    agg_speaker_results_of_debate,
    agg_team_results_of_debate,
    answers_of_feedback_from_judges,
    answers_of_feedback_from_teams,
    ballots,
    break_categories,
    debates,
    feedback_of_judges,
    feedback_of_teams,
    feedback_questions,
    groups,
    institutions,
    judge_availability,
    judge_clashes_of_judge,
    judge_room_constraints,
    judge_stated_eligibility,
    judges,
    judges_of_debate,
    members_of_group,
    motions_of_round,
    org,
    permissions_of_group,
    room_categories,
    rooms,
    rooms_of_category,
    rounds,
    snapshots,
    speaker_metrics,
    speaker_room_constraints,
    speaker_scores_of_ballot,
    speaker_standings,
    speakers,
    speakers_of_team,
    team_availability,
    team_clashes_of_judge,
    team_metrics,
    team_ranks_of_ballot,
    team_standings,
    teams,
    teams_of_debate,
    tickets_of_round,
    tournaments,
    users,
);
