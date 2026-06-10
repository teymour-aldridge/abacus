use crate::schema::*;
use axum::body::Bytes;
use axum_test::{TestResponse, TestServer};
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use fuzzcheck::DefaultMutator;
use fuzzcheck::Mutator;
use fuzzcheck::mutators::integer_within_range::U64WithinRangeMutator;
use fuzzcheck::mutators::map::MapMutator;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

const TABDA_DICTIONARY_STRINGS: [&str; 33] = [
    "otter",
    "badger",
    "lynx",
    "falcon",
    "gecko",
    "narwhal",
    "wombat",
    "panther",
    "meerkat",
    "ibex",
    "quetzal",
    "capybara",
    "speaker",
    "speakers",
    "judge",
    "judges",
    "score",
    "text",
    "bool",
    "none",
    "draft",
    "confirmed",
    "released_teams",
    "released_full",
    "up",
    "down",
    "in",
    "out",
    "on",
    "true",
    "false",
    "in_round",
    "-----",
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

#[allow(dead_code)]
pub struct NameStringMutator {
    inner: InnerStringMutator,
}

#[allow(dead_code)]
pub struct MotionStringMutator {
    inner: InnerStringMutator,
}

pub struct TabdaDictionaryStringMutator {
    inner: InnerStringMutator,
}

#[derive(Clone)]
pub struct GrammarStringCache {
    ast: Option<InnerStringAst>,
    ast_cache: Option<InnerStringCache>,
}

#[derive(Clone)]
pub struct GrammarStringArbitraryStep {
    inner: InnerStringArbitraryStep,
}

#[derive(Clone)]
pub struct GrammarStringMutationStep {
    inner: Option<InnerStringMutationStep>,
    arbitrary: InnerStringArbitraryStep,
}

pub enum GrammarStringUnmutateToken {
    ReplaceWhole {
        old_value: String,
        old_cache: GrammarStringCache,
    },
    Inner(InnerStringUnmutateToken),
}

impl NameStringMutator {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            inner: fuzzcheck::mutators::grammar::grammar_based_ast_mutator(
                name_string_grammar(),
            ),
        }
    }
}

impl MotionStringMutator {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            inner: fuzzcheck::mutators::grammar::grammar_based_ast_mutator(
                motion_string_grammar(),
            ),
        }
    }
}

impl TabdaDictionaryStringMutator {
    pub fn new() -> Self {
        Self {
            inner: fuzzcheck::mutators::grammar::grammar_based_ast_mutator(
                tabda_dictionary_string_grammar(),
            ),
        }
    }
}

fn ast_to_string(ast: &InnerStringAst) -> String {
    ast.to_string()
}

fn ordered_grammar_value(
    inner: &InnerStringMutator,
    step: &mut InnerStringArbitraryStep,
    max_cplx: f64,
) -> Option<(String, GrammarStringCache, f64)> {
    let (ast, cplx) = inner.ordered_arbitrary(step, max_cplx)?;
    let ast_cache = inner
        .validate_value(&ast)
        .expect("grammar mutator should validate its own AST output");
    Some((
        ast_to_string(&ast),
        GrammarStringCache {
            ast: Some(ast),
            ast_cache: Some(ast_cache),
        },
        cplx,
    ))
}

fn random_grammar_value(
    inner: &InnerStringMutator,
    max_cplx: f64,
) -> (String, GrammarStringCache, f64) {
    let (ast, cplx) = inner.random_arbitrary(max_cplx);
    let ast_cache = inner
        .validate_value(&ast)
        .expect("grammar mutator should validate its own AST output");
    (
        ast_to_string(&ast),
        GrammarStringCache {
            ast: Some(ast),
            ast_cache: Some(ast_cache),
        },
        cplx,
    )
}

fn fallback_grammar_cache() -> GrammarStringCache {
    GrammarStringCache {
        ast: None,
        ast_cache: None,
    }
}

fn literal_string(value: &str) -> Rc<fuzzcheck::mutators::grammar::Grammar> {
    fuzzcheck::mutators::grammar::concatenation(
        value.chars().map(fuzzcheck::mutators::grammar::literal),
    )
}

fn dictionary_grammar() -> Rc<fuzzcheck::mutators::grammar::Grammar> {
    fuzzcheck::mutators::grammar::alternation(
        TABDA_DICTIONARY_STRINGS
            .iter()
            .map(|value| literal_string(value)),
    )
}

fn email_grammar() -> Rc<fuzzcheck::mutators::grammar::Grammar> {
    fuzzcheck::mutators::grammar::regex(
        "[a-z]{1,12}@[a-z]{1,10}\\.(com|org|net|edu)",
    )
}

#[allow(dead_code)]
fn name_string_grammar() -> Rc<fuzzcheck::mutators::grammar::Grammar> {
    fuzzcheck::mutators::grammar::regex(
        "[A-Za-z0-9][A-Za-z0-9 ]{2,22}[A-Za-z0-9]",
    )
}

#[allow(dead_code)]
fn motion_string_grammar() -> Rc<fuzzcheck::mutators::grammar::Grammar> {
    fuzzcheck::mutators::grammar::regex(
        "[A-Za-z0-9][A-Za-z0-9 ,.'-]{6,46}[A-Za-z0-9.]",
    )
}

fn tabda_dictionary_string_grammar() -> Rc<fuzzcheck::mutators::grammar::Grammar>
{
    use fuzzcheck::mutators::grammar::{
        alternation, concatenation, recurse, recursive, regex,
    };

    let dictionary = dictionary_grammar();
    let email = email_grammar();
    let ascii = regex("[ -~]");

    recursive(|whole: &Weak<fuzzcheck::mutators::grammar::Grammar>| {
        alternation([
            dictionary.clone(),
            dictionary.clone(),
            dictionary.clone(),
            email.clone(),
            ascii.clone(),
            concatenation([recurse(whole), recurse(whole)]),
        ])
    })
}

macro_rules! impl_ascii_string_mutator {
    ($mutator:ty) => {
        impl Mutator<String> for $mutator {
            type Cache = GrammarStringCache;
            type MutationStep = GrammarStringMutationStep;
            type ArbitraryStep = GrammarStringArbitraryStep;
            type UnmutateToken = GrammarStringUnmutateToken;

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
                    Some(fallback_grammar_cache())
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
                    inner: cache
                        .ast
                        .as_ref()
                        .zip(cache.ast_cache.as_ref())
                        .map(|(ast, ast_cache)| {
                            self.inner.default_mutation_step(ast, ast_cache)
                        }),
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
                ordered_grammar_value(&self.inner, &mut step.inner, max_cplx)
                    .map(|(value, _, cplx)| (value, cplx))
            }

            fn random_arbitrary(&self, max_cplx: f64) -> (String, f64) {
                let (value, _, cplx) =
                    random_grammar_value(&self.inner, max_cplx);
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
                        *value = ast_to_string(ast);
                        return Some((
                            GrammarStringUnmutateToken::Inner(token),
                            cplx,
                        ));
                    }
                }

                let old_value = value.clone();
                let old_cache = cache.clone();
                let (new_value, new_cache, cplx) = ordered_grammar_value(
                    &self.inner,
                    &mut step.arbitrary,
                    max_cplx,
                )?;
                *value = new_value;
                *cache = new_cache;
                step.inner =
                    cache.ast.as_ref().zip(cache.ast_cache.as_ref()).map(
                        |(ast, ast_cache)| {
                            self.inner.default_mutation_step(ast, ast_cache)
                        },
                    );
                Some((
                    GrammarStringUnmutateToken::ReplaceWhole {
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
                    *value = ast_to_string(ast);
                    (GrammarStringUnmutateToken::Inner(token), cplx)
                } else {
                    let old_value = value.clone();
                    let old_cache = cache.clone();
                    let (new_value, new_cache, cplx) =
                        random_grammar_value(&self.inner, max_cplx);
                    *value = new_value;
                    *cache = new_cache;
                    (
                        GrammarStringUnmutateToken::ReplaceWhole {
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
                    GrammarStringUnmutateToken::ReplaceWhole {
                        old_value,
                        old_cache,
                    } => {
                        *value = old_value;
                        *cache = old_cache;
                    }
                    GrammarStringUnmutateToken::Inner(token) => {
                        if let (Some(ast), Some(ast_cache)) =
                            (cache.ast.as_mut(), cache.ast_cache.as_mut())
                        {
                            self.inner.unmutate(ast, ast_cache, token);
                            *value = ast_to_string(ast);
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
    };
}

impl_ascii_string_mutator!(NameStringMutator);
impl_ascii_string_mutator!(MotionStringMutator);
impl_ascii_string_mutator!(TabdaDictionaryStringMutator);

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

pub type UsizeMutator = impl Mutator<usize>;

#[define_opaque(UsizeMutator)]
pub fn make_usize_mutator() -> UsizeMutator {
    MapMutator::<u64, usize, _, _, _, _>::new(
        U64WithinRangeMutator::new(0..=250),
        |output: &usize| Some(*output as u64),
        |x: &u64| *x as usize,
        |_: &usize, cplx| cplx,
    )
}

#[derive(DefaultMutator, Clone, Debug, Hash, Serialize, Deserialize)]
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
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
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
    UpdateTournamentConfiguration {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
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
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        institution_idx: Option<usize>,
    },
    EditTeam {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        institution_idx: Option<usize>,
    },
    CreateSpeaker {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
    },
    CreateJudge {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
        institution_idx: Option<usize>,
    },
    EditJudge {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
        institution_idx: Option<usize>,
    },

    // Constraints
    AddConstraint {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        ptype: String,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        pid_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        category_idx: usize,
    },
    RemoveConstraint {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        ptype: String,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        pid_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        constraint_idx: usize,
    },

    // Rooms
    CreateRoom {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        priority: i64,
    },
    DeleteRoom {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        room_idx: usize,
    },
    CreateRoomCategory {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
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
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        category_idx: usize,
    },
    AddRoomToCategory {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        category_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        room_idx: usize,
    },
    RemoveRoomFromCategory {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        category_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        room_idx: usize,
    },

    // Feedback
    AddFeedbackQuestion {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
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
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        question_idx: usize,
    },

    // Rounds & Draws
    CreateRound {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        category_idx: Option<usize>,
        #[serde(default = "default_round_seq")]
        seq: u32,
    },
    GenerateDraw {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
    },
    SetDrawPublished {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[serde(default)]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        published: Option<bool>,
    },
    SetRoundCompleted {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        completed: bool,
    },
    PublishMotions {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
    },
    PublishResults {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
    },

    // Availability & Eligibility
    UpdateJudgeAvailability {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        available: bool,
    },
    UpdateAllJudgeAvailability {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        available: bool,
    },
    UpdateTeamEligibility {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team_idx: usize,
        eligible: bool,
    },
    UpdateAllTeamEligibility {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        eligible: bool,
    },

    // Ballots
    SubmitBallot {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        private_url_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        form: FuzzerBallotForm,
    },
}

fn default_round_seq() -> u32 {
    1
}

#[derive(DefaultMutator, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct FuzzerBallotForm {
    pub motion_idx: usize,
    pub teams: Vec<FuzzerBallotTeamEntry>,
    pub expected_version: i64,
}

#[derive(DefaultMutator, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct FuzzerBallotTeamEntry {
    pub speaker_indices: Vec<(usize, Option<i32>)>,
    pub points: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FuzzState {
    user_passwords: HashMap<String, String>,
}

impl FuzzState {
    fn remember_user_password(&mut self, user_id: String, password: String) {
        self.user_passwords.entry(user_id).or_insert(password);
    }

    fn password_for_user(&self, user_id: &str) -> Option<&str> {
        self.user_passwords.get(user_id).map(String::as_str)
    }
}

fn path_segment(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

struct ActionContext<'a> {
    pool: &'a Pool<ConnectionManager<SqliteConnection>>,
    client: &'a mut TestServer,
    state: &'a mut FuzzState,
}

impl<'a> ActionContext<'a> {
    fn new(
        pool: &'a Pool<ConnectionManager<SqliteConnection>>,
        client: &'a mut TestServer,
        state: &'a mut FuzzState,
    ) -> Self {
        Self {
            pool,
            client,
            state,
        }
    }

    fn conn(
        &self,
    ) -> diesel::r2d2::PooledConnection<ConnectionManager<SqliteConnection>>
    {
        self.pool.get().unwrap()
    }

    fn tournament_id(&self, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            tournaments::table
                .select(tournaments::id)
                .order_by(tournaments::id),
            idx,
        )
    }

    fn round_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            rounds::table
                .filter(rounds::tournament_id.eq(tournament_id))
                .select(rounds::id)
                .order_by(rounds::id),
            idx,
        )
    }

    fn team_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            teams::table
                .filter(teams::tournament_id.eq(tournament_id))
                .select(teams::id)
                .order_by(teams::id),
            idx,
        )
    }

    fn judge_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            judges::table
                .filter(judges::tournament_id.eq(tournament_id))
                .select(judges::id)
                .order_by(judges::id),
            idx,
        )
    }

    fn room_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            rooms::table
                .filter(rooms::tournament_id.eq(tournament_id))
                .select(rooms::id)
                .order_by(rooms::id),
            idx,
        )
    }

    fn room_category_id(
        &self,
        tournament_id: &str,
        idx: usize,
    ) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            room_categories::table
                .filter(room_categories::tournament_id.eq(tournament_id))
                .select(room_categories::id)
                .order_by(room_categories::id),
            idx,
        )
    }

    fn feedback_question_id(
        &self,
        tournament_id: &str,
        idx: usize,
    ) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            feedback_questions::table
                .filter(feedback_questions::tournament_id.eq(tournament_id))
                .select(feedback_questions::id)
                .order_by(feedback_questions::id),
            idx,
        )
    }

    async fn post(&mut self, action: &str, path: String) {
        let response = self.client.post(&path).await;
        assert_response_no_5xx(action, &path, &response);
    }

    async fn post_form<F>(&mut self, action: &str, path: String, form: &F)
    where
        F: ?Sized + Serialize,
    {
        let response = self.client.post(&path).form(form).await;
        assert_response_no_5xx(action, &path, &response);
    }

    async fn post_bytes(&mut self, action: &str, path: String, bytes: Vec<u8>) {
        let response = self.client.post(&path).bytes(Bytes::from(bytes)).await;
        assert_response_no_5xx(action, &path, &response);
    }
}

fn assert_response_no_5xx(action: &str, path: &str, response: &TestResponse) {
    assert!(
        !response.status_code().is_server_error(),
        "{action} POST {path} returned {}\n{}",
        response.status_code(),
        response.text(),
    );
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
        let mut ctx = ActionContext::new(pool, client, state);

        match self {
            Action::RegisterUser {
                username,
                email,
                password,
            } => {
                let password2 = password.clone();
                let lookup_username = username.clone();
                let lookup_email = email.clone();
                let form = [
                    ("username", username),
                    ("email", email),
                    ("password", password),
                    ("password2", password2.clone()),
                ];
                ctx.post_form("RegisterUser", "/register".to_string(), &form)
                    .await;

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
                    ctx.state.remember_user_password(user_id, password2);
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

                    if let Some(password) =
                        ctx.state.password_for_user(&user_id)
                    {
                        let form = [
                            ("id", login_id),
                            ("password", password.to_string()),
                        ];
                        ctx.post_form("LoginUser", "/login".to_string(), &form)
                            .await;
                    }
                }
            }
            Action::LogoutUser => {
                ctx.post("LogoutUser", "/logout".to_string()).await;
            }
            Action::CreateTournament { name, abbrv, slug } => {
                let form = [("name", name), ("abbrv", abbrv), ("slug", slug)];
                ctx.post_form(
                    "CreateTournament",
                    "/tournaments/create".to_string(),
                    &form,
                )
                .await;
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
                        ctx.post_form(
                            "UpdateTournamentConfiguration",
                            format!("/tournaments/{}/configuration", tid),
                            &form,
                        )
                        .await;
                    }
                }
            }
            Action::CreateTeam {
                tournament_idx,
                name,
                institution_idx,
            } => {
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
                    ctx.post_form(
                        "CreateTeam",
                        format!("/tournaments/{}/teams/create", tid),
                        &form,
                    )
                    .await;
                }
            }
            Action::EditTeam {
                tournament_idx,
                team_idx,
                name,
                institution_idx,
            } => {
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
                        ctx.post_form(
                            "EditTeam",
                            format!(
                                "/tournaments/{}/teams/{}/edit",
                                tid, team_id
                            ),
                            &form,
                        )
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
                    ctx.post_form(
                        "CreateJudge",
                        format!("/tournaments/{}/judges/create", tid),
                        &form,
                    )
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
                        ctx.post_form(
                            "EditJudge",
                            format!(
                                "/tournaments/{}/judges/{}/edit",
                                tid, judge_id
                            ),
                            &form,
                        )
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
                        ctx.post_form(
                            "CreateSpeaker",
                            format!(
                                "/tournaments/{}/teams/{}/speakers/create",
                                tid, team_id
                            ),
                            &form,
                        )
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
                        let ptype = path_segment(&ptype);
                        ctx.post_form(
                            "AddConstraint",
                            format!(
                                "/tournaments/{}/participants/{}/{}/constraints/add",
                                tid, ptype, pid
                            ),
                            &form,
                        )
                        .await;
                    }
                }
            }
            Action::RemoveConstraint {
                tournament_idx,
                ptype,
                pid_idx,
                constraint_idx,
            } => {
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
                                ctx.post_form(
                                    "RemoveConstraint",
                                    format!(
                                        "/tournaments/{}/participants/speaker/{}/constraints/remove",
                                        tid, pid
                                    ),
                                    &form,
                                )
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
                                ctx.post_form(
                                    "RemoveConstraint",
                                    format!(
                                        "/tournaments/{}/participants/judge/{}/constraints/remove",
                                        tid, pid
                                    ),
                                    &form,
                                )
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
                    ctx.post_form(
                        "CreateRoom",
                        format!("/tournaments/{}/rooms/create", tid),
                        &form,
                    )
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
                        ctx.post(
                            "DeleteRoom",
                            format!(
                                "/tournaments/{}/rooms/{}/delete",
                                tid, room_id
                            ),
                        )
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
                let private_name = if private_name.is_empty() {
                    legacy_name.clone()
                } else {
                    private_name
                };
                let public_name = if public_name.is_empty() {
                    legacy_name
                } else {
                    public_name
                };
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
                    ctx.post_form(
                        "CreateRoomCategory",
                        format!("/tournaments/{}/rooms/categories/create", tid),
                        &form,
                    )
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
                        ctx.post(
                            "DeleteRoomCategory",
                            format!(
                                "/tournaments/{}/rooms/categories/{}/delete",
                                tid, cat_id
                            ),
                        )
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
                        ctx.post_form(
                            "AddRoomToCategory",
                            format!(
                                "/tournaments/{}/rooms/categories/{}/add_room",
                                tid, cat_id
                            ),
                            &form,
                        )
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
                        ctx.post_form(
                            "RemoveRoomFromCategory",
                            format!(
                                "/tournaments/{}/rooms/categories/{}/remove_room",
                                tid, cat_id
                            ),
                            &form,
                        )
                        .await;
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
                    ctx.post_form(
                        "AddFeedbackQuestion",
                        format!("/tournaments/{}/feedback/manage/add", tid),
                        &form,
                    )
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
                        ctx.post_form(
                            "DeleteFeedbackQuestion",
                            format!(
                                "/tournaments/{}/feedback/manage/delete",
                                tid
                            ),
                            &form,
                        )
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
                    ctx.post_form(
                        "CreateRound",
                        format!(
                            "/tournaments/{}/rounds/{}/create",
                            tid, category_id
                        ),
                        &form,
                    )
                    .await;
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
                        ctx.post(
                            "GenerateDraw",
                            format!(
                                "/tournaments/{}/rounds/{}/draws/create",
                                tid, rid
                            ),
                        )
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
                let _ = published;
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
                        ctx.post_form(
                            "SetDrawPublished",
                            format!(
                                "/tournaments/{}/rounds/{}/draws/setreleased",
                                tid, rid
                            ),
                            &form,
                        )
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
                        ctx.post_form(
                            "SetRoundCompleted",
                            format!(
                                "/tournaments/{}/rounds/{}/complete",
                                tid, rid
                            ),
                            &form,
                        )
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
                        ctx.post(
                            "PublishMotions",
                            format!(
                                "/tournaments/{}/rounds/{}/motions/publish",
                                tid, rid
                            ),
                        )
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
                        ctx.post_form(
                            "PublishResults",
                            format!(
                                "/tournaments/{}/rounds/{}/results/publish",
                                tid, rid
                            ),
                            &form,
                        )
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
                        ctx.post_form(
                            "UpdateJudgeAvailability",
                            format!(
                                "/tournaments/{}/rounds/{}/update_judge_availability",
                                tid, rid
                            ),
                            &form,
                        )
                        .await;
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
                        ctx.post(
                            "UpdateAllJudgeAvailability",
                            format!(
                                "/tournaments/{}/rounds/{}/availability/judges/all?check={}",
                                tid, rid, check
                            ),
                        )
                        .await;
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
                        ctx.post_form(
                            "UpdateTeamEligibility",
                            format!(
                                "/tournaments/{}/rounds/{}/update_team_eligibility",
                                tid, rid
                            ),
                            &form,
                        )
                        .await;
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
                        ctx.post(
                            "UpdateAllTeamEligibility",
                            format!(
                                "/tournaments/{}/rounds/{}/availability/teams/all?check={}",
                                tid, rid, check
                            ),
                        )
                        .await;
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
                            ctx.post_bytes(
                                "SubmitBallot",
                                format!(
                                    "/tournaments/{}/privateurls/{}/rounds/{}/submit",
                                    tid, private_url, rid
                                ),
                                bytes,
                            )
                            .await;
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
