use crate::schema::*;
use axum::body::Bytes;
use axum_test::TestServer;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use fuzzcheck::DefaultMutator;
use fuzzcheck::Mutator;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use uuid::Uuid;

const TABDA_DICTIONARY_ANIMALS: [&str; 12] = [
    "otter", "badger", "lynx", "falcon", "gecko", "narwhal", "wombat",
    "panther", "meerkat", "ibex", "quetzal", "capybara",
];

type InnerStringMutator = fuzzcheck::mutators::grammar::ASTMutator;
type InnerStringCache =
    <InnerStringMutator as Mutator<fuzzcheck::mutators::grammar::AST>>::Cache;
type InnerStringMutationStep = <InnerStringMutator as Mutator<
    fuzzcheck::mutators::grammar::AST,
>>::MutationStep;
type InnerStringArbitraryStep = <InnerStringMutator as Mutator<
    fuzzcheck::mutators::grammar::AST,
>>::ArbitraryStep;
type InnerStringUnmutateToken = <InnerStringMutator as Mutator<
    fuzzcheck::mutators::grammar::AST,
>>::UnmutateToken;
type InnerStringAst = fuzzcheck::mutators::grammar::AST;

pub struct TabdaDictionaryStringMutator {
    inner: InnerStringMutator,
}

#[derive(Clone)]
pub struct TabdaDictionaryStringCache {
    ast: Option<InnerStringAst>,
    ast_cache: Option<InnerStringCache>,
}

#[derive(Clone)]
pub struct TabdaDictionaryStringArbitraryStep {
    inner: InnerStringArbitraryStep,
}

#[derive(Clone)]
pub struct TabdaDictionaryStringMutationStep {
    inner: Option<InnerStringMutationStep>,
    arbitrary: InnerStringArbitraryStep,
}

pub enum TabdaDictionaryStringUnmutateToken {
    ReplaceWhole {
        old_value: String,
        old_cache: TabdaDictionaryStringCache,
    },
    Inner(InnerStringUnmutateToken),
}

impl TabdaDictionaryStringMutator {
    fn new() -> Self {
        Self {
            inner: fuzzcheck::mutators::grammar::grammar_based_ast_mutator(
                tabda_dictionary_string_grammar(),
            ),
        }
    }

    fn ast_to_string(ast: &InnerStringAst) -> String {
        ast.to_string()
    }

    fn arbitrary_value_from_step(
        &self,
        step: &mut InnerStringArbitraryStep,
        max_cplx: f64,
    ) -> Option<(String, TabdaDictionaryStringCache, f64)> {
        let (ast, cplx) = self.inner.ordered_arbitrary(step, max_cplx)?;
        let ast_cache = self
            .inner
            .validate_value(&ast)
            .expect("grammar mutator should validate its own AST output");
        Some((
            Self::ast_to_string(&ast),
            TabdaDictionaryStringCache {
                ast: Some(ast),
                ast_cache: Some(ast_cache),
            },
            cplx,
        ))
    }

    fn random_value(
        &self,
        max_cplx: f64,
    ) -> (String, TabdaDictionaryStringCache, f64) {
        let (ast, cplx) = self.inner.random_arbitrary(max_cplx);
        let ast_cache = self
            .inner
            .validate_value(&ast)
            .expect("grammar mutator should validate its own AST output");
        (
            Self::ast_to_string(&ast),
            TabdaDictionaryStringCache {
                ast: Some(ast),
                ast_cache: Some(ast_cache),
            },
            cplx,
        )
    }

    fn fallback_cache() -> TabdaDictionaryStringCache {
        TabdaDictionaryStringCache {
            ast: None,
            ast_cache: None,
        }
    }
}

fn literal_string(value: &str) -> Rc<fuzzcheck::mutators::grammar::Grammar> {
    fuzzcheck::mutators::grammar::concatenation(
        value.chars().map(fuzzcheck::mutators::grammar::literal),
    )
}

fn animal_grammar() -> Rc<fuzzcheck::mutators::grammar::Grammar> {
    fuzzcheck::mutators::grammar::alternation(
        TABDA_DICTIONARY_ANIMALS
            .iter()
            .map(|animal| literal_string(animal)),
    )
}

fn email_grammar() -> Rc<fuzzcheck::mutators::grammar::Grammar> {
    fuzzcheck::mutators::grammar::regex(
        "[a-z]{1,12}@[a-z]{1,10}\\.(com|org|net|edu)",
    )
}

fn tabda_dictionary_string_grammar() -> Rc<fuzzcheck::mutators::grammar::Grammar>
{
    use fuzzcheck::mutators::grammar::{
        alternation, concatenation, recurse, recursive, regex,
    };

    let animal = animal_grammar();
    let email = email_grammar();
    let ascii = regex("[ -~]");

    recursive(|whole: &Weak<fuzzcheck::mutators::grammar::Grammar>| {
        alternation([
            animal.clone(),
            animal.clone(),
            animal.clone(),
            email.clone(),
            ascii.clone(),
            concatenation([recurse(whole), recurse(whole)]),
        ])
    })
}

impl Mutator<String> for TabdaDictionaryStringMutator {
    type Cache = TabdaDictionaryStringCache;
    type MutationStep = TabdaDictionaryStringMutationStep;
    type ArbitraryStep = TabdaDictionaryStringArbitraryStep;
    type UnmutateToken = TabdaDictionaryStringUnmutateToken;

    fn initialize(&self) {
        self.inner.initialize();
    }

    fn default_arbitrary_step(&self) -> Self::ArbitraryStep {
        Self::ArbitraryStep {
            inner: self.inner.default_arbitrary_step(),
        }
    }

    fn is_valid(&self, value: &String) -> bool {
        value.is_ascii()
    }

    fn validate_value(&self, value: &String) -> Option<Self::Cache> {
        if self.is_valid(value) {
            Some(Self::fallback_cache())
        } else {
            None
        }
    }

    fn default_mutation_step(
        &self,
        _value: &String,
        cache: &Self::Cache,
    ) -> Self::MutationStep {
        Self::MutationStep {
            inner: cache.ast.as_ref().zip(cache.ast_cache.as_ref()).map(
                |(ast, ast_cache)| {
                    self.inner.default_mutation_step(ast, ast_cache)
                },
            ),
            arbitrary: self.inner.default_arbitrary_step(),
        }
    }

    fn global_search_space_complexity(&self) -> f64 {
        self.inner.global_search_space_complexity()
    }

    fn max_complexity(&self) -> f64 {
        self.inner.max_complexity()
    }

    fn min_complexity(&self) -> f64 {
        self.inner.min_complexity()
    }

    fn complexity(&self, value: &String, cache: &Self::Cache) -> f64 {
        match (cache.ast.as_ref(), cache.ast_cache.as_ref()) {
            (Some(ast), Some(ast_cache)) => {
                self.inner.complexity(ast, ast_cache)
            }
            _ => (value.len() * 8) as f64,
        }
    }

    fn ordered_arbitrary(
        &self,
        step: &mut Self::ArbitraryStep,
        max_cplx: f64,
    ) -> Option<(String, f64)> {
        self.arbitrary_value_from_step(&mut step.inner, max_cplx)
            .map(|(value, _, cplx)| (value, cplx))
    }

    fn random_arbitrary(&self, max_cplx: f64) -> (String, f64) {
        let (value, _, cplx) = self.random_value(max_cplx);
        (value, cplx)
    }

    fn ordered_mutate(
        &self,
        value: &mut String,
        cache: &mut Self::Cache,
        step: &mut Self::MutationStep,
        subvalue_provider: &dyn fuzzcheck::SubValueProvider,
        max_cplx: f64,
    ) -> Option<(Self::UnmutateToken, f64)> {
        if let (Some(ast), Some(ast_cache), Some(ast_step)) = (
            cache.ast.as_mut(),
            cache.ast_cache.as_mut(),
            step.inner.as_mut(),
        ) {
            if let Some((token, cplx)) = self.inner.ordered_mutate(
                ast,
                ast_cache,
                ast_step,
                subvalue_provider,
                max_cplx,
            ) {
                *value = Self::ast_to_string(ast);
                return Some((
                    TabdaDictionaryStringUnmutateToken::Inner(token),
                    cplx,
                ));
            }
        }

        let old_value = value.clone();
        let old_cache = cache.clone();
        let (new_value, new_cache, cplx) =
            self.arbitrary_value_from_step(&mut step.arbitrary, max_cplx)?;
        *value = new_value;
        *cache = new_cache;
        step.inner = cache.ast.as_ref().zip(cache.ast_cache.as_ref()).map(
            |(ast, ast_cache)| self.inner.default_mutation_step(ast, ast_cache),
        );
        Some((
            TabdaDictionaryStringUnmutateToken::ReplaceWhole {
                old_value,
                old_cache,
            },
            cplx,
        ))
    }

    fn random_mutate(
        &self,
        value: &mut String,
        cache: &mut Self::Cache,
        max_cplx: f64,
    ) -> (Self::UnmutateToken, f64) {
        if let (Some(ast), Some(ast_cache)) =
            (cache.ast.as_mut(), cache.ast_cache.as_mut())
        {
            let (token, cplx) =
                self.inner.random_mutate(ast, ast_cache, max_cplx);
            *value = Self::ast_to_string(ast);
            (TabdaDictionaryStringUnmutateToken::Inner(token), cplx)
        } else {
            let old_value = value.clone();
            let old_cache = cache.clone();
            let (new_value, new_cache, cplx) = self.random_value(max_cplx);
            *value = new_value;
            *cache = new_cache;
            (
                TabdaDictionaryStringUnmutateToken::ReplaceWhole {
                    old_value,
                    old_cache,
                },
                cplx,
            )
        }
    }

    fn unmutate(
        &self,
        value: &mut String,
        cache: &mut Self::Cache,
        t: Self::UnmutateToken,
    ) {
        match t {
            TabdaDictionaryStringUnmutateToken::ReplaceWhole {
                old_value,
                old_cache,
            } => {
                *value = old_value;
                *cache = old_cache;
            }
            TabdaDictionaryStringUnmutateToken::Inner(token) => {
                if let (Some(ast), Some(ast_cache)) =
                    (cache.ast.as_mut(), cache.ast_cache.as_mut())
                {
                    self.inner.unmutate(ast, ast_cache, token);
                    *value = Self::ast_to_string(ast);
                }
            }
        }
    }

    fn visit_subvalues<'a>(
        &self,
        value: &'a String,
        cache: &'a Self::Cache,
        visit: &mut dyn FnMut(&'a dyn Any, f64),
    ) {
        if let (Some(ast), Some(ast_cache)) =
            (cache.ast.as_ref(), cache.ast_cache.as_ref())
        {
            self.inner.visit_subvalues(ast, ast_cache, visit);
        } else {
            visit(value, self.complexity(value, cache));
        }
    }
}

macro_rules! get_id_by_idx {
    ($conn:expr, $query:expr, $idx:expr $(,)?) => {{
        let count = $query.count().get_result::<i64>($conn).unwrap_or(0);
        if count == 0 {
            None
        } else {
            let offset = ($idx as i64) % count;
            $query
                .offset(offset)
                .limit(1)
                .get_result::<String>($conn)
                .ok()
        }
    }};
}

#[derive(DefaultMutator, Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    // Auth
    RegisterUser {
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        username: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        password: String,
    },
    LogoutUser,
    LoginUser {
        user_idx: usize,
    },
    // Tournaments
    CreateTournament {
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        abbrv: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        slug: String,
    },
    CreateBreakCategory {
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        priority: i64,
    },
    UpdateTournamentConfiguration {
        tournament_idx: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        abbrv: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slug: Option<String>,
        #[serde(default)]
        show_draws: bool,
        #[serde(default)]
        show_round_results: bool,
        #[serde(default)]
        team_tab_public: bool,
        #[serde(default)]
        speaker_tab_public: bool,
        #[serde(default)]
        standings_public: bool,
    },

    // Participants
    CreateTeam {
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        institution_idx: Option<usize>,
    },
    EditTeam {
        tournament_idx: usize,
        team_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        institution_idx: Option<usize>,
    },
    CreateSpeaker {
        tournament_idx: usize,
        team_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
    },
    CreateJudge {
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
        institution_idx: Option<usize>,
    },
    EditJudge {
        tournament_idx: usize,
        judge_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
        institution_idx: Option<usize>,
    },

    // Constraints
    AddConstraint {
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        ptype: String,
        pid_idx: usize,
        category_idx: usize,
    },
    RemoveConstraint {
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        ptype: String,
        pid_idx: usize,
        constraint_idx: usize,
    },

    // Rooms
    CreateRoom {
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        priority: i64,
    },
    DeleteRoom {
        tournament_idx: usize,
        room_idx: usize,
    },
    CreateRoomCategory {
        tournament_idx: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default)]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        private_name: String,
        #[serde(default)]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        public_name: String,
        #[serde(default)]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        description: String,
    },
    DeleteRoomCategory {
        tournament_idx: usize,
        category_idx: usize,
    },
    AddRoomToCategory {
        tournament_idx: usize,
        category_idx: usize,
        room_idx: usize,
    },
    RemoveRoomFromCategory {
        tournament_idx: usize,
        category_idx: usize,
        room_idx: usize,
    },

    // Feedback
    AddFeedbackQuestion {
        tournament_idx: usize,
        #[serde(alias = "question_text")]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        question: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        question_type: String,
        #[serde(default)]
        for_judges: bool,
        #[serde(default)]
        for_teams: bool,
    },
    DeleteFeedbackQuestion {
        tournament_idx: usize,
        question_idx: usize,
    },

    // Rounds & Draws
    CreateRound {
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        category_idx: Option<usize>,
        #[serde(default = "default_round_seq")]
        seq: u32,
    },
    CreateMotion {
        tournament_idx: usize,
        round_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        motion: String,
        infoslide: Option<String>,
    },
    GenerateDraw {
        tournament_idx: usize,
        round_idx: usize,
    },
    SetDrawPublished {
        tournament_idx: usize,
        round_idx: usize,
        #[serde(default)]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        published: Option<bool>,
    },
    SetRoundCompleted {
        tournament_idx: usize,
        round_idx: usize,
        completed: bool,
    },
    PublishMotions {
        tournament_idx: usize,
        round_idx: usize,
    },
    PublishResults {
        tournament_idx: usize,
        round_idx: usize,
    },

    // Availability & Eligibility
    UpdateJudgeAvailability {
        tournament_idx: usize,
        round_idx: usize,
        judge_idx: usize,
        available: bool,
    },
    UpdateAllJudgeAvailability {
        tournament_idx: usize,
        round_idx: usize,
        available: bool,
    },
    UpdateTeamEligibility {
        tournament_idx: usize,
        round_idx: usize,
        team_idx: usize,
        eligible: bool,
    },
    UpdateAllTeamEligibility {
        tournament_idx: usize,
        round_idx: usize,
        eligible: bool,
    },

    // Ballots
    SubmitBallot {
        tournament_idx: usize,
        private_url_idx: usize,
        round_idx: usize,
        form: FuzzerBallotForm,
    },
}

fn default_round_seq() -> u32 {
    1
}

#[derive(DefaultMutator, Clone, Debug, Serialize, Deserialize)]
pub struct FuzzerBallotForm {
    pub motion_idx: usize,
    pub teams: Vec<FuzzerBallotTeamEntry>,
    pub expected_version: i64,
}

#[derive(DefaultMutator, Clone, Debug, Serialize, Deserialize)]
pub struct FuzzerBallotTeamEntry {
    pub speaker_indices: Vec<(usize, Option<i32>)>,
    pub points: Option<usize>,
}

#[derive(Default)]
pub struct FuzzState {
    user_passwords: HashMap<String, String>,
    logged_in: bool,
}

impl FuzzState {
    fn remember_user_password(&mut self, user_id: String, password: String) {
        self.user_passwords.entry(user_id).or_insert(password);
    }

    fn password_for_user(&self, user_id: &str) -> Option<&str> {
        self.user_passwords.get(user_id).map(String::as_str)
    }
}

fn normalize_username(input: String) -> String {
    let mut value: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if value.is_empty() {
        value = "user".to_string();
    }
    if value.len() > 32 {
        value.truncate(32);
    }
    value
}

fn normalize_email(input: String) -> String {
    let lower = input.to_ascii_lowercase();
    let filtered: String = lower
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-')
        })
        .collect();
    let parts: Vec<_> = filtered.split('@').collect();
    if parts.len() == 2 && parts[1].contains('.') && !parts[0].is_empty() {
        return filtered;
    }
    let stem: String = filtered
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let stem = if stem.is_empty() { "otter" } else { &stem };
    format!("{stem}@tabda.org")
}

fn normalize_password(input: String) -> String {
    let mut value: String =
        input.chars().filter(|c| c.is_ascii_graphic()).collect();
    if value.len() < 6 {
        value.push_str("wombat");
    }
    if value.len() > 32 {
        value.truncate(32);
    }
    value
}

fn normalize_name(input: String, min_len: usize, max_len: usize) -> String {
    let mut value: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
        .collect();
    if value.trim().is_empty() {
        value = "tabda".to_string();
    }
    while value.len() < min_len {
        value.push('a');
    }
    if value.len() > max_len {
        value.truncate(max_len);
    }
    value
}

fn normalize_slug(input: String) -> String {
    let mut value: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if value.is_empty() {
        value = "tabda1".to_string();
    }
    if value.len() > 16 {
        value.truncate(16);
    }
    value
}

fn normalize_participant_kind(input: String) -> &'static str {
    if input.eq_ignore_ascii_case("speaker")
        || input.eq_ignore_ascii_case("speakers")
    {
        "speaker"
    } else {
        "judge"
    }
}

fn normalize_feedback_kind(input: String) -> &'static str {
    match input.to_ascii_lowercase().as_str() {
        "score" => "score",
        "text" => "text",
        "bool" => "bool",
        _ => "score",
    }
}

fn normalize_draw_status(
    input: String,
    published: Option<bool>,
) -> &'static str {
    match input.as_str() {
        "confirmed" => "confirmed",
        "released_teams" => "released_teams",
        "released_full" => "released_full",
        _ => {
            if published.unwrap_or(false) {
                "released_full"
            } else {
                "confirmed"
            }
        }
    }
}

impl Action {
    #[tracing::instrument(skip_all)]
    pub async fn run(
        self,
        pool: &Pool<ConnectionManager<SqliteConnection>>,
        client: &mut TestServer,
        state: &mut FuzzState,
    ) {
        tracing::info!("Running Action::run");
        match self {
            Action::RegisterUser {
                username,
                email,
                password,
            } => {
                let username = normalize_username(username);
                let email = normalize_email(email);
                let password = normalize_password(password);
                let password2 = password.clone();
                let lookup_username = username.clone();
                let lookup_email = email.clone();
                let form = [
                    ("username", username),
                    ("email", email),
                    ("password", password),
                    ("password2", password2.clone()),
                ];
                let res = client.post("/register").form(&form).await;
                assert!(
                    !res.status_code().is_server_error(),
                    "{:?}",
                    res.status_code()
                );

                let mut conn = pool.get().unwrap();
                if let Ok(user_id) = users::table
                    .filter(
                        users::username
                            .eq(lookup_username)
                            .and(users::email.eq(lookup_email)),
                    )
                    .select(users::id)
                    .first::<String>(&mut *conn)
                {
                    state.remember_user_password(user_id, password2);
                    state.logged_in = true;
                }
            }
            Action::LoginUser { user_idx } => {
                let mut conn = pool.get().unwrap();
                if let Some(user_id) = get_id_by_idx!(
                    &mut *conn,
                    users::table.select(users::id).order_by(users::id),
                    user_idx,
                ) {
                    let login_id: String = users::table
                        .filter(users::id.eq(&user_id))
                        .select(users::username)
                        .first(&mut *conn)
                        .unwrap();
                    drop(conn);

                    if let Some(password) = state.password_for_user(&user_id) {
                        let form = [
                            ("id", login_id),
                            ("password", password.to_string()),
                        ];
                        client.post("/login").form(&form).await;
                        state.logged_in = true;
                    }
                }
            }
            Action::LogoutUser => {
                if state.logged_in {
                    client.post("/logout").await;
                    state.logged_in = false;
                }
            }
            Action::CreateTournament { name, abbrv, slug } => {
                let name = normalize_name(name, 4, 32);
                let abbrv = normalize_slug(abbrv);
                let abbrv = if abbrv.len() < 2 {
                    format!("{abbrv}x")
                } else if abbrv.len() > 8 {
                    abbrv[..8].to_string()
                } else {
                    abbrv
                };
                let slug = normalize_slug(slug);
                let form = [("name", name), ("abbrv", abbrv), ("slug", slug)];
                let res = client.post("/tournaments/create").form(&form).await;
                assert!(
                    !res.status_code().is_server_error(),
                    "{:?}",
                    res.status_code()
                );
            }
            Action::CreateBreakCategory {
                tournament_idx,
                name,
                priority,
            } => {
                let name = normalize_name(name, 4, 32);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let next_priority = if priority < 0 { 0 } else { priority };
                    let _ = diesel::insert_into(break_categories::table)
                        .values((
                            break_categories::id.eq(Uuid::now_v7().to_string()),
                            break_categories::tournament_id.eq(tid),
                            break_categories::name.eq(name),
                            break_categories::priority.eq(next_priority),
                        ))
                        .execute(&mut *conn);
                }
            }
            Action::UpdateTournamentConfiguration {
                tournament_idx,
                name: _name,
                abbrv: _abbrv,
                slug: _slug,
                show_draws,
                show_round_results,
                team_tab_public,
                speaker_tab_public,
                standings_public,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    drop(conn);
                    let mut conn = pool.get().unwrap();
                    if let Ok(tournament) =
                        crate::tournaments::Tournament::fetch(&tid, &mut *conn)
                    {
                        let mut config = crate::tournaments::manage::config::config_of_tournament(&tournament);
                        config.show_draws = show_draws;
                        config.show_round_results = show_round_results;
                        config.team_tab_public = team_tab_public;
                        config.speaker_tab_public = speaker_tab_public;
                        config.standings_public = standings_public;
                        let config = toml::to_string(&config).unwrap();
                        drop(conn);
                        let form = [("config", config)];
                        client
                            .post(&format!(
                                "/tournaments/{}/configuration",
                                tid
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::CreateTeam {
                tournament_idx,
                name,
                institution_idx,
            } => {
                let name = normalize_name(name, 4, 32);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let inst_id = institution_idx
                        .and_then(|idx| {
                            get_id_by_idx!(
                                &mut *conn,
                                institutions::table
                                    .filter(
                                        institutions::tournament_id.eq(&tid),
                                    )
                                    .select(institutions::id)
                                    .order_by(institutions::id),
                                idx,
                            )
                        })
                        .unwrap_or_else(|| "-----".to_string());
                    drop(conn);

                    let form = [("name", name), ("institution_id", inst_id)];
                    client
                        .post(&format!("/tournaments/{}/teams/create", tid))
                        .form(&form)
                        .await;
                }
            }
            Action::EditTeam {
                tournament_idx,
                team_idx,
                name,
                institution_idx,
            } => {
                let name = normalize_name(name, 4, 32);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(team_id) = get_id_by_idx!(
                        &mut *conn,
                        teams::table
                            .filter(teams::tournament_id.eq(&tid))
                            .select(teams::id)
                            .order_by(teams::id),
                        team_idx,
                    ) {
                        let inst_id = institution_idx
                            .and_then(|idx| {
                                get_id_by_idx!(
                                    &mut *conn,
                                    institutions::table
                                        .filter(
                                            institutions::tournament_id
                                                .eq(&tid),
                                        )
                                        .select(institutions::id)
                                        .order_by(institutions::id),
                                    idx,
                                )
                            })
                            .unwrap_or_else(|| "-----".to_string());
                        drop(conn);
                        let form =
                            [("name", name), ("institution_id", inst_id)];
                        client
                            .post(&format!(
                                "/tournaments/{}/teams/{}/edit",
                                tid, team_id
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::CreateJudge {
                tournament_idx,
                name,
                email,
                institution_idx,
            } => {
                let name = normalize_name(name, 1, 64);
                let email = normalize_email(email);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let inst_id = institution_idx
                        .and_then(|idx| {
                            get_id_by_idx!(
                                &mut *conn,
                                institutions::table
                                    .filter(
                                        institutions::tournament_id.eq(&tid),
                                    )
                                    .select(institutions::id)
                                    .order_by(institutions::id),
                                idx,
                            )
                        })
                        .unwrap_or_else(|| "-----".to_string());
                    drop(conn);

                    let form = [
                        ("name", name),
                        ("email", email),
                        ("institution_id", inst_id),
                    ];
                    client
                        .post(&format!("/tournaments/{}/judges/create", tid))
                        .form(&form)
                        .await;
                }
            }
            Action::EditJudge {
                tournament_idx,
                judge_idx,
                name,
                email,
                institution_idx,
            } => {
                let name = normalize_name(name, 1, 64);
                let email = normalize_email(email);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(judge_id) = get_id_by_idx!(
                        &mut *conn,
                        judges::table
                            .filter(judges::tournament_id.eq(&tid))
                            .select(judges::id)
                            .order_by(judges::id),
                        judge_idx,
                    ) {
                        let inst_id = institution_idx
                            .and_then(|idx| {
                                get_id_by_idx!(
                                    &mut *conn,
                                    institutions::table
                                        .filter(
                                            institutions::tournament_id
                                                .eq(&tid),
                                        )
                                        .select(institutions::id)
                                        .order_by(institutions::id),
                                    idx,
                                )
                            })
                            .unwrap_or_else(|| "-----".to_string());
                        drop(conn);
                        let form = [
                            ("name", name),
                            ("email", email),
                            ("institution_id", inst_id),
                        ];
                        client
                            .post(&format!(
                                "/tournaments/{}/judges/{}/edit",
                                tid, judge_id
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::CreateSpeaker {
                tournament_idx,
                team_idx,
                name,
                email,
            } => {
                let name = normalize_name(name, 1, 64);
                let email = normalize_email(email);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(team_id) = get_id_by_idx!(
                        &mut *conn,
                        teams::table
                            .filter(teams::tournament_id.eq(&tid))
                            .select(teams::id)
                            .order_by(teams::id),
                        team_idx,
                    ) {
                        drop(conn);
                        let form = [("name", name), ("email", email)];
                        client
                            .post(&format!(
                                "/tournaments/{}/teams/{}/speakers/create",
                                tid, team_id
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::AddConstraint {
                tournament_idx,
                ptype,
                pid_idx,
                category_idx,
            } => {
                let ptype = normalize_participant_kind(ptype);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let pid = if ptype == "speaker" {
                        get_id_by_idx!(
                            &mut *conn,
                            speakers::table
                                .filter(speakers::tournament_id.eq(&tid))
                                .select(speakers::id)
                                .order_by(speakers::id),
                            pid_idx,
                        )
                    } else {
                        get_id_by_idx!(
                            &mut *conn,
                            judges::table
                                .filter(judges::tournament_id.eq(&tid))
                                .select(judges::id)
                                .order_by(judges::id),
                            pid_idx,
                        )
                    };

                    if let (Some(pid), Some(cat_id)) = (
                        pid,
                        get_id_by_idx!(
                            &mut *conn,
                            room_categories::table
                                .filter(room_categories::tournament_id.eq(&tid))
                                .select(room_categories::id)
                                .order_by(room_categories::id),
                            category_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [("category_id", cat_id)];
                        client.post(&format!("/tournaments/{}/participants/{}/{}/constraints/add", tid, ptype, pid)).form(&form).await;
                    }
                }
            }
            Action::RemoveConstraint {
                tournament_idx,
                ptype,
                pid_idx,
                constraint_idx,
            } => {
                let ptype = normalize_participant_kind(ptype);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let pid = if ptype == "speaker" {
                        get_id_by_idx!(
                            &mut *conn,
                            speakers::table
                                .filter(speakers::tournament_id.eq(&tid))
                                .select(speakers::id)
                                .order_by(speakers::id),
                            pid_idx,
                        )
                    } else {
                        get_id_by_idx!(
                            &mut *conn,
                            judges::table
                                .filter(judges::tournament_id.eq(&tid))
                                .select(judges::id)
                                .order_by(judges::id),
                            pid_idx,
                        )
                    };

                    if let Some(pid) = pid {
                        if ptype == "speaker" {
                            let query = speaker_room_constraints::table
                                .filter(
                                    speaker_room_constraints::speaker_id
                                        .eq(&pid),
                                )
                                .select(speaker_room_constraints::category_id)
                                .order_by(speaker_room_constraints::pref);

                            if let Some(cat_id) = get_id_by_idx!(
                                &mut *conn,
                                query,
                                constraint_idx
                            ) {
                                drop(conn);
                                let form = [("category_id", cat_id)];
                                client
                                    .post(&format!(
                                        "/tournaments/{}/participants/speaker/{}/constraints/remove",
                                        tid, pid
                                    ))
                                    .form(&form)
                                    .await;
                            }
                        } else {
                            let query = judge_room_constraints::table
                                .filter(
                                    judge_room_constraints::judge_id.eq(&pid),
                                )
                                .select(judge_room_constraints::category_id)
                                .order_by(judge_room_constraints::pref);

                            if let Some(cat_id) = get_id_by_idx!(
                                &mut *conn,
                                query,
                                constraint_idx
                            ) {
                                drop(conn);
                                let form = [("category_id", cat_id)];
                                client
                                    .post(&format!(
                                        "/tournaments/{}/participants/judge/{}/constraints/remove",
                                        tid, pid
                                    ))
                                    .form(&form)
                                    .await;
                            }
                        }
                    }
                }
            }
            Action::CreateRoom {
                tournament_idx,
                name,
                priority,
            } => {
                let name = normalize_name(name, 1, 64);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    drop(conn);
                    let form =
                        [("name", name), ("priority", priority.to_string())];
                    client
                        .post(&format!("/tournaments/{}/rooms/create", tid))
                        .form(&form)
                        .await;
                }
            }
            Action::DeleteRoom {
                tournament_idx,
                room_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(room_id) = get_id_by_idx!(
                        &mut *conn,
                        rooms::table
                            .filter(rooms::tournament_id.eq(&tid))
                            .select(rooms::id)
                            .order_by(rooms::id),
                        room_idx,
                    ) {
                        drop(conn);
                        client
                            .post(&format!(
                                "/tournaments/{}/rooms/{}/delete",
                                tid, room_id
                            ))
                            .await;
                    }
                }
            }
            Action::CreateRoomCategory {
                tournament_idx,
                name,
                private_name,
                public_name,
                description,
            } => {
                let legacy_name = name.unwrap_or_default();
                let private_name = normalize_name(
                    if private_name.is_empty() {
                        legacy_name.clone()
                    } else {
                        private_name
                    },
                    1,
                    64,
                );
                let public_name = normalize_name(
                    if public_name.is_empty() {
                        legacy_name
                    } else {
                        public_name
                    },
                    1,
                    64,
                );
                let description = normalize_name(description, 0, 128);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    drop(conn);
                    let form = [
                        ("private_name", private_name),
                        ("public_name", public_name),
                        ("description", description),
                    ];
                    client
                        .post(&format!(
                            "/tournaments/{}/rooms/categories/create",
                            tid
                        ))
                        .form(&form)
                        .await;
                }
            }
            Action::DeleteRoomCategory {
                tournament_idx,
                category_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(cat_id) = get_id_by_idx!(
                        &mut *conn,
                        room_categories::table
                            .filter(room_categories::tournament_id.eq(&tid))
                            .select(room_categories::id)
                            .order_by(room_categories::id),
                        category_idx,
                    ) {
                        drop(conn);
                        client
                            .post(&format!(
                                "/tournaments/{}/rooms/categories/{}/delete",
                                tid, cat_id
                            ))
                            .await;
                    }
                }
            }
            Action::AddRoomToCategory {
                tournament_idx,
                category_idx,
                room_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let (Some(cat_id), Some(room_id)) = (
                        get_id_by_idx!(
                            &mut *conn,
                            room_categories::table
                                .filter(room_categories::tournament_id.eq(&tid))
                                .select(room_categories::id)
                                .order_by(room_categories::id),
                            category_idx,
                        ),
                        get_id_by_idx!(
                            &mut *conn,
                            rooms::table
                                .filter(rooms::tournament_id.eq(&tid))
                                .select(rooms::id)
                                .order_by(rooms::id),
                            room_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [("room_id", room_id)];
                        client
                            .post(&format!(
                                "/tournaments/{}/rooms/categories/{}/add_room",
                                tid, cat_id
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::RemoveRoomFromCategory {
                tournament_idx,
                category_idx,
                room_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let (Some(cat_id), Some(room_id)) = (
                        get_id_by_idx!(
                            &mut *conn,
                            room_categories::table
                                .filter(room_categories::tournament_id.eq(&tid))
                                .select(room_categories::id)
                                .order_by(room_categories::id),
                            category_idx,
                        ),
                        get_id_by_idx!(
                            &mut *conn,
                            rooms::table
                                .filter(rooms::tournament_id.eq(&tid))
                                .select(rooms::id)
                                .order_by(rooms::id),
                            room_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [("room_id", room_id)];
                        client.post(&format!("/tournaments/{}/rooms/categories/{}/remove_room", tid, cat_id)).form(&form).await;
                    }
                }
            }
            Action::AddFeedbackQuestion {
                tournament_idx,
                question,
                question_type,
                for_judges,
                for_teams,
            } => {
                let question = normalize_name(question, 4, 128);
                let question_type = normalize_feedback_kind(question_type);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    drop(conn);
                    let form = [
                        ("question", question),
                        ("kind", question_type.to_string()),
                        (
                            "for_judges",
                            if for_judges { "true" } else { "false" }
                                .to_string(),
                        ),
                        (
                            "for_teams",
                            if for_teams { "true" } else { "false" }
                                .to_string(),
                        ),
                    ];
                    client
                        .post(&format!(
                            "/tournaments/{}/feedback/manage/add",
                            tid
                        ))
                        .form(&form)
                        .await;
                }
            }
            Action::DeleteFeedbackQuestion {
                tournament_idx,
                question_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(q_id) = get_id_by_idx!(
                        &mut *conn,
                        feedback_questions::table
                            .filter(feedback_questions::tournament_id.eq(&tid))
                            .select(feedback_questions::id)
                            .order_by(feedback_questions::id),
                        question_idx,
                    ) {
                        drop(conn);
                        let form = [("question_id", q_id)];
                        client
                            .post(&format!(
                                "/tournaments/{}/feedback/manage/delete",
                                tid
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::CreateRound {
                tournament_idx,
                name,
                category_idx,
                seq,
            } => {
                // map (deterministically) to [0,199]
                let seq = seq % 200;
                let name = normalize_name(name, 4, 32);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let category_id = category_idx
                        .and_then(|idx| {
                            get_id_by_idx!(
                                &mut *conn,
                                break_categories::table
                                    .filter(
                                        break_categories::tournament_id
                                            .eq(&tid)
                                    )
                                    .select(break_categories::id)
                                    .order_by(break_categories::id),
                                idx,
                            )
                        })
                        .unwrap_or_else(|| "in_round".to_string());
                    drop(conn);
                    let form = [("name", name), ("seq", seq.to_string())];
                    client
                        .post(&format!(
                            "/tournaments/{}/rounds/{}/create",
                            tid, category_id
                        ))
                        .form(&form)
                        .await;
                }
            }
            Action::CreateMotion {
                tournament_idx,
                round_idx,
                motion,
                infoslide,
            } => {
                let motion = normalize_name(motion, 8, 160);
                let infoslide = infoslide.map(|s| normalize_name(s, 0, 160));
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        let _ = diesel::insert_into(motions_of_round::table)
                            .values((
                                motions_of_round::id
                                    .eq(Uuid::now_v7().to_string()),
                                motions_of_round::tournament_id.eq(tid),
                                motions_of_round::round_id.eq(rid),
                                motions_of_round::infoslide.eq(infoslide),
                                motions_of_round::motion.eq(motion),
                                motions_of_round::published_at
                                    .eq(None::<chrono::NaiveDateTime>),
                            ))
                            .execute(&mut *conn);
                    }
                }
            }
            Action::GenerateDraw {
                tournament_idx,
                round_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/draws/create",
                                tid, rid
                            ))
                            .await;
                    }
                }
            }
            Action::SetDrawPublished {
                tournament_idx,
                round_idx,
                status,
                published,
            } => {
                let status = normalize_draw_status(status, published);
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        let form = [("status", status)];
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/draws/setreleased",
                                tid, rid
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::SetRoundCompleted {
                tournament_idx,
                round_idx,
                completed,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        let form = [(
                            "completed",
                            if completed { "true" } else { "false" },
                        )];
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/complete",
                                tid, rid
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::PublishMotions {
                tournament_idx,
                round_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/motions/publish",
                                tid, rid
                            ))
                            .await;
                    }
                }
            }
            Action::PublishResults {
                tournament_idx,
                round_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        let form = [("published", "true")];
                        client
                            .post(&format!(
                                "/tournaments/{}/rounds/{}/results/publish",
                                tid, rid
                            ))
                            .form(&form)
                            .await;
                    }
                }
            }
            Action::UpdateJudgeAvailability {
                tournament_idx,
                round_idx,
                judge_idx,
                available,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let (Some(rid), Some(jid)) = (
                        get_id_by_idx!(
                            &mut *conn,
                            rounds::table
                                .filter(rounds::tournament_id.eq(&tid))
                                .select(rounds::id)
                                .order_by(rounds::id),
                            round_idx,
                        ),
                        get_id_by_idx!(
                            &mut *conn,
                            judges::table
                                .filter(judges::tournament_id.eq(&tid))
                                .select(judges::id)
                                .order_by(judges::id),
                            judge_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [
                            ("judge", jid),
                            (
                                "available",
                                if available { "on" } else { "" }.to_string(),
                            ),
                        ];
                        client.post(&format!("/tournaments/{}/rounds/{}/update_judge_availability", tid, rid)).form(&form).await;
                    }
                }
            }
            Action::UpdateAllJudgeAvailability {
                tournament_idx,
                round_idx,
                available,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        let check = if available { "in" } else { "out" };
                        client.post(&format!("/tournaments/{}/rounds/{}/availability/judges/all?check={}", tid, rid, check)).await;
                    }
                }
            }
            Action::UpdateTeamEligibility {
                tournament_idx,
                round_idx,
                team_idx,
                eligible,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let (Some(rid), Some(team_id)) = (
                        get_id_by_idx!(
                            &mut *conn,
                            rounds::table
                                .filter(rounds::tournament_id.eq(&tid))
                                .select(rounds::id)
                                .order_by(rounds::id),
                            round_idx,
                        ),
                        get_id_by_idx!(
                            &mut *conn,
                            teams::table
                                .filter(teams::tournament_id.eq(&tid))
                                .select(teams::id)
                                .order_by(teams::id),
                            team_idx,
                        ),
                    ) {
                        drop(conn);
                        let form = [
                            ("team", team_id),
                            (
                                "available",
                                if eligible { "true" } else { "false" }
                                    .to_string(),
                            ),
                        ];
                        client.post(&format!("/tournaments/{}/rounds/{}/update_team_eligibility", tid, rid)).form(&form).await;
                    }
                }
            }
            Action::UpdateAllTeamEligibility {
                tournament_idx,
                round_idx,
                eligible,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    if let Some(rid) = get_id_by_idx!(
                        &mut *conn,
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select(rounds::id)
                            .order_by(rounds::id),
                        round_idx,
                    ) {
                        drop(conn);
                        let check = if eligible { "in" } else { "out" };
                        client.post(&format!("/tournaments/{}/rounds/{}/availability/teams/all?check={}", tid, rid, check)).await;
                    }
                }
            }
            Action::SubmitBallot {
                tournament_idx,
                private_url_idx,
                round_idx,
                form: f_form,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let judges_with_urls: Vec<(String, String)> = judges::table
                        .filter(judges::tournament_id.eq(&tid))
                        .select((judges::id, judges::private_url))
                        .load(&mut *conn)
                        .unwrap();

                    if !judges_with_urls.is_empty() {
                        let (judge_id, private_url) = &judges_with_urls
                            [private_url_idx % judges_with_urls.len()];

                        if let Some(rid) = get_id_by_idx!(
                            &mut *conn,
                            rounds::table
                                .filter(rounds::tournament_id.eq(&tid))
                                .select(rounds::id)
                                .order_by(rounds::id),
                            round_idx,
                        ) {
                            let bytes = serialize_ballot_form(
                                &mut *conn, &tid, &rid, judge_id, f_form,
                            );
                            drop(conn);
                            client.post(&format!("/tournaments/{}/privateurls/{}/rounds/{}/submit", tid, private_url, rid)).bytes(Bytes::from(bytes)).await;
                        }
                    }
                }
            }
        }
    }
}

fn serialize_ballot_form(
    conn: &mut SqliteConnection,
    _tid: &str,
    rid: &str,
    _judge_id: &str,
    form: FuzzerBallotForm,
) -> Vec<u8> {
    use std::collections::HashMap;

    // We need to fetch the debate structure to know the teams and speakers
    let debate_info: Vec<(String, String, i64, i64)> = teams_of_debate::table
        .inner_join(debates::table)
        .filter(debates::round_id.eq(rid))
        .select((
            teams_of_debate::team_id,
            debates::id,
            teams_of_debate::side,
            teams_of_debate::seq,
        ))
        .load(conn)
        .unwrap();

    if debate_info.is_empty() {
        return Vec::new();
    }

    let m_ids: Vec<String> = motions_of_round::table
        .filter(motions_of_round::round_id.eq(rid))
        .select(motions_of_round::id)
        .order_by(motions_of_round::id)
        .load(conn)
        .unwrap();

    let mut query = HashMap::new();
    if !m_ids.is_empty() {
        query.insert(
            "motion_id".to_string(),
            m_ids[form.motion_idx % m_ids.len()].clone(),
        );
    }
    query.insert(
        "expected_version".to_string(),
        form.expected_version.to_string(),
    );

    for (i, team_entry) in form.teams.into_iter().enumerate() {
        if i >= debate_info.len() {
            break;
        }

        let team_id = &debate_info[i].0;

        if let Some(p) = team_entry.points {
            query.insert(format!("teams[{}][points]", i), p.to_string());
        }

        let team_speakers: Vec<String> = speakers_of_team::table
            .filter(speakers_of_team::team_id.eq(team_id))
            .select(speakers_of_team::speaker_id)
            .order_by(speakers_of_team::speaker_id)
            .load(conn)
            .unwrap();

        for (j, (s_idx, score)) in
            team_entry.speaker_indices.into_iter().enumerate()
        {
            if !team_speakers.is_empty() {
                let s_id = &team_speakers[s_idx % team_speakers.len()];
                query.insert(
                    format!("teams[{}][speakers][{}][id]", i, j),
                    s_id.clone(),
                );
                if let Some(sc) = score {
                    // Convert back to f32 for the form, assuming sc is e.g. score * 10
                    let sc_f = sc as f32 / 10.0;
                    query.insert(
                        format!("teams[{}][speakers][{}][score]", i, j),
                        sc_f.to_string(),
                    );
                }
            }
        }
    }

    serde_qs::to_string(&query).unwrap().into_bytes()
}
