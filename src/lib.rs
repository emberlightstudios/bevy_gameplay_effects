use std::marker::PhantomData;
use bevy::prelude::*;
use bevy_hierarchical_tags::TagId;
use crate::{effects::{add_effect, process_active_effects, remove_effect}, prelude::*};

mod gameplay_stats;
mod effects;
mod timing;
mod calculation;
mod events;
mod enum_macro;

pub mod prelude {
    pub use crate::{
        stats,
        GameplayEffectsPlugin,
        GameplayEffectsSystemSet,
        StackingBehaviors,
        gameplay_stats::{GameplayStat, GameplayStats, StatTrait},
        effects::{GameplayEffect, ActiveEffects, ActiveTags},
        timing::EffectDuration,
        calculation::{EffectCalculation, StackingPolicy, EffectMagnitude, StatScalingParams},
        events::{AddEffectData, EffectMetadata, AddEffect, RemoveEffect, OnEffectAdded,
            OnEffectRemoved, OnBoundsBreached, OnRepeatingEffectTriggered, BoundsBreachedMetadata},
    };
}

pub struct GameplayEffectsPlugin<T: StatTrait>(StackingBehaviors, PhantomData<T>);

impl<T: StatTrait> Default for GameplayEffectsPlugin<T> {
    fn default() -> Self {
        Self::new(StackingBehaviors::new())
    }
}

impl<T: StatTrait> GameplayEffectsPlugin<T> {
    pub fn new(stacking: StackingBehaviors) -> Self {
        Self(stacking, PhantomData)
    }
}

#[derive(Resource, Clone)]
pub struct StackingBehaviors([Option<StackingPolicy>; 1024]);

impl StackingBehaviors {
    pub fn new() -> Self {
        Self([None; 1024])
    }

    pub fn stack(mut self, tag: TagId, policy: StackingPolicy) -> Self {
        self.0[*tag as usize] = Some(policy);
        self
    }
}


#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GameplayEffectsSystemSet;

impl<T: StatTrait> Plugin for GameplayEffectsPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_message::<OnEffectAdded>();
        app.add_message::<OnEffectRemoved>();
        app.add_message::<OnRepeatingEffectTriggered>();
        app.add_message::<OnBoundsBreached<T>>();
        app.add_observer(add_effect::<T>);
        app.add_observer(remove_effect::<T>);
        app.add_systems(Update, process_active_effects::<T>.in_set(GameplayEffectsSystemSet));
        app.insert_resource(self.0.clone());
    }
}


#[cfg(test)]
mod tests {
    use std::time::Duration;
    use super::*;
    pub(crate) use bevy::{prelude::*, time::TimePlugin};
    pub(crate) use crate::prelude::*;

    stats!(
        MyStats {
            Health,
            HealthRegen,
            HealthMax,
            Strength,
        }
    );

    pub(crate) fn setup_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.build().disable::<TimePlugin>());
        app.world_mut().insert_resource::<Time>(Time::default());
        app.add_plugins(GameplayEffectsPlugin::<MyStats>::default());
        app
    }

    fn setup_entity<'a>(app: &mut App) -> (Entity, QueryState<(Entity, &'a GameplayStats<MyStats>, &'a ActiveEffects<MyStats>)>) {
        let stats_component = GameplayStats::<MyStats>::new(
            |stat| {
                match stat {
                    MyStats::Health => { 100.0 },
                    MyStats::HealthRegen => { 5.0 },
                    MyStats::HealthMax => { 100.0 },
                    MyStats::Strength => { 10.0 },
                    MyStats::None => { 0. }
                }
            },
        );
        let active_effects = ActiveEffects::<MyStats>::new(std::iter::empty());
        let active_tags = ActiveTags::default();
        let entity = app.world_mut().spawn((
            stats_component,
            active_effects,
            active_tags,
        )).id();
        let query = app.world_mut()
            .query::<(Entity, &GameplayStats<MyStats>, &ActiveEffects<MyStats>)>();
        app.update();
        (entity, query)
    }
    
    #[test] 
    fn test_lower_bound() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::Fixed(0.),
                EffectCalculation::LowerBound,
                EffectDuration::Persistent(None),
            ),
            None,
        )));
        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::Fixed(-200.),
                EffectCalculation::Additive,
                EffectDuration::Immediate,
            ),
            None,
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 0.);

        let events = app.world_mut().resource_mut::<Messages<OnBoundsBreached<MyStats>>>();
        let mut cursor = events.get_cursor();
        let mut events = cursor.read(&events);
        assert_eq!(events.len(), 1);
        
        let event = events.next().unwrap();
        assert!(matches!(event.bound, EffectCalculation::LowerBound));
        assert_eq!(event.target_entity, entity);
        assert_eq!(event.stat, MyStats::Health);
    }

    #[test] 
    fn test_upper_bound() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::Fixed(150.),
                EffectCalculation::UpperBound,
                EffectDuration::Persistent(None),
            ),
            None
        )));
        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::Fixed(200.),
                EffectCalculation::Additive,
                EffectDuration::Immediate,
            ),
            None
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 150.);

        let events = app.world_mut().resource_mut::<Messages<OnBoundsBreached<MyStats>>>();
        let mut cursor = events.get_cursor();
        let mut events = cursor.read(&events);
        assert_eq!(events.len(), 1);
        
        let event = events.next().unwrap();
        assert!(matches!(event.bound, EffectCalculation::UpperBound));
        assert_eq!(event.target_entity, entity);
        assert_eq!(event.stat, MyStats::Health);
    }

    #[test] 
    fn test_set_value() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::LocalStat(MyStats::HealthMax, StatScalingParams::default()),
                EffectCalculation::UpperBound,
                EffectDuration::Persistent(None),
            ),
            None
        )));

        // Try to set past max health
        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::Fixed(200.),
                EffectCalculation::SetValue,
                EffectDuration::Immediate,
            ),
            None
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 100.);

        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::Fixed(50.),
                EffectCalculation::SetValue,
                EffectDuration::Immediate,
            ),
            None
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 50.);

        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::LocalStat(MyStats::HealthMax, StatScalingParams::default()),
                EffectCalculation::SetValue,
                EffectDuration::Immediate,
            ),
            None,
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 100.);
    }

    #[test] 
    fn test_periodic_effect() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        let scaling = StatScalingParams {
            multiplier: 2.0,
            ..default()
        };
        let regen_tag = TagId::from(1);
        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                Some(regen_tag),
                MyStats::Health,
                EffectMagnitude::LocalStat(MyStats::HealthRegen, scaling),
                EffectCalculation::Additive,
                EffectDuration::Repeating(1.0.into(), Some(10.0.into())),
            ),
            None
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 100.);
        
        for i in 1..=10 {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();
            let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(health, 100. + (10. * i as f32));
            let events = app.world_mut().resource_mut::<Messages<OnRepeatingEffectTriggered>>();
            let mut cursor = events.get_cursor();
            let events = cursor.read(&events);
            assert!(events.len() >= 1);
        }

        let events = app.world_mut().resource_mut::<Messages<OnRepeatingEffectTriggered>>();
        let mut cursor = events.get_cursor();
        let event = cursor.read(&events).next().unwrap();
        assert_eq!(event.target_entity, entity);
        assert_eq!(event.tag, Some(regen_tag));
            
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 200.);

        let (_, _, active) = query.iter(app.world_mut()).next().unwrap();
        assert_eq!(active.0.len(), 0);
    }

    #[test] 
    fn test_continuous_with_nonlocal_magnitude() {
        let mut app = setup_app();
        let (entity1, _) = setup_entity(&mut app);
        let (entity2, mut query) = setup_entity(&mut app);

        let scaling = StatScalingParams {
            multiplier: -2.0,
            ..default()
        };
        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity1, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::NonlocalStat(MyStats::Strength, scaling, entity2),
                EffectCalculation::Additive,
                EffectDuration::Continuous(Some(10.0.into())),
            ),
            Some(entity2)
        )));

        let events = app.world_mut().resource_mut::<Messages<OnEffectAdded>>();
        let mut cursor = events.get_cursor();
        let mut events = cursor.read(&events);
        assert_eq!(events.len(), 1);
        let event = events.next().unwrap();
        let EffectMetadata { target_entity, tag, source_entity } = event.0;
        assert_eq!(source_entity, Some(entity2));
        assert_eq!(target_entity, entity1);
        assert_eq!(tag, None);


        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 100.);

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        for (entity, stats, _) in query.iter(app.world_mut()) {
            if entity != entity1 { continue; }
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(health, 0.);
        }

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        
        for (entity, stats, _) in query.iter(app.world_mut()) {
            if entity != entity1 { continue; }
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(health, -100.);
        }
    }

    #[test] 
    fn test_persistent_with_duration() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        let buff = GameplayEffect::new(
            None,
            MyStats::Health,
            EffectMagnitude::Fixed(2.),
            EffectCalculation::Multiplicative,
            EffectDuration::Persistent(Some(5.0.into())),
        );
        app.world_mut().trigger(AddEffect(AddEffectData::new(entity, buff.clone(), None)));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 200.);

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 100.);
    }

    #[test] 
    fn test_persistent_removal() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        let tag1 = TagId::from(1);
        let buff1 = GameplayEffect::new(
            Some(tag1),
            MyStats::Health,
            EffectMagnitude::Fixed(2.),
            EffectCalculation::Multiplicative,
            EffectDuration::Persistent(None),
        );
        let tag2 = TagId::from(2);
        let buff2 = GameplayEffect::new(
            Some(tag2),
            MyStats::Health,
            EffectMagnitude::Fixed(2.),
            EffectCalculation::Multiplicative,
            EffectDuration::Persistent(None),
        );

        app.world_mut().trigger(AddEffect(AddEffectData::new(entity, buff1.clone(), None)));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 200.);
        app.world_mut().trigger(AddEffect(AddEffectData::new(entity, buff2.clone(), None)));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 400.);

        app.world_mut().trigger(AddEffect(AddEffectData::new(
            entity, 
            GameplayEffect::new(
                None,
                MyStats::Health,
                EffectMagnitude::Fixed(-100.),
                EffectCalculation::Additive,
                EffectDuration::Immediate,
            ),
            None
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 300.);
        
        app.world_mut().trigger(RemoveEffect(EffectMetadata::new(entity, buff1.tag, None)));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 150.);

        app.world_mut().trigger(RemoveEffect(EffectMetadata::new(entity, buff2.tag, None)));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 75.);
    }

    #[test] 
    fn test_no_stacking() {
        let mut app = setup_app();

        let tag = TagId::from(1);
        app.insert_resource(StackingBehaviors::new()
            .stack(tag, StackingPolicy::NoStacking)
        );

        let (entity, mut query) = setup_entity(&mut app);

        let effect = GameplayEffect::new(
            Some(tag),
            MyStats::Health,
            EffectMagnitude::Fixed(-1.0),
            EffectCalculation::Additive,
            EffectDuration::Continuous(Some(3.0.into())),
        );

        app.world_mut().trigger(AddEffect(AddEffectData::new(entity, effect.clone(), None)));
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
        app.update();
        
        let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 99.);
        assert_eq!(effects.0.iter().len(), 1);

        app.world_mut().trigger(AddEffect(AddEffectData::new(entity, effect, None)));
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
        app.update();

        let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 98.);
        assert_eq!(effects.0.iter().len(), 1);
    }

    #[test] 
    fn test_no_stacking_reset_timer() {
        let mut app = setup_app();
        let tag = TagId::from(1);
        app.insert_resource(StackingBehaviors::new()
            .stack(tag, StackingPolicy::NoStackingResetDuration)
        );

        let (entity, mut query) = setup_entity(&mut app);

        let effect = GameplayEffect::new(
            Some(tag),
            MyStats::Health,
            EffectMagnitude::Fixed(-1.0),
            EffectCalculation::Additive,
            EffectDuration::Continuous(Some(3.0.into())),
        );

        for i in 0..5 {
            app.world_mut().trigger(AddEffect(AddEffectData::new(entity, effect.clone(), None)));
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();

            let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            let target = 99.0 - i as f32;
            assert_eq!(health, target);
            assert_eq!(effects.0.iter().len(), 1);
        }
    }

    #[test] 
    fn test_multiple_effects_stacking() {
        let mut app = setup_app();

        let tag = TagId::from(1);
        app.insert_resource(StackingBehaviors::new()
            .stack(tag, StackingPolicy::MultipleEffects(3))
        );

        let (entity, mut query) = setup_entity(&mut app);

        let effect = GameplayEffect::new(
            Some(tag),
            MyStats::Health,
            EffectMagnitude::Fixed(-1.0),
            EffectCalculation::Additive,
            EffectDuration::Continuous(Some(5.0.into())),
        );

        let mut target = 100.;
        for i in 0..4 {
            app.world_mut().trigger(AddEffect(AddEffectData::new(entity, effect.clone(), None)));
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();
            let n_effects: usize = usize::min(i+1, 3);
            target -= n_effects as f32;

            let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(effects.0.iter().len(), n_effects);
            assert_eq!(health, target);
        }
        
        // effects should start timing out now
        let mut n_effects: i32 = 3;
        for _ in 0..6 {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();
            target -= n_effects as f32;
            n_effects = i32::max(0, n_effects - 1);

            let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(effects.0.iter().len(), n_effects as usize);
            assert_eq!(health, target);
        }
    }

    #[test] 
    fn test_multiple_effects_reset_timers_stacking() {
        let mut app = setup_app();

        let tag = TagId::from(1);
        app.insert_resource(StackingBehaviors::new()
            .stack(tag, StackingPolicy::MultipleEffectsResetDurations(3))
        );

        let (entity, mut query) = setup_entity(&mut app);

        let effect = GameplayEffect::new(
            Some(tag),
            MyStats::Health,
            EffectMagnitude::Fixed(-1.0),
            EffectCalculation::Additive,
            EffectDuration::Continuous(Some(5.0.into())),
        );

        let mut target = 100.;
        for i in 0..8 {
            app.world_mut().trigger(AddEffect(AddEffectData::new(entity, effect.clone(), None)));
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();
            let n_effects: usize = usize::min(i+1, 3);
            target -= n_effects as f32;

            let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(effects.0.iter().len(), n_effects);
            assert_eq!(health, target);
        }
        
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        target -= 15.;
        let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(effects.0.iter().len(), 0);
        assert_eq!(health, target);
    }


    #[test] 
    fn test_tag_effect() {
        let mut app = setup_app();
        let tag = TagId::from(1);
        let (entity, _) = setup_entity(&mut app);
        let mut query = app.world_mut()
            .query::<(Entity, &GameplayStats<MyStats>, &ActiveEffects<MyStats>, &ActiveTags)>();
        let effect = GameplayEffect::<MyStats>::tag_effect(tag, Some(5.0));
        app.world_mut().trigger(AddEffect(AddEffectData::new(entity, effect.clone(), None)));
        let (_, _, effects, tags) = query.iter(app.world_mut()).next().unwrap();
        assert!(tags.contains(&tag));
        assert_eq!(effects.iter().len(), 1);

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        let (_, _, effects, tags) = query.iter(app.world_mut()).next().unwrap();
        assert!(!tags.contains(&tag));
        assert_eq!(effects.iter().len(), 0);
    }
}