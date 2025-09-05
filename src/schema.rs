// @generated automatically by Diesel CLI.

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
    tournament_debate_judges (debate_id, judge_id) {
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
        draw_id -> Text,
        room_id -> Nullable<Text>,
        number -> BigInt,
    }
}

diesel::table! {
    tournament_draws (id) {
        id -> Text,
        tournament_id -> Text,
        round_id -> Text,
        status -> Text,
        released_at -> Nullable<Timestamp>,
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
        institution_id -> Nullable<Text>,
        participant_id -> Text,
        number -> BigInt,
    }
}

diesel::table! {
    tournament_members (id) {
        id -> Text,
        user_id -> Text,
        tournament_id -> Text,
        is_superuser -> Bool,
        is_ca -> Bool,
        is_equity -> Bool,
    }
}

diesel::table! {
    tournament_participants (id) {
        id -> Text,
        tournament_id -> Text,
        private_url -> Text,
    }
}

diesel::table! {
    tournament_rooms (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        url -> Text,
        priority -> BigInt,
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
    tournament_speakers (id) {
        id -> Text,
        tournament_id -> Text,
        name -> Text,
        email -> Text,
        participant_id -> Text,
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
    tournament_team_speakers (rowid) {
        rowid -> BigInt,
        team_id -> Text,
        speaker_id -> Text,
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
        teams_per_side -> BigInt,
        substantive_speakers -> BigInt,
        reply_speakers -> Bool,
        reply_must_speak -> Bool,
        max_substantive_speech_index_for_reply -> Nullable<BigInt>,
        pool_ballot_setup -> Text,
        elim_ballot_setup -> Text,
        elim_ballots_require_speaks -> Bool,
        institution_penalty -> Nullable<BigInt>,
        history_penalty -> Nullable<BigInt>,
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
diesel::joinable!(tournament_debates -> tournament_draws (draw_id));
diesel::joinable!(tournament_debates -> tournament_rooms (room_id));
diesel::joinable!(tournament_debates -> tournaments (tournament_id));
diesel::joinable!(tournament_draws -> tournament_rounds (round_id));
diesel::joinable!(tournament_draws -> tournaments (tournament_id));
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
diesel::joinable!(tournament_judges -> tournament_participants (participant_id));
diesel::joinable!(tournament_judges -> tournaments (tournament_id));
diesel::joinable!(tournament_members -> tournaments (tournament_id));
diesel::joinable!(tournament_members -> users (user_id));
diesel::joinable!(tournament_participants -> tournaments (tournament_id));
diesel::joinable!(tournament_rooms -> tournaments (tournament_id));
diesel::joinable!(tournament_round_motions -> tournament_rounds (round_id));
diesel::joinable!(tournament_round_motions -> tournaments (tournament_id));
diesel::joinable!(tournament_round_tickets -> tournament_rounds (round_id));
diesel::joinable!(tournament_rounds -> tournament_break_categories (break_category));
diesel::joinable!(tournament_rounds -> tournaments (tournament_id));
diesel::joinable!(tournament_snapshots -> tournaments (tournament_id));
diesel::joinable!(tournament_speaker_score_entries -> tournament_ballots (ballot_id));
diesel::joinable!(tournament_speaker_score_entries -> tournament_speakers (speaker_id));
diesel::joinable!(tournament_speaker_score_entries -> tournament_teams (team_id));
diesel::joinable!(tournament_speakers -> tournament_participants (participant_id));
diesel::joinable!(tournament_speakers -> tournaments (tournament_id));
diesel::joinable!(tournament_team_availability -> tournament_rounds (round_id));
diesel::joinable!(tournament_team_availability -> tournament_teams (team_id));
diesel::joinable!(tournament_team_speakers -> tournament_speakers (speaker_id));
diesel::joinable!(tournament_team_speakers -> tournament_teams (team_id));
diesel::joinable!(tournament_teams -> tournament_institutions (institution_id));
diesel::joinable!(tournament_teams -> tournaments (tournament_id));

diesel::allow_tables_to_appear_in_same_query!(
    tournament_action_logs,
    tournament_ballots,
    tournament_break_categories,
    tournament_debate_judges,
    tournament_debate_speaker_results,
    tournament_debate_team_results,
    tournament_debate_teams,
    tournament_debates,
    tournament_draws,
    tournament_institutions,
    tournament_judge_availability,
    tournament_judge_judge_clash,
    tournament_judge_stated_eligibility,
    tournament_judge_team_clash,
    tournament_judges,
    tournament_members,
    tournament_participants,
    tournament_rooms,
    tournament_round_motions,
    tournament_round_tickets,
    tournament_rounds,
    tournament_snapshots,
    tournament_speaker_score_entries,
    tournament_speakers,
    tournament_team_availability,
    tournament_team_speakers,
    tournament_teams,
    tournaments,
    users,
);
