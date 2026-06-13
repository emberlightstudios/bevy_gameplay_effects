use crate::prelude::*;
use bevy::prelude::*;

#[derive(Default, Copy, Clone)]
pub enum StackingPolicy {
    #[default]
    NoStacking,
    NoStackingResetDuration,
    MultipleEffects(u8),
    MultipleEffectsResetDurations(u8),
}

#[derive(Clone, PartialEq)]
pub enum EffectMagnitude<T: StatTrait> {
    None,
    Fixed(f32),
    LocalStat(T, StatScalingParams),
    NonlocalStat(T, StatScalingParams, Entity),
}

#[derive(Clone, PartialEq)]
pub enum EffectCalculation {
    None,
    Additive,
    Multiplicative,
    SetValue,
    LowerBound,
    UpperBound,
}

#[derive(Clone, PartialEq)]
pub struct StatScalingParams {
    pub shift: f32,
    pub stat_offset: f32,
    pub multiplier: f32,
    pub exponent: f32,
    pub min: Option<f32>,
    pub max: Option<f32>,
}

impl Default for StatScalingParams {
    fn default() -> Self {
        Self {
            shift: 0.0,
            stat_offset: 0.0,
            multiplier: 1.0,
            exponent: 1.0,
            min: None,
            max: None,
        }
    }
}

impl StatScalingParams {
    pub(crate) fn apply(&self, stat: f32) -> f32 {
        let mut out = stat - self.stat_offset;
        if self.exponent != 1.0 {
            out = out.powf(self.exponent);
        }
        out = self.shift + self.multiplier * out;
        if let Some(min) = self.min {
            out = f32::max(min, out);
        }
        if let Some(max) = self.max {
            out = f32::min(max, out);
        }
        out
    }
}

/// Apply changes to a stat's current value
#[inline]
pub(crate) fn apply_immediate<T: StatTrait>(
    entity: Entity,
    effect: &GameplayEffect<T>,
    stats_query: &mut Query<&mut GameplayStats<T>>,
    amount: f32,
    effects: &ActiveEffects<T>,
) -> Option<OnBoundsBreached<T>> {
    if effect.stat_target.into() == u8::MAX {
        return None;
    }
    let (upper_bound, lower_bound) = get_bounds(entity, effect.stat_target, effects, stats_query);
    let mut stats = stats_query
        .get_mut(entity)
        .expect("Missing GameplayStats component");
    let stat = stats.get_mut(effect.stat_target);

    match &effect.calculation {
        EffectCalculation::Additive => stat.current_value += amount,
        EffectCalculation::Multiplicative => stat.current_value *= amount,
        EffectCalculation::SetValue => stat.current_value = amount,
        _ => {}
    }
    if stat.current_value >= upper_bound {
        stat.current_value = upper_bound;
        Some(OnBoundsBreached(BoundsBreachedMetadata::new(
            entity,
            effect.stat_target,
            EffectCalculation::UpperBound,
        )))
    } else if stat.current_value <= lower_bound {
        stat.current_value = lower_bound;
        Some(OnBoundsBreached(BoundsBreachedMetadata::new(
            entity,
            effect.stat_target,
            EffectCalculation::LowerBound,
        )))
    } else {
        None
    }
}

/// After persistent effects are added/removed recalulate base and current stat values
#[inline]
pub(crate) fn recalculate_stats<T: StatTrait>(
    entity: Entity,
    effects: &ActiveEffects<T>,
    stat_target: T,
    stats_query: &mut Query<&mut GameplayStats<T>>,
) -> Option<OnBoundsBreached<T>> {
    if stat_target.into() == u8::MAX {
        return None;
    }
    let mut additive: f32 = 0.;
    let mut multiplicative: f32 = 1.;

    for effect in effects.0.iter() {
        let source = get_effect_source_stats(effect, entity, stats_query);
        let amount = get_effect_amount(effect, source);

        if effect.stat_target == stat_target {
            match effect.calculation {
                EffectCalculation::Additive => additive += amount,
                EffectCalculation::Multiplicative => multiplicative *= amount,
                _ => {}
            }
        }
    }

    let (upper_bound, lower_bound) = get_bounds(entity, stat_target, effects, stats_query);
    let mut stats = stats_query
        .get_mut(entity)
        .expect("No stats component found");
    let stat = stats.get_mut(stat_target);
    let prev_base = stat.modified_base;
    let mut new_base = (stat.base_value + additive) * multiplicative;
    new_base = f32::min(upper_bound, new_base);
    new_base = f32::max(lower_bound, new_base);
    stat.modified_base = new_base;
    stat.current_value *= new_base / prev_base;

    if stat.current_value >= upper_bound {
        stat.current_value = upper_bound;
        Some(OnBoundsBreached(BoundsBreachedMetadata {
            stat: stat_target,
            bound: EffectCalculation::UpperBound,
            target_entity: entity,
        }))
    } else if stat.current_value <= lower_bound {
        stat.current_value = lower_bound;
        Some(OnBoundsBreached(BoundsBreachedMetadata {
            stat: stat_target,
            bound: EffectCalculation::LowerBound,
            target_entity: entity,
        }))
    } else {
        None
    }
}

/// Get the magnitude of the effect on the stat
#[inline]
pub(crate) fn get_effect_amount<T: StatTrait>(
    effect: &GameplayEffect<T>,
    source: Option<&GameplayStats<T>>,
) -> f32 {
    match &effect.magnitude {
        EffectMagnitude::None => 0.,
        EffectMagnitude::Fixed(x) => *x,
        EffectMagnitude::LocalStat(stat, f) => {
            let stats = source.unwrap();
            f.apply(stats.get(*stat).current_value)
        }
        EffectMagnitude::NonlocalStat(stat, f, _) => {
            let stats = source.unwrap();
            f.apply(stats.get(*stat).current_value)
        }
    }
}

#[inline]
pub(crate) fn get_bounds<T: StatTrait>(
    entity: Entity,
    stat_target: T,
    effects: &ActiveEffects<T>,
    stats_query: &mut Query<&mut GameplayStats<T>>,
) -> (f32, f32) {
    let mut ub = f32::MAX;
    let mut lb = f32::MIN;

    for effect in effects.iter().filter(|x| x.stat_target == stat_target) {
        let source = get_effect_source_stats(effect, entity, stats_query);
        let amount = get_effect_amount(effect, source);
        match effect.calculation {
            EffectCalculation::LowerBound => {
                lb = f32::max(lb, amount);
            }
            EffectCalculation::UpperBound => {
                ub = f32::min(ub, amount);
            }
            _ => {}
        }
    }
    (ub, lb)
}

#[inline]
pub(crate) fn get_effect_source_stats<'a, T: StatTrait>(
    effect: &GameplayEffect<T>,
    entity: Entity,
    stats_query: &'a Query<&mut GameplayStats<T>>,
) -> Option<&'a GameplayStats<T>> {
    match &effect.magnitude {
        EffectMagnitude::NonlocalStat(_, _, source_entity) => stats_query.get(*source_entity).ok(),
        EffectMagnitude::LocalStat(..) => stats_query.get(entity).ok(),
        _ => None,
    }
}
