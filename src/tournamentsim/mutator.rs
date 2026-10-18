//! Preserve workload setup when appending reused actions or create-then-use sequences.

use std::any::Any;

use fuzzcheck::{DefaultMutator, Mutator, SubValueProvider};

use super::{WorkloadInput, inputs::Action, stencil};

type Inner = <WorkloadInput as DefaultMutator>::Mutator;
type Cache = <Inner as Mutator<WorkloadInput>>::Cache;

#[derive(Clone)]
pub struct MutationStep {
    inner: <Inner as Mutator<WorkloadInput>>::MutationStep,
    next_strategy: usize,
    subset_round: usize,
    stencil_round: usize,
}

pub struct SequenceAwareMutator {
    inner: Inner,
    action: <Action as DefaultMutator>::Mutator,
    append_enabled: bool,
}

pub enum Undo {
    Inner(<Inner as Mutator<WorkloadInput>>::UnmutateToken),
    Append(Cache, usize),
}

#[derive(Clone, Copy)]
enum Strategy {
    Structural,
    AppendSubset,
    AppendStencil,
}

// Keep structural mutation common while revisiting each append strategy for
// every saved input. A new strategy can be added without changing the fuzzer.
const STRATEGIES: [Strategy; 8] = [
    Strategy::AppendStencil,
    Strategy::Structural,
    Strategy::AppendSubset,
    Strategy::Structural,
    Strategy::Structural,
    Strategy::Structural,
    Strategy::Structural,
    Strategy::Structural,
];

impl SequenceAwareMutator {
    pub fn new() -> Self {
        Self {
            inner: WorkloadInput::default_mutator(),
            action: Action::default_mutator(),
            append_enabled: std::env::var("ABACUS_DISABLE_APPEND").as_deref()
                != Ok("1"),
        }
    }

    fn try_append_actions(
        &self,
        value: &mut WorkloadInput,
        cache: &mut Cache,
        max_cplx: f64,
        actions: Vec<Action>,
    ) -> Option<(Undo, f64)> {
        if actions.is_empty() || value.actions.len() + actions.len() > 64 {
            return None;
        }
        let original_len = value.actions.len();
        value.actions.extend(actions);
        if let Some(new_cache) = self.inner.validate_value(value) {
            let cplx = self.inner.complexity(value, &new_cache);
            if cplx <= max_cplx {
                let old_cache = std::mem::replace(cache, new_cache);
                return Some((
                    Undo::Append(old_cache, value.actions.len() - original_len),
                    cplx,
                ));
            }
        }
        value.actions.truncate(original_len);
        None
    }

    fn try_append_action(
        &self,
        value: &mut WorkloadInput,
        cache: &mut Cache,
        max_cplx: f64,
        action: Action,
    ) -> Option<(Undo, f64)> {
        self.try_append_actions(value, cache, max_cplx, vec![action])
    }

    fn try_append_stencil(
        &self,
        value: &mut WorkloadInput,
        cache: &mut Cache,
        max_cplx: f64,
        round: usize,
    ) -> Option<(Undo, f64)> {
        let template = stencil::STENCILS[round % stencil::STENCILS.len()];
        let actions = template.sample(value, value.actions.len() + round)?;
        self.try_append_actions(value, cache, max_cplx, actions)
    }

    fn try_append_subset(
        &self,
        value: &mut WorkloadInput,
        cache: &mut Cache,
        max_cplx: f64,
        round: usize,
    ) -> Option<(Undo, f64)> {
        let available = max_cplx - self.inner.complexity(value, cache);
        if value.actions.len() >= 64
            || available <= self.action.min_complexity()
        {
            return None;
        }
        // Reuse an action without changing its setup prefix. Do not assume
        // registration and tournament creation occupy particular positions.
        let selected = value.actions.get(round % value.actions.len().max(1))?;
        let mut action = (*selected).clone();
        if rand::random::<bool>() {
            if let Some(mut action_cache) = self.action.validate_value(&action)
            {
                let original = std::mem::discriminant(&action);
                self.action.random_mutate(
                    &mut action,
                    &mut action_cache,
                    available,
                );
                if std::mem::discriminant(&action) != original {
                    action = (*selected).clone();
                }
            }
        }
        self.try_append_action(value, cache, max_cplx, action)
    }
}

impl Mutator<WorkloadInput> for SequenceAwareMutator {
    type Cache = Cache;
    type MutationStep = MutationStep;
    type ArbitraryStep = <Inner as Mutator<WorkloadInput>>::ArbitraryStep;
    type UnmutateToken = Undo;

    fn initialize(&self) {
        self.inner.initialize();
        self.action.initialize();
    }

    fn default_arbitrary_step(&self) -> Self::ArbitraryStep {
        self.inner.default_arbitrary_step()
    }

    fn is_valid(&self, value: &WorkloadInput) -> bool {
        self.inner.is_valid(value)
    }

    fn validate_value(&self, value: &WorkloadInput) -> Option<Self::Cache> {
        self.inner.validate_value(value)
    }

    fn default_mutation_step(
        &self,
        value: &WorkloadInput,
        cache: &Self::Cache,
    ) -> Self::MutationStep {
        MutationStep {
            inner: self.inner.default_mutation_step(value, cache),
            next_strategy: 0,
            subset_round: 0,
            stencil_round: 0,
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

    fn complexity(&self, value: &WorkloadInput, cache: &Self::Cache) -> f64 {
        self.inner.complexity(value, cache)
    }

    fn ordered_arbitrary(
        &self,
        step: &mut Self::ArbitraryStep,
        max_cplx: f64,
    ) -> Option<(WorkloadInput, f64)> {
        self.inner.ordered_arbitrary(step, max_cplx)
    }

    fn random_arbitrary(&self, max_cplx: f64) -> (WorkloadInput, f64) {
        self.inner.random_arbitrary(max_cplx)
    }

    fn ordered_mutate(
        &self,
        value: &mut WorkloadInput,
        cache: &mut Self::Cache,
        step: &mut Self::MutationStep,
        subvalue_provider: &dyn SubValueProvider,
        max_cplx: f64,
    ) -> Option<(Self::UnmutateToken, f64)> {
        for _ in 0..STRATEGIES.len() {
            let strategy = STRATEGIES[step.next_strategy % STRATEGIES.len()];
            step.next_strategy += 1;
            let result = match strategy {
                Strategy::Structural => self
                    .inner
                    .ordered_mutate(
                        value,
                        cache,
                        &mut step.inner,
                        subvalue_provider,
                        max_cplx,
                    )
                    .map(|(undo, cplx)| (Undo::Inner(undo), cplx)),
                Strategy::AppendSubset if self.append_enabled => {
                    let round = step.subset_round;
                    step.subset_round += 1;
                    self.try_append_subset(value, cache, max_cplx, round)
                }
                Strategy::AppendStencil if self.append_enabled => {
                    let round = step.stencil_round;
                    step.stencil_round += 1;
                    self.try_append_stencil(value, cache, max_cplx, round)
                }
                _ => None,
            };
            if result.is_some() {
                return result;
            }
        }
        None
    }

    fn random_mutate(
        &self,
        value: &mut WorkloadInput,
        cache: &mut Self::Cache,
        max_cplx: f64,
    ) -> (Self::UnmutateToken, f64) {
        if self.append_enabled {
            let result =
                match STRATEGIES[rand::random_range(0..STRATEGIES.len())] {
                    Strategy::AppendSubset => self.try_append_subset(
                        value,
                        cache,
                        max_cplx,
                        rand::random::<u16>() as usize,
                    ),
                    Strategy::AppendStencil => self.try_append_stencil(
                        value,
                        cache,
                        max_cplx,
                        rand::random_range(0..stencil::STENCILS.len()),
                    ),
                    Strategy::Structural => None,
                };
            if let Some(result) = result {
                return result;
            }
        }
        let (undo, cplx) = self.inner.random_mutate(value, cache, max_cplx);
        (Undo::Inner(undo), cplx)
    }

    fn unmutate(
        &self,
        value: &mut WorkloadInput,
        cache: &mut Self::Cache,
        undo: Self::UnmutateToken,
    ) {
        match undo {
            Undo::Inner(undo) => self.inner.unmutate(value, cache, undo),
            Undo::Append(old_cache, count) => {
                value.actions.truncate(value.actions.len() - count);
                *cache = old_cache;
            }
        }
    }

    fn visit_subvalues<'a>(
        &self,
        value: &'a WorkloadInput,
        cache: &'a Self::Cache,
        visit: &mut dyn FnMut(&'a dyn Any, f64),
    ) {
        self.inner.visit_subvalues(value, cache, visit);
    }
}
