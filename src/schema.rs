// @generated automatically by Diesel CLI.

diesel::table! {
    feedback_from_judges_question_answers (id) {
        id -> Text,
        feedback_id -> Text,
        question_id -> Text,
        answer -> Text,
    }
}

diesel::table! {
    feedback_from_teams_question_answers (id) {
        id -> Text,
        feedback_id -> Text,
        question_id -> Text,
        answer -> Text,
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
    judge_room_constraints (id) {
        id -> Text,
        judge_id -> Text,
        category_id -> Text,
        pref -> BigInt,
    }
}

diesel::table! {
    rooms_of_room_categories (id) {
        id -> Text,
        category_id -> Text,
        room_id -> Text,
    }
}

diesel::table! {
    speaker_room_constraints (id) {
        id -> Text,
        speaker_id -> Text,
        category_id -> Text,
        pref -> BigInt,
    }
}

diesel::table! {
    tournament_action_logs (id) {
        id -> Text,
        snapshot_id -> Text,
        message -> Nullable<Text>,
    }
}

diesel::table! {
    tournament_ballots (id) {
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
    tournament_break_categories (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        priority -> BigInt,
    }
}

diesel::table! {
    tournament_debate_judges (id) {
        id -> Text,
        debate_id -> Text,
        judge_id -> Text,
        status -> Text,
    }
}

diesel::table! {
    tournament_debate_speaker_results (id) {
        id -> Text,
        debate_id -> Text,
        speaker_id -> Text,
        team_id -> Text,
        position -> BigInt,
        score -> Float,
    }
}

diesel::table! {
    tournament_debate_team_results (id) {
        id -> Text,
        debate_id -> Text,
        team_id -> Text,
        points -> BigInt,
    }
}

diesel::table! {
    tournament_debate_teams (id) {
        id -> Text,
        debate_id -> Text,
        team_id -> Text,
        side -> BigInt,
        seq -> BigInt,
    }
}

diesel::table! {
    tournament_debates (id) {
        id -> Text,
        tournament_id -> Text,
        round_id -> Text,
        room_id -> Nullable<Text>,
        number -> BigInt,
    }
}

diesel::table! {
    tournament_group_members (id) {
        id -> Text,
        member_id -> Text,
        group_id -> Text,
    }
}

diesel::table! {
    tournament_group_permissions (id) {
        id -> Text,
        tournament_id -> Text,
        group_id -> Text,
        permission -> Text,
    }
}

diesel::table! {
    tournament_groups (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
    }
}

diesel::table! {
    tournament_institutions (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        code -> Text,
    }
}

diesel::table! {
    tournament_judge_availability (id) {
        id -> Text,
        round_id -> Text,
        judge_id -> Text,
        available -> Bool,
        comment -> Nullable<Text>,
    }
}

diesel::table! {
    tournament_judge_judge_clash (id) {
        id -> Text,
        tournament_id -> Text,
        judge1_id -> Text,
        judge2_id -> Text,
    }
}

diesel::table! {
    tournament_judge_stated_eligibility (id) {
        id -> Text,
        round_id -> Text,
        judge_id -> Text,
        available -> Bool,
    }
}

diesel::table! {
    tournament_judge_team_clash (id) {
        id -> Text,
        tournament_id -> Text,
        judge_id -> Text,
        team_id -> Text,
    }
}

diesel::table! {
    tournament_judges (id) {
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
    tournament_members (id) {
        id -> Text,
        user_id -> Text,
        tournament_id -> Text,
        is_superuser -> Bool,
    }
}

diesel::table! {
    tournament_room_categories (id) {
        id -> Text,
        tournament_id -> Text,
        private_name -> Text,
        public_name -> Text,
        description -> Text,
    }
}

diesel::table! {
    tournament_rooms (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        url -> Nullable<Text>,
        priority -> BigInt,
        number -> BigInt,
    }
}

diesel::table! {
    tournament_round_motions (id) {
        id -> Text,
        tournament_id -> Text,
        round_id -> Text,
        infoslide -> Nullable<Text>,
        motion -> Text,
    }
}

diesel::table! {
    tournament_round_tickets (id) {
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
    tournament_rounds (id) {
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
    tournament_snapshots (id) {
        id -> Text,
        created_at -> Timestamp,
        contents -> Nullable<Text>,
        tournament_id -> Text,
        prev -> Nullable<Text>,
        schema_id -> Text,
    }
}

diesel::table! {
    tournament_speaker_metrics (id) {
        id -> Text,
        tournament_id -> Text,
        speaker_id -> Text,
        metric_kind -> Text,
        metric_value -> Float,
    }
}

diesel::table! {
    tournament_speaker_score_entries (id) {
        id -> Text,
        ballot_id -> Text,
        team_id -> Text,
        speaker_id -> Text,
        speaker_position -> BigInt,
        score -> Float,
    }
}

diesel::table! {
    tournament_speaker_standings (id) {
        id -> Text,
        tournament_id -> Text,
        speaker_id -> Text,
        rank -> BigInt,
    }
}

diesel::table! {
    tournament_speakers (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        email -> Text,
        private_url -> Text,
    }
}

diesel::table! {
    tournament_team_availability (id) {
        id -> Text,
        round_id -> Text,
        team_id -> Text,
        available -> Bool,
    }
}

diesel::table! {
    tournament_team_metrics (id) {
        id -> Text,
        tournament_id -> Text,
        team_id -> Text,
        metric_kind -> Text,
        metric_value -> Float,
    }
}

diesel::table! {
    tournament_team_speakers (id) {
        id -> Text,
        team_id -> Text,
        speaker_id -> Text,
    }
}

diesel::table! {
    tournament_team_standings (id) {
        id -> Text,
        tournament_id -> Text,
        team_id -> Text,
        rank -> BigInt,
    }
}

diesel::table! {
    tournament_teams (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        institution_id -> Nullable<Text>,
        number -> BigInt,
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
        elim_ballots_require_speaks -> Bool,
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

diesel::joinable!(feedback_from_judges_question_answers -> feedback_questions (question_id));
diesel::joinable!(feedback_from_teams_question_answers -> feedback_questions (question_id));
diesel::joinable!(feedback_of_judges -> tournament_debates (debate_id));
diesel::joinable!(feedback_of_judges -> tournament_judges (judge_id));
diesel::joinable!(feedback_of_judges -> tournaments (tournament_id));
diesel::joinable!(feedback_of_teams -> tournament_debates (debate_id));
diesel::joinable!(feedback_of_teams -> tournaments (tournament_id));
diesel::joinable!(feedback_questions -> tournaments (tournament_id));
diesel::joinable!(judge_room_constraints -> rooms_of_room_categories (category_id));
diesel::joinable!(judge_room_constraints -> tournament_judges (judge_id));
diesel::joinable!(rooms_of_room_categories -> tournament_room_categories (category_id));
diesel::joinable!(rooms_of_room_categories -> tournament_rooms (room_id));
diesel::joinable!(speaker_room_constraints -> rooms_of_room_categories (category_id));
diesel::joinable!(speaker_room_constraints -> tournament_speakers (speaker_id));
diesel::joinable!(tournament_action_logs -> tournament_snapshots (snapshot_id));
diesel::joinable!(tournament_ballots -> tournament_debates (debate_id));
diesel::joinable!(tournament_ballots -> tournament_judges (judge_id));
diesel::joinable!(tournament_ballots -> tournament_round_motions (motion_id));
diesel::joinable!(tournament_ballots -> tournaments (tournament_id));
diesel::joinable!(tournament_ballots -> users (editor_id));
diesel::joinable!(tournament_break_categories -> tournaments (tournament_id));
diesel::joinable!(tournament_debate_judges -> tournament_debates (debate_id));
diesel::joinable!(tournament_debate_judges -> tournament_judges (judge_id));
diesel::joinable!(tournament_debate_speaker_results -> tournament_debates (debate_id));
diesel::joinable!(tournament_debate_speaker_results -> tournament_speakers (speaker_id));
diesel::joinable!(tournament_debate_speaker_results -> tournament_teams (team_id));
diesel::joinable!(tournament_debate_team_results -> tournament_debates (debate_id));
diesel::joinable!(tournament_debate_team_results -> tournament_teams (team_id));
diesel::joinable!(tournament_debate_teams -> tournament_debates (debate_id));
diesel::joinable!(tournament_debate_teams -> tournament_teams (team_id));
diesel::joinable!(tournament_debates -> tournament_rooms (room_id));
diesel::joinable!(tournament_debates -> tournament_rounds (round_id));
diesel::joinable!(tournament_debates -> tournaments (tournament_id));
diesel::joinable!(tournament_group_members -> tournament_groups (group_id));
diesel::joinable!(tournament_group_members -> tournament_members (member_id));
diesel::joinable!(tournament_group_permissions -> tournament_groups (group_id));
diesel::joinable!(tournament_group_permissions -> tournaments (tournament_id));
diesel::joinable!(tournament_groups -> tournaments (tournament_id));
diesel::joinable!(tournament_institutions -> tournaments (tournament_id));
diesel::joinable!(tournament_judge_availability -> tournament_judges (judge_id));
diesel::joinable!(tournament_judge_availability -> tournament_rounds (round_id));
diesel::joinable!(tournament_judge_judge_clash -> tournaments (tournament_id));
diesel::joinable!(tournament_judge_stated_eligibility -> tournament_judges (judge_id));
diesel::joinable!(tournament_judge_stated_eligibility -> tournament_rounds (round_id));
diesel::joinable!(tournament_judge_team_clash -> tournament_judges (judge_id));
diesel::joinable!(tournament_judge_team_clash -> tournament_teams (team_id));
diesel::joinable!(tournament_judge_team_clash -> tournaments (tournament_id));
diesel::joinable!(tournament_judges -> tournament_institutions (institution_id));
diesel::joinable!(tournament_judges -> tournaments (tournament_id));
diesel::joinable!(tournament_members -> tournaments (tournament_id));
diesel::joinable!(tournament_members -> users (user_id));
diesel::joinable!(tournament_room_categories -> tournaments (tournament_id));
diesel::joinable!(tournament_rooms -> tournaments (tournament_id));
diesel::joinable!(tournament_round_motions -> tournament_rounds (round_id));
diesel::joinable!(tournament_round_motions -> tournaments (tournament_id));
diesel::joinable!(tournament_round_tickets -> tournament_rounds (round_id));
diesel::joinable!(tournament_rounds -> tournament_break_categories (break_category));
diesel::joinable!(tournament_rounds -> tournaments (tournament_id));
diesel::joinable!(tournament_snapshots -> tournaments (tournament_id));
diesel::joinable!(tournament_speaker_metrics -> tournament_speakers (speaker_id));
diesel::joinable!(tournament_speaker_metrics -> tournaments (tournament_id));
diesel::joinable!(tournament_speaker_score_entries -> tournament_ballots (ballot_id));
diesel::joinable!(tournament_speaker_score_entries -> tournament_speakers (speaker_id));
diesel::joinable!(tournament_speaker_score_entries -> tournament_teams (team_id));
diesel::joinable!(tournament_speaker_standings -> tournament_speakers (speaker_id));
diesel::joinable!(tournament_speaker_standings -> tournaments (tournament_id));
diesel::joinable!(tournament_speakers -> tournaments (tournament_id));
diesel::joinable!(tournament_team_availability -> tournament_rounds (round_id));
diesel::joinable!(tournament_team_availability -> tournament_teams (team_id));
diesel::joinable!(tournament_team_metrics -> tournament_teams (team_id));
diesel::joinable!(tournament_team_metrics -> tournaments (tournament_id));
diesel::joinable!(tournament_team_speakers -> tournament_speakers (speaker_id));
diesel::joinable!(tournament_team_speakers -> tournament_teams (team_id));
diesel::joinable!(tournament_team_standings -> tournament_teams (team_id));
diesel::joinable!(tournament_team_standings -> tournaments (tournament_id));
diesel::joinable!(tournament_teams -> tournament_institutions (institution_id));
diesel::joinable!(tournament_teams -> tournaments (tournament_id));

diesel::allow_tables_to_appear_in_same_query!(
    feedback_from_judges_question_answers,
    feedback_from_teams_question_answers,
    feedback_of_judges,
    feedback_of_teams,
    feedback_questions,
    judge_room_constraints,
    rooms_of_room_categories,
    speaker_room_constraints,
    tournament_action_logs,
    tournament_ballots,
    tournament_break_categories,
    tournament_debate_judges,
    tournament_debate_speaker_results,
    tournament_debate_team_results,
    tournament_debate_teams,
    tournament_debates,
    tournament_group_members,
    tournament_group_permissions,
    tournament_groups,
    tournament_institutions,
    tournament_judge_availability,
    tournament_judge_judge_clash,
    tournament_judge_stated_eligibility,
    tournament_judge_team_clash,
    tournament_judges,
    tournament_members,
    tournament_room_categories,
    tournament_rooms,
    tournament_round_motions,
    tournament_round_tickets,
    tournament_rounds,
    tournament_snapshots,
    tournament_speaker_metrics,
    tournament_speaker_score_entries,
    tournament_speaker_standings,
    tournament_speakers,
    tournament_team_availability,
    tournament_team_metrics,
    tournament_team_speakers,
    tournament_team_standings,
    tournament_teams,
    tournaments,
    users,
);
