use crate::{
    calculation::{apply_immediate, get_effect_amount, get_effect_source_stats, recalculate_stats},
    events::EffectMetadata,
    prelude::*,
    timing::SmallTimer,
    StackingBehaviors,
};
use bevy::prelude::*;
use bevy_hierarchical_tags::prelude::*;
use smallvec::SmallVec;

const ACTIVE_EFFECTS_SIZE: usize = 24;
const ACTIVE_TAGS_SIZE: usize = 32;

#[derive(Clone)]
pub struct GameplayEffect<T: StatTrait> {
    pub stat_target: T,
    pub magnitude: EffectMagnitude<T>,
    pub calculation: EffectCalculation,
    pub duration: EffectDuration,
    pub tag: Option<TagId>,
}

impl<T: StatTrait> GameplayEffect<T> {
    pub fn set_duration(&mut self, duration: impl Into<SmallTimer>) -> Result<(), &'static str> {
        match &mut self.duration {
            EffectDuration::Continuous(Some(timer)) => {
                timer.set_duration(duration);
            }
            EffectDuration::Persistent(Some(timer)) => {
                timer.set_duration(duration);
            }
            EffectDuration::Repeating(_, Some(timer)) => {
                timer.set_duration(duration);
            }
            _ => return Err("Effect has no duration timer set"),
        }
        Ok(())
    }
}

impl<T: StatTrait> GameplayEffect<T> {
    pub fn new(
        tag: Option<TagId>,
        stat_target: T,
        magnitude: EffectMagnitude<T>,
        calculation: EffectCalculation,
        duration: EffectDuration,
    ) -> Self {
        Self {
            stat_target,
            magnitude,
            calculation,
            duration,
            tag,
        }
    }

    pub fn tag_effect(tag: TagId, duration: Option<f32>) -> Self {
        let duration: Option<SmallTimer> = duration.map(|d| d.into());
        Self {
            stat_target: T::NONE,
            magnitude: EffectMagnitude::None,
            calculation: EffectCalculation::None,
            duration: EffectDuration::Persistent(duration),
            tag: Some(tag),
        }
    }
}

impl<T: StatTrait> GameplayEffect<T> {
    fn get_duration_timer(&self) -> Option<&SmallTimer> {
        match &self.duration {
            EffectDuration::Continuous(Some(timer)) => Some(timer),
            EffectDuration::Persistent(Some(timer)) => Some(timer),
            EffectDuration::Repeating(_, Some(timer)) => Some(timer),
            _ => None,
        }
    }
}

#[derive(Component, Deref, DerefMut, Default)]
pub struct ActiveTags(TagList<ACTIVE_TAGS_SIZE>);

impl ActiveTags {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, tag: TagId) {
        if !self.iter().any(|&t| t == tag) {
            self.push(tag);
        }
    }

    pub fn add_from(&mut self, tags: &[TagId]) {
        for tag in tags.iter() {
            self.add(*tag);
        }
    }

    pub fn remove(&mut self, tag: TagId) {
        self.retain(|t| *t != tag);
    }

    pub fn remove_from(&mut self, tags: &[TagId]) {
        for tag in tags.iter() {
            self.remove(*tag);
        }
    }
}

#[derive(Component, Clone, Deref, DerefMut)]
#[require(ActiveTags)]
pub struct ActiveEffects<T: StatTrait>(
    pub(crate) SmallVec<[GameplayEffect<T>; ACTIVE_EFFECTS_SIZE]>,
);

impl<T: StatTrait> ActiveEffects<T> {
    pub fn new(effects: impl IntoIterator<Item = GameplayEffect<T>>) -> Self {
        let mut instance = Self(SmallVec::<[GameplayEffect<T>; ACTIVE_EFFECTS_SIZE]>::new());
        instance.0.extend(effects);
        instance
    }

    pub fn match_effect_type(
        &mut self,
        other: TagId,
    ) -> impl Iterator<Item = &mut GameplayEffect<T>> {
        self.0.iter_mut().filter(move |e| e.tag == Some(other))
    }
}

pub(crate) fn add_effect<T: StatTrait>(
    trigger: On<AddEffect<T>>,
    mut stats_query: Query<&mut GameplayStats<T>>,
    mut active_effects: Query<(Entity, &mut ActiveEffects<T>, &mut ActiveTags)>,
    mut added_writer: MessageWriter<OnEffectAdded>,
    mut breached_writer: MessageWriter<OnBoundsBreached<T>>,
    stacking_behaviors: Res<StackingBehaviors>,
) {
    let event = trigger.event();
    let AddEffectData::<T> {
        effect,
        target_entity,
        source_entity,
    } = &event.0;

    if let Ok((entity, mut effects, mut tags)) = active_effects.get_mut(*target_entity) {
        let source = get_effect_source_stats(effect, entity, &mut stats_query);
        let amount = get_effect_amount(effect, source);

        if !matches!(effect.duration, EffectDuration::Immediate) {
            if let Some(tag) = effect.tag {
                tags.add(tag);
                let stacking = stacking_behaviors.0[*tag as usize].unwrap_or_default();

                match stacking {
                    StackingPolicy::NoStacking => {
                        if effects.match_effect_type(tag).count() == 0 {
                            effects.0.push(effect.clone());
                        } else {
                            return;
                        }
                    }
                    StackingPolicy::NoStackingResetDuration => {
                        if effects.match_effect_type(tag).count() == 0 {
                            effects.0.push(effect.clone());
                        } else {
                            if let Some(timer) = effect.get_duration_timer() {
                                for other in effects.match_effect_type(tag) {
                                    other.set_duration(timer.clone()).ok();
                                }
                            }
                            return;
                        }
                    }
                    StackingPolicy::MultipleEffects(max) => {
                        if effects.match_effect_type(tag).count() < max as usize {
                            effects.0.push(effect.clone());
                        } else {
                            return;
                        }
                    }
                    StackingPolicy::MultipleEffectsResetDurations(max) => {
                        if let Some(timer) = effect.get_duration_timer() {
                            for other in effects.match_effect_type(tag) {
                                other.set_duration(timer.clone()).ok();
                            }
                        }
                        if effects.match_effect_type(tag).count() < max as usize {
                            effects.0.push(effect.clone());
                        } else {
                            return;
                        }
                    }
                }
            } else {
                effects.0.push(effect.clone());
            }
        }
        // Check for bounds breach
        match &effect.duration {
            EffectDuration::Immediate => {
                if let Some(e) = apply_immediate(entity, effect, &mut stats_query, amount, &effects)
                {
                    breached_writer.write(e);
                }
            }
            EffectDuration::Persistent(_) => {
                if let Some(e) =
                    recalculate_stats(entity, &effects, effect.stat_target, &mut stats_query)
                {
                    breached_writer.write(e);
                }
            }
            _ => {}
        }
        added_writer.write(OnEffectAdded(EffectMetadata::new(
            event.0.target_entity,
            effect.tag,
            *source_entity,
        )));
    }
}

pub(crate) fn remove_effect<T: StatTrait>(
    trigger: On<RemoveEffect>,
    mut breached_writer: MessageWriter<OnBoundsBreached<T>>,
    mut removed_writer: MessageWriter<OnEffectRemoved>,
    mut effects_entities_query: Query<(&mut ActiveEffects<T>, &mut ActiveTags)>,
    mut stats_query: Query<&mut GameplayStats<T>>,
) {
    let EffectMetadata {
        tag,
        target_entity,
        source_entity,
    } = trigger.event().0;
    let Ok((mut effects, mut tags)) = effects_entities_query.get_mut(target_entity) else {
        return;
    };
    if let Some(tag) = tag {
        tags.remove(tag);
    }
    let mut to_remove = SmallVec::<[usize; 8]>::new();

    for (index, current_effect) in effects.0.iter().enumerate() {
        if tag == current_effect.tag {
            to_remove.push(index);
        }
    }

    for &i in to_remove.iter().rev() {
        let effect = effects.0.remove(i);
        if let Some(e) = recalculate_stats(
            target_entity,
            &effects,
            effect.stat_target,
            &mut stats_query,
        ) {
            breached_writer.write(e);
        }
        removed_writer.write(OnEffectRemoved(EffectMetadata::new(
            target_entity,
            effect.tag,
            source_entity,
        )));
    }
}

pub(crate) fn process_active_effects<T: StatTrait>(
    time: Res<Time>,
    mut stats_query: Query<&mut GameplayStats<T>>,
    mut entity_effects_query: Query<(Entity, &mut ActiveEffects<T>, &mut ActiveTags)>,
    mut periodic_event_writer: MessageWriter<OnRepeatingEffectTriggered>,
    mut breached_writer: MessageWriter<OnBoundsBreached<T>>,
    mut removed_writer: MessageWriter<OnEffectRemoved>,
) {
    entity_effects_query
        .iter_mut()
        .for_each(|(entity, mut effects, mut tags)| {
            // Tick all the timers
            for effect in effects.0.iter_mut() {
                match &mut effect.duration {
                    EffectDuration::Continuous(Some(timer)) => {
                        timer.tick(time.delta_secs());
                    }
                    EffectDuration::Persistent(Some(timer)) => {
                        timer.tick(time.delta_secs());
                    }
                    EffectDuration::Repeating(period, timer) => {
                        period.tick(time.delta_secs());
                        if let Some(timer) = timer {
                            timer.tick(time.delta_secs());
                        }
                    }
                    _ => {}
                }
            }

            let mut removed = SmallVec::<[usize; 8]>::new();

            // Now apply effects for this frame
            for (idx, effect) in effects.0.iter().enumerate() {
                // Get effect magnitude
                let source = get_effect_source_stats(effect, entity, &mut stats_query);
                if matches!(effect.magnitude, EffectMagnitude::NonlocalStat(..)) && source.is_none()
                {
                    // Source entity gone
                    removed.push(idx);
                }
                let mut amount = get_effect_amount(effect, source);
                if matches!(effect.duration, EffectDuration::Continuous(_)) {
                    amount *= time.delta_secs();
                    // TODO check effect saturation so framerate spikes don't cause a huge effect
                }

                // Check for expiration timers
                if let Some(timer) = effect.get_duration_timer() {
                    if timer.finished() {
                        removed.push(idx);
                    }
                }

                // Persistent and immediate effects are already applied
                let apply = match effect.duration {
                    EffectDuration::Repeating(period, _) => {
                        if period.just_triggered() {
                            periodic_event_writer.write(OnRepeatingEffectTriggered(
                                EffectMetadata::new(entity, effect.tag, None),
                            ));
                            true
                        } else {
                            false
                        }
                    }
                    EffectDuration::Continuous(_) => true,
                    _ => false,
                };
                if apply {
                    if let Some(event) =
                        apply_immediate(entity, effect, &mut stats_query, amount, &effects)
                    {
                        breached_writer.write(event);
                    }
                }
            }

            for &i in removed.iter().rev() {
                let effect = effects.0.remove(i);
                if matches!(effect.duration, EffectDuration::Persistent(_)) {
                    if let Some(e) =
                        recalculate_stats(entity, &effects, effect.stat_target, &mut stats_query)
                    {
                        breached_writer.write(e);
                    }
                }
                if let Some(tag) = effect.tag {
                    tags.remove(tag);
                }
                removed_writer.write(OnEffectRemoved(EffectMetadata::new(
                    entity, effect.tag, None,
                )));
            }
        });
}
