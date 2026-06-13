use crate::prelude::*;
use bevy::prelude::*;
use bevy_hierarchical_tags::TagId;

#[derive(Clone)]
pub struct AddEffectData<T: StatTrait> {
    pub target_entity: Entity,
    pub effect: GameplayEffect<T>,
    pub source_entity: Option<Entity>,
}

impl<T: StatTrait> AddEffectData<T> {
    pub fn new(
        target_entity: Entity,
        effect: GameplayEffect<T>,
        source_entity: Option<Entity>,
    ) -> Self {
        Self {
            effect,
            target_entity,
            source_entity,
        }
    }
}

pub struct EffectMetadata {
    pub target_entity: Entity,
    pub tag: Option<TagId>,
    pub source_entity: Option<Entity>,
}

impl EffectMetadata {
    pub fn new(target_entity: Entity, tag: Option<TagId>, source_entity: Option<Entity>) -> Self {
        Self {
            source_entity,
            target_entity,
            tag,
        }
    }
}

pub struct BoundsBreachedMetadata<T> {
    pub target_entity: Entity,
    pub stat: T,
    pub bound: EffectCalculation,
}

impl<T: StatTrait> BoundsBreachedMetadata<T> {
    pub fn new(entity: Entity, stat: T, bound: EffectCalculation) -> Self {
        Self {
            target_entity: entity,
            stat,
            bound,
        }
    }
}

#[derive(Event, Deref)]
pub struct AddEffect<T: StatTrait>(pub AddEffectData<T>);

#[derive(Event, Deref)]
pub struct RemoveEffect(pub EffectMetadata);

#[derive(Message, Deref)]
pub struct OnEffectAdded(pub EffectMetadata);

#[derive(Message, Deref)]
pub struct OnEffectRemoved(pub EffectMetadata);

#[derive(Message, Deref)]
pub struct OnRepeatingEffectTriggered(pub EffectMetadata);

#[derive(Message, Deref)]
pub struct OnBoundsBreached<T: StatTrait>(pub BoundsBreachedMetadata<T>);
