use bevy::prelude::Component;
use std::marker::PhantomData;

pub(crate) const STAT_LIMIT: usize = 16;

#[derive(Default, Copy, Clone)]
pub struct GameplayStat {
    pub current_value: f32,
    pub base_value: f32,
    pub(crate) modified_base: f32,
}

impl GameplayStat {
    pub fn new(base_value: f32, current_value: f32) -> Self {
        Self {
            base_value,
            current_value,
            modified_base: base_value,
        }
    }
}

pub trait StatTrait: Copy + Eq + Into<u8> + Send + Sync + 'static {
    const NONE: Self;
    fn variants() -> &'static [Self]; // all real variants, not including NONE
}

#[derive(Component, Clone)]
pub struct GameplayStats<T: StatTrait>([GameplayStat; STAT_LIMIT], PhantomData<T>);

impl<T: StatTrait> GameplayStats<T> {
    pub fn new(init: impl Fn(T) -> f32) -> Self {
        let variants = T::variants();
        assert!(variants.len() <= 16, "Max number of stat variants is 16");

        let mut instance = Self([GameplayStat::default(); STAT_LIMIT], PhantomData);

        for &variant in variants {
            let initial: f32 = init(variant);
            let index = variant.into() as usize;
            instance.0[index] = GameplayStat::new(initial, initial);
        }

        instance
    }

    pub fn get(&self, stat_variant: T) -> &GameplayStat {
        &self.0[stat_variant.into() as usize]
    }

    pub fn get_mut(&mut self, stat_variant: T) -> &mut GameplayStat {
        &mut self.0[stat_variant.into() as usize]
    }

    // TODO need to trigger recalculate effects after setting...
    pub fn set(&mut self, stat_variant: T, stat: GameplayStat) {
        self.0[stat_variant.into() as usize] = stat;
    }
}
