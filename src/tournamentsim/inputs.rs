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

const TABDA_DICTIONARY_STRINGS: [&str; 48] = [
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
    "consensus",
    "individual",
    "wins",
    "ballots",
    "draw_strength_by_wins",
    "draw_strength_by_speaks",
    "total_speaker_score",
    "avg_total_speaker_score",
    "n_times_achieved_0",
    "n_times_achieved_1",
    "n_times_achieved_2",
    "n_times_achieved_3",
    "lowest_rank",
    "highest_rank",
    "random",
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
        #[serde(default)]
        require_prelim_substantive_speaks: bool,
        #[serde(default)]
        require_prelim_speaker_order: bool,
        #[serde(default)]
        require_elim_substantive_speaks: bool,
        #[serde(default)]
        require_elim_speaker_order: bool,
        #[serde(default)]
        reply_speakers: bool,
        #[serde(default)]
        reply_must_speak: bool,
    },
    ApplyTournamentPreset {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        preset_idx: usize,
    },
    ViewTournamentPage {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        page_idx: usize,
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
    MoveConstraint {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        ptype: String,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        pid_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        constraint_idx: usize,
        up: bool,
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
    EditFeedbackQuestion {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        question_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        question: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        question_type: String,
        seq: i64,
    },
    MoveFeedbackQuestion {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        question_idx: usize,
        up: bool,
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
    EditRound {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[serde(default = "default_round_seq")]
        seq: u32,
    },
    CreateMotion {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        motion: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        infoslide: Option<String>,
    },
    GenerateDraw {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[serde(default)]
        force: bool,
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

    // Draw editing
    SubmitDrawCommand {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        cmd: String,
    },
    MoveJudge {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        debate_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        role: String,
        assign: bool,
    },
    MoveTeam {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team1_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team2_idx: usize,
    },
    ChangeJudgeRole {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_allocation_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        role: String,
    },
    MoveRoom {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        room_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        debate_idx: usize,
        assign: bool,
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
    EditBallot {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        debate_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        form: FuzzerBallotForm,
    },

    // Public feedback
    SubmitFeedback {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        private_url_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        target_judge_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        answer: String,
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

fn form_encode(pairs: &[(String, String)]) -> Vec<u8> {
    pairs
        .iter()
        .enumerate()
        .fold(String::new(), |mut out, (idx, (key, value))| {
            if idx > 0 {
                out.push('&');
            }
            out.push_str(
                &utf8_percent_encode(key, NON_ALPHANUMERIC).to_string(),
            );
            out.push('=');
            out.push_str(
                &utf8_percent_encode(value, NON_ALPHANUMERIC).to_string(),
            );
            out
        })
        .into_bytes()
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

    fn debate_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            debates::table
                .filter(debates::tournament_id.eq(tournament_id))
                .select(debates::id)
                .order_by(debates::id),
            idx,
        )
    }

    fn debate_id_in_round(&self, round_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            debates::table
                .filter(debates::round_id.eq(round_id))
                .select(debates::id)
                .order_by(debates::id),
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

    fn private_url(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        let judge_count = judges::table
            .filter(judges::tournament_id.eq(tournament_id))
            .count()
            .get_result::<i64>(&mut *conn)
            .unwrap_or(0);
        let speaker_count = speakers::table
            .filter(speakers::tournament_id.eq(tournament_id))
            .count()
            .get_result::<i64>(&mut *conn)
            .unwrap_or(0);
        let total = judge_count + speaker_count;
        if total == 0 {
            return None;
        }
        let idx = (idx as i64) % total;
        if idx < judge_count {
            judges::table
                .filter(judges::tournament_id.eq(tournament_id))
                .select(judges::private_url)
                .order_by(judges::id)
                .offset(idx)
                .limit(1)
                .get_result::<String>(&mut *conn)
                .ok()
        } else {
            speakers::table
                .filter(speakers::tournament_id.eq(tournament_id))
                .select(speakers::private_url)
                .order_by(speakers::id)
                .offset(idx - judge_count)
                .limit(1)
                .get_result::<String>(&mut *conn)
                .ok()
        }
    }

    async fn post(&mut self, action: &str, path: String) {
        let response = self.client.post(&path).await;
        assert_response_no_5xx(action, &path, &response);
    }

    async fn get(&mut self, action: &str, path: String) {
        let response = self.client.get(&path).await;
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

    async fn post_urlencoded(
        &mut self,
        action: &str,
        path: String,
        fields: &[(String, String)],
    ) {
        let response = self
            .client
            .post(&path)
            .bytes(Bytes::from(form_encode(fields)))
            .content_type("application/x-www-form-urlencoded")
            .await;
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
                require_prelim_substantive_speaks,
                require_prelim_speaker_order,
                require_elim_substantive_speaks,
                require_elim_speaker_order,
                reply_speakers,
                reply_must_speak,
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
                        config.require_prelim_substantive_speaks =
                            require_prelim_substantive_speaks;
                        config.require_prelim_speaker_order =
                            require_prelim_speaker_order;
                        config.require_elim_substantive_speaks =
                            require_elim_substantive_speaks;
                        config.require_elim_speaker_order =
                            require_elim_speaker_order;
                        config.reply_speakers = reply_speakers;
                        config.reply_must_speak = reply_must_speak;
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
            Action::ApplyTournamentPreset {
                tournament_idx,
                preset_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let (Some(tid), Some(preset_id)) = (
                    get_id_by_idx!(
                        &mut *conn,
                        tournaments::table
                            .select(tournaments::id)
                            .order_by(tournaments::id),
                        tournament_idx,
                    ),
                    get_id_by_idx!(
                        &mut *conn,
                        tournament_presets::table
                            .select(tournament_presets::id)
                            .order_by(tournament_presets::id),
                        preset_idx,
                    ),
                ) {
                    drop(conn);
                    let form = [("preset_id", preset_id)];
                    ctx.post_form(
                        "ApplyTournamentPreset",
                        format!("/tournaments/{}/configuration/preset", tid),
                        &form,
                    )
                    .await;
                }
            }
            Action::ViewTournamentPage {
                tournament_idx,
                round_idx,
                page_idx,
            } => {
                let mut conn = pool.get().unwrap();
                if let Some(tid) = get_id_by_idx!(
                    &mut *conn,
                    tournaments::table
                        .select(tournaments::id)
                        .order_by(tournaments::id),
                    tournament_idx,
                ) {
                    let round_count = rounds::table
                        .filter(rounds::tournament_id.eq(&tid))
                        .count()
                        .get_result::<i64>(&mut *conn)
                        .unwrap_or(0);
                    let round_info = if round_count == 0 {
                        None
                    } else {
                        rounds::table
                            .filter(rounds::tournament_id.eq(&tid))
                            .select((rounds::id, rounds::seq))
                            .order_by(rounds::id)
                            .offset((round_idx as i64) % round_count)
                            .limit(1)
                            .get_result::<(String, i64)>(&mut *conn)
                            .ok()
                    };
                    drop(conn);

                    let round_id = round_info.as_ref().map(|(id, _)| id);
                    let round_seq = round_info.as_ref().map(|(_, seq)| seq);
                    let round_path = |suffix: &str| {
                        round_seq.as_ref().map(|seq| {
                            format!(
                                "/tournaments/{}/rounds/{}{}",
                                tid, seq, suffix
                            )
                        })
                    };
                    let path = match page_idx % 38 {
                        0 => Some(format!("/tournaments/{}", tid)),
                        1 => Some(format!("/tournaments/{}/manage", tid)),
                        2 => Some(format!("/tournaments/{}/participants", tid)),
                        3 => Some(format!(
                            "/tournaments/{}/participants/privateurls",
                            tid
                        )),
                        4 => Some(format!("/tournaments/{}/rooms", tid)),
                        5 => {
                            Some(format!("/tournaments/{}/configuration", tid))
                        }
                        6 => Some(format!(
                            "/tournaments/{}/configuration/custom",
                            tid
                        )),
                        7 => Some(format!(
                            "/tournaments/{}/feedback/manage",
                            tid
                        )),
                        8 => {
                            Some(format!("/tournaments/{}/feedback/table", tid))
                        }
                        9 => Some(format!("/tournaments/{}/rounds", tid)),
                        10 => {
                            Some(format!("/tournaments/{}/rounds/create", tid))
                        }
                        11 => Some(format!(
                            "/tournaments/{}/standings/teams",
                            tid
                        )),
                        12 => Some(format!("/tournaments/{}/tab/team", tid)),
                        13 => Some(format!("/tournaments/{}/motions", tid)),
                        14 => round_path(""),
                        15 => round_path("/setup"),
                        16 => round_path("/draw/manage"),
                        17 => round_path("/briefing"),
                        18 => round_path("/results/manage"),
                        19 => round_path("/availability/judges"),
                        20 => round_path("/availability/teams"),
                        21 => round_path("/ballots"),
                        22 => round_path("/draw"),
                        23 => round_path("/results"),
                        24 => round_id.map(|rid| {
                            format!("/tournaments/{}/rounds/{}/edit", tid, rid)
                        }),
                        25 => round_id.map(|rid| {
                            format!(
                                "/tournaments/{}/rounds/{}/draws/create",
                                tid, rid
                            )
                        }),
                        26 => round_id.map(|rid| {
                            format!(
                                "/tournaments/{}/rounds/draws/edit?rounds={}",
                                tid, rid
                            )
                        }),
                        27 => round_id.map(|rid| {
                            format!(
                                "/tournaments/{}/rounds/draws/rooms/edit?rounds={}",
                                tid, rid
                            )
                        }),
                        28 => ctx.private_url(&tid, round_idx).map(|private_url| {
                            format!(
                                "/tournaments/{}/privateurls/{}",
                                tid, private_url
                            )
                        }),
                        29 => round_id.and_then(|rid| {
                            ctx.private_url(&tid, round_idx).map(|private_url| {
                                format!(
                                    "/tournaments/{}/privateurls/{}/rounds/{}/submit",
                                    tid, private_url, rid
                                )
                            })
                        }),
                        30 => round_id.and_then(|rid| {
                            ctx.private_url(&tid, round_idx).map(|private_url| {
                                format!(
                                    "/tournaments/{}/privateurls/{}/rounds/{}/feedback/submit",
                                    tid, private_url, rid
                                )
                            })
                        }),
                        31 => ctx.team_id(&tid, round_idx).map(|team_id| {
                            format!("/tournaments/{}/teams/{}", tid, team_id)
                        }),
                        32 => ctx.debate_id(&tid, round_idx).map(|debate_id| {
                            format!(
                                "/tournaments/{}/debates/{}/ballots",
                                tid, debate_id
                            )
                        }),
                        33 => {
                            let mut conn = pool.get().unwrap();
                            let targets = judges_of_debate::table
                                .inner_join(
                                    debates::table.on(
                                        debates::id
                                            .eq(judges_of_debate::debate_id),
                                    )
                                )
                                .inner_join(
                                    rounds::table.on(
                                        rounds::id.eq(debates::round_id),
                                    )
                                )
                                .filter(debates::tournament_id.eq(&tid))
                                .filter(rounds::draw_status.eq("released_full"))
                                .select((
                                    judges_of_debate::debate_id,
                                    judges_of_debate::judge_id,
                                ))
                                .order_by(judges_of_debate::id)
                                .load::<(String, String)>(&mut *conn)
                                .unwrap_or_default();
                            let target = if targets.is_empty() {
                                None
                            } else {
                                Some(
                                    targets
                                        [round_idx % targets.len()]
                                    .clone(),
                                )
                            };
                            target.map(|(debate_id, judge_id)| {
                                format!(
                                    "/tournaments/{}/debates/{}/judges/{}/edit",
                                    tid, debate_id, judge_id
                                )
                            })
                        }
                        34 => {
                            let mut conn = pool.get().unwrap();
                            let targets = ballots::table
                                .filter(ballots::tournament_id.eq(&tid))
                                .select((ballots::debate_id, ballots::id))
                                .order_by(ballots::id)
                                .load::<(String, String)>(&mut *conn)
                                .unwrap_or_default();
                            let target = if targets.is_empty() {
                                None
                            } else {
                                Some(
                                    targets
                                        [round_idx % targets.len()]
                                    .clone(),
                                )
                            };
                            target.map(|(debate_id, ballot_id)| {
                                format!(
                                    "/tournaments/{}/debates/{}/ballots/{}/view",
                                    tid, debate_id, ballot_id
                                )
                            })
                        }
                        35 => ctx.judge_id(&tid, round_idx).map(|judge_id| {
                            format!(
                                "/tournaments/{}/participants/judge/{}/constraints",
                                tid, judge_id
                            )
                        }),
                        36 => ctx.team_id(&tid, round_idx).map(|team_id| {
                            format!(
                                "/tournaments/{}/teams/{}/speakers/create",
                                tid, team_id
                            )
                        }),
                        37 => Some(format!("/tournaments/{}/judges/create", tid)),
                        _ => unreachable!(),
                    };
                    if let Some(path) = path {
                        ctx.get("ViewTournamentPage", path).await;
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
            Action::MoveConstraint {
                tournament_idx,
                ptype,
                pid_idx,
                constraint_idx,
                up,
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
                        let cat_id = if ptype == "speaker" {
                            get_id_by_idx!(
                                &mut *conn,
                                speaker_room_constraints::table
                                    .filter(
                                        speaker_room_constraints::speaker_id
                                            .eq(&pid),
                                    )
                                    .select(
                                        speaker_room_constraints::category_id,
                                    )
                                    .order_by(speaker_room_constraints::pref,),
                                constraint_idx,
                            )
                        } else {
                            get_id_by_idx!(
                                &mut *conn,
                                judge_room_constraints::table
                                    .filter(
                                        judge_room_constraints::judge_id
                                            .eq(&pid),
                                    )
                                    .select(
                                        judge_room_constraints::category_id,
                                    )
                                    .order_by(judge_room_constraints::pref),
                                constraint_idx,
                            )
                        };

                        if let Some(cat_id) = cat_id {
                            drop(conn);
                            let ptype = if ptype == "speaker" {
                                "speaker".to_string()
                            } else {
                                "judge".to_string()
                            };
                            let form = [
                                ("category_id".to_string(), cat_id),
                                (
                                    "direction".to_string(),
                                    if up { "up" } else { "down" }.to_string(),
                                ),
                            ];
                            ctx.post_urlencoded(
                                "MoveConstraint",
                                format!(
                                    "/tournaments/{}/participants/{}/{}/constraints/move",
                                    tid, ptype, pid
                                ),
                                &form,
                            )
                            .await;
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
            Action::EditFeedbackQuestion {
                tournament_idx,
                question_idx,
                question,
                question_type,
                seq,
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
                        let form = [
                            ("question_id", q_id.clone()),
                            ("question", question),
                            ("kind", question_type),
                            ("seq", seq.to_string()),
                        ];
                        ctx.post_form(
                            "EditFeedbackQuestion",
                            format!(
                                "/tournaments/{}/feedback/manage/{}/edit",
                                tid, q_id
                            ),
                            &form,
                        )
                        .await;
                    }
                }
            }
            Action::MoveFeedbackQuestion {
                tournament_idx,
                question_idx,
                up,
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
                        let direction = if up { "up" } else { "down" };
                        ctx.post_form(
                            "MoveFeedbackQuestion",
                            format!(
                                "/tournaments/{}/feedback/manage/{}",
                                tid, direction
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
            Action::EditRound {
                tournament_idx,
                round_idx,
                name,
                seq,
            } => {
                let seq = (seq % 200).max(1);
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
                        let form = [("name", name), ("seq", seq.to_string())];
                        ctx.post_form(
                            "EditRound",
                            format!("/tournaments/{}/rounds/{}/edit", tid, rid),
                            &form,
                        )
                        .await;
                    }
                }
            }
            Action::CreateMotion {
                tournament_idx,
                round_idx,
                motion,
                infoslide,
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
                        let mut form = vec![("motion".to_string(), motion)];
                        if let Some(infoslide) = infoslide {
                            form.push(("infoslide".to_string(), infoslide));
                        }
                        ctx.post_urlencoded(
                            "CreateMotion",
                            format!(
                                "/tournaments/{}/rounds/{}/motions/create",
                                tid, rid
                            ),
                            &form,
                        )
                        .await;
                    }
                }
            }
            Action::GenerateDraw {
                tournament_idx,
                round_idx,
                force,
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
                        let force = if force { "?force=true" } else { "" };
                        ctx.post(
                            "GenerateDraw",
                            format!(
                                "/tournaments/{}/rounds/{}/draws/create{}",
                                tid, rid, force
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
            Action::SubmitDrawCommand {
                tournament_idx,
                round_idx,
                cmd,
            } => {
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let Some(rid) = ctx.round_id(&tid, round_idx) {
                        let form = [("cmd".to_string(), cmd)];
                        ctx.post_urlencoded(
                            "SubmitDrawCommand",
                            format!(
                                "/tournaments/{}/rounds/draws/edit?rounds={}",
                                tid, rid
                            ),
                            &form,
                        )
                        .await;
                    }
                }
            }
            Action::MoveJudge {
                tournament_idx,
                round_idx,
                judge_idx,
                debate_idx,
                role,
                assign,
            } => {
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let (Some(rid), Some(judge_id)) = (
                        ctx.round_id(&tid, round_idx),
                        ctx.judge_id(&tid, judge_idx),
                    ) {
                        let to_debate_id = if assign {
                            ctx.debate_id_in_round(&rid, debate_idx)
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        let role = match role.as_str() {
                            "C" | "chair" => "C",
                            "T" | "trainee" => "T",
                            _ => "P",
                        };
                        let form = [
                            ("judge_id".to_string(), judge_id),
                            ("to_debate_id".to_string(), to_debate_id),
                            ("role".to_string(), role.to_string()),
                            ("rounds".to_string(), rid),
                        ];
                        ctx.post_urlencoded(
                            "MoveJudge",
                            format!(
                                "/tournaments/{}/rounds/draws/edit/move",
                                tid
                            ),
                            &form,
                        )
                        .await;
                    }
                }
            }
            Action::MoveTeam {
                tournament_idx,
                round_idx,
                team1_idx,
                team2_idx,
            } => {
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let Some(rid) = ctx.round_id(&tid, round_idx) {
                        let mut conn = pool.get().unwrap();
                        let team1_id = get_id_by_idx!(
                            &mut *conn,
                            teams_of_debate::table
                                .inner_join(debates::table.on(
                                    debates::id.eq(teams_of_debate::debate_id),
                                ),)
                                .filter(debates::round_id.eq(&rid))
                                .select(teams_of_debate::team_id)
                                .order_by(teams_of_debate::id),
                            team1_idx,
                        );
                        let team2_id = get_id_by_idx!(
                            &mut *conn,
                            teams_of_debate::table
                                .inner_join(debates::table.on(
                                    debates::id.eq(teams_of_debate::debate_id),
                                ),)
                                .filter(debates::round_id.eq(&rid))
                                .select(teams_of_debate::team_id)
                                .order_by(teams_of_debate::id),
                            team2_idx,
                        );
                        drop(conn);
                        if let (Some(team1_id), Some(team2_id)) =
                            (team1_id, team2_id)
                        {
                            let form = [
                                ("team1_id".to_string(), team1_id),
                                ("team2_id".to_string(), team2_id),
                                ("rounds".to_string(), rid),
                            ];
                            ctx.post_urlencoded(
                                "MoveTeam",
                                format!(
                                    "/tournaments/{}/rounds/draws/edit/move_team",
                                    tid
                                ),
                                &form,
                            )
                            .await;
                        }
                    }
                }
            }
            Action::ChangeJudgeRole {
                tournament_idx,
                round_idx,
                judge_allocation_idx,
                role,
            } => {
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let Some(rid) = ctx.round_id(&tid, round_idx) {
                        let mut conn = pool.get().unwrap();
                        let allocation = get_id_by_idx!(
                            &mut *conn,
                            judges_of_debate::table
                                .inner_join(debates::table.on(
                                    debates::id.eq(judges_of_debate::debate_id),
                                ),)
                                .filter(debates::round_id.eq(&rid))
                                .select(judges_of_debate::id)
                                .order_by(judges_of_debate::id),
                            judge_allocation_idx,
                        );
                        if let Some(allocation_id) = allocation {
                            let (judge_id, debate_id): (String, String) =
                                judges_of_debate::table
                                    .filter(
                                        judges_of_debate::id.eq(allocation_id),
                                    )
                                    .select((
                                        judges_of_debate::judge_id,
                                        judges_of_debate::debate_id,
                                    ))
                                    .first(&mut *conn)
                                    .unwrap();
                            drop(conn);
                            let role = match role.as_str() {
                                "C" | "chair" => "C",
                                "T" | "trainee" => "T",
                                _ => "P",
                            };
                            let form = [
                                ("judge_id".to_string(), judge_id),
                                ("debate_id".to_string(), debate_id),
                                ("role".to_string(), role.to_string()),
                                ("rounds".to_string(), rid),
                            ];
                            ctx.post_urlencoded(
                                "ChangeJudgeRole",
                                format!(
                                    "/tournaments/{}/rounds/draws/edit/role",
                                    tid
                                ),
                                &form,
                            )
                            .await;
                        }
                    }
                }
            }
            Action::MoveRoom {
                tournament_idx,
                round_idx,
                room_idx,
                debate_idx,
                assign,
            } => {
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let (Some(rid), Some(room_id)) = (
                        ctx.round_id(&tid, round_idx),
                        ctx.room_id(&tid, room_idx),
                    ) {
                        let to_debate_id = if assign {
                            ctx.debate_id_in_round(&rid, debate_idx)
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        let form = [
                            ("room_id".to_string(), room_id),
                            ("to_debate_id".to_string(), to_debate_id),
                            ("rounds".to_string(), rid),
                        ];
                        ctx.post_urlencoded(
                            "MoveRoom",
                            format!(
                                "/tournaments/{}/rounds/draws/rooms/edit/move",
                                tid
                            ),
                            &form,
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
            Action::EditBallot {
                tournament_idx,
                debate_idx,
                judge_idx,
                form: f_form,
            } => {
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let Some(debate_id) = ctx.debate_id(&tid, debate_idx) {
                        let mut conn = pool.get().unwrap();
                        let judge_id = get_id_by_idx!(
                            &mut *conn,
                            judges_of_debate::table
                                .filter(
                                    judges_of_debate::debate_id.eq(&debate_id),
                                )
                                .select(judges_of_debate::judge_id)
                                .order_by(judges_of_debate::judge_id),
                            judge_idx,
                        );

                        if let Some(judge_id) = judge_id {
                            let bytes = serialize_ballot_form_for_debate(
                                &mut *conn, &debate_id, f_form,
                            );
                            drop(conn);
                            if !bytes.is_empty() {
                                ctx.post_bytes(
                                    "EditBallot",
                                    format!(
                                        "/tournaments/{}/debates/{}/judges/{}/edit",
                                        tid, debate_id, judge_id
                                    ),
                                    bytes,
                                )
                                .await;
                            }
                        }
                    }
                }
            }
            Action::SubmitFeedback {
                tournament_idx,
                private_url_idx,
                round_idx,
                target_judge_idx,
                answer,
            } => {
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let (Some(private_url), Some(rid)) = (
                        ctx.private_url(&tid, private_url_idx),
                        ctx.round_id(&tid, round_idx),
                    ) {
                        let mut conn = pool.get().unwrap();
                        let target_judge_id = get_id_by_idx!(
                            &mut *conn,
                            judges_of_debate::table
                                .inner_join(debates::table.on(
                                    debates::id.eq(judges_of_debate::debate_id),
                                ),)
                                .filter(debates::round_id.eq(&rid))
                                .select(judges_of_debate::judge_id)
                                .order_by(judges_of_debate::judge_id),
                            target_judge_idx,
                        );

                        if let Some(target_judge_id) = target_judge_id {
                            let question_id = feedback_questions::table
                                .filter(
                                    feedback_questions::tournament_id.eq(&tid),
                                )
                                .select(feedback_questions::id)
                                .order_by(feedback_questions::seq)
                                .first::<String>(&mut *conn)
                                .ok();
                            drop(conn);

                            let mut form = vec![(
                                "target_judge_id".to_string(),
                                target_judge_id,
                            )];
                            if let Some(question_id) = question_id {
                                form.push((question_id, answer));
                            }

                            ctx.post_urlencoded(
                                "SubmitFeedback",
                                format!(
                                    "/tournaments/{}/privateurls/{}/rounds/{}/feedback/submit",
                                    tid, private_url, rid
                                ),
                                &form,
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
    let debate_id = debates::table
        .filter(debates::round_id.eq(rid))
        .select(debates::id)
        .order_by(debates::id)
        .first::<String>(conn)
        .ok();

    if let Some(debate_id) = debate_id {
        serialize_ballot_form_for_debate(conn, &debate_id, form)
    } else {
        Vec::new()
    }
}

fn serialize_ballot_form_for_debate(
    conn: &mut SqliteConnection,
    debate_id: &str,
    form: FuzzerBallotForm,
) -> Vec<u8> {
    use std::collections::HashMap;

    // We need to fetch the debate structure to know the teams and speakers
    let debate_info: Vec<(String, String, i64, i64)> = teams_of_debate::table
        .filter(teams_of_debate::debate_id.eq(debate_id))
        .select((
            teams_of_debate::team_id,
            teams_of_debate::debate_id,
            teams_of_debate::side,
            teams_of_debate::seq,
        ))
        .order_by((teams_of_debate::side, teams_of_debate::seq))
        .load(conn)
        .unwrap();

    if debate_info.is_empty() {
        return Vec::new();
    }

    let rid = debates::table
        .filter(debates::id.eq(debate_id))
        .select(debates::round_id)
        .first::<String>(conn)
        .unwrap();

    let m_ids: Vec<String> = motions_of_round::table
        .filter(motions_of_round::round_id.eq(&rid))
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
