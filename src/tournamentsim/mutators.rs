//! Application-independent mutator adapters used by the tournament simulator.

use fuzzcheck::Mutator;
use std::any::Any;

/// Presents an inner mutator's values at a fraction of their normal complexity.
///
/// This is useful for fuzz targets whose interesting inputs are compositions of
/// many individually complex values. The adapter contains only another mutator
/// and a constant scale: it neither observes nor retains state from the system
/// under test.
pub struct ComplexityScaledMutator<M> {
    inner: M,
    scale: f64,
}

impl<M> ComplexityScaledMutator<M> {
    pub fn new(inner: M, scale: f64) -> Self {
        assert!(scale.is_finite() && scale >= 1.0);
        Self { inner, scale }
    }

    fn outer_complexity(&self, complexity: f64) -> f64 {
        complexity / self.scale
    }

    fn inner_complexity(&self, complexity: f64) -> f64 {
        complexity * self.scale
    }
}

impl<T, M> Mutator<T> for ComplexityScaledMutator<M>
where
    T: Clone + 'static,
    M: Mutator<T>,
{
    type Cache = M::Cache;
    type MutationStep = M::MutationStep;
    type ArbitraryStep = M::ArbitraryStep;
    type UnmutateToken = M::UnmutateToken;

    fn initialize(&self) {
        self.inner.initialize();
    }

    fn default_arbitrary_step(&self) -> Self::ArbitraryStep {
        self.inner.default_arbitrary_step()
    }

    fn is_valid(&self, value: &T) -> bool {
        self.inner.is_valid(value)
    }

    fn validate_value(&self, value: &T) -> Option<Self::Cache> {
        self.inner.validate_value(value)
    }

    fn default_mutation_step(
        &self,
        value: &T,
        cache: &Self::Cache,
    ) -> Self::MutationStep {
        self.inner.default_mutation_step(value, cache)
    }

    fn global_search_space_complexity(&self) -> f64 {
        self.outer_complexity(self.inner.global_search_space_complexity())
    }

    fn max_complexity(&self) -> f64 {
        self.outer_complexity(self.inner.max_complexity())
    }

    fn min_complexity(&self) -> f64 {
        self.outer_complexity(self.inner.min_complexity())
    }

    fn complexity(&self, value: &T, cache: &Self::Cache) -> f64 {
        self.outer_complexity(self.inner.complexity(value, cache))
    }

    fn ordered_arbitrary(
        &self,
        step: &mut Self::ArbitraryStep,
        max_cplx: f64,
    ) -> Option<(T, f64)> {
        self.inner
            .ordered_arbitrary(step, self.inner_complexity(max_cplx))
            .map(|(value, cplx)| (value, self.outer_complexity(cplx)))
    }

    fn random_arbitrary(&self, max_cplx: f64) -> (T, f64) {
        let (value, cplx) =
            self.inner.random_arbitrary(self.inner_complexity(max_cplx));
        (value, self.outer_complexity(cplx))
    }

    fn ordered_mutate(
        &self,
        value: &mut T,
        cache: &mut Self::Cache,
        step: &mut Self::MutationStep,
        subvalue_provider: &dyn fuzzcheck::SubValueProvider,
        max_cplx: f64,
    ) -> Option<(Self::UnmutateToken, f64)> {
        self.inner
            .ordered_mutate(
                value,
                cache,
                step,
                subvalue_provider,
                self.inner_complexity(max_cplx),
            )
            .map(|(token, cplx)| (token, self.outer_complexity(cplx)))
    }

    fn random_mutate(
        &self,
        value: &mut T,
        cache: &mut Self::Cache,
        max_cplx: f64,
    ) -> (Self::UnmutateToken, f64) {
        let (token, cplx) = self.inner.random_mutate(
            value,
            cache,
            self.inner_complexity(max_cplx),
        );
        (token, self.outer_complexity(cplx))
    }

    fn unmutate(
        &self,
        value: &mut T,
        cache: &mut Self::Cache,
        token: Self::UnmutateToken,
    ) {
        self.inner.unmutate(value, cache, token);
    }

    fn visit_subvalues<'a>(
        &self,
        value: &'a T,
        cache: &'a Self::Cache,
        visit: &mut dyn FnMut(&'a dyn Any, f64),
    ) {
        self.inner
            .visit_subvalues(value, cache, &mut |value, cplx| {
                visit(value, self.outer_complexity(cplx));
            });
    }
}
