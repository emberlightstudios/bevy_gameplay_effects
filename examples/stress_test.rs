use std::time::Duration;

use bevy::prelude::*;
use bevy_hierarchical_tags::prelude::*;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::time::common_conditions::on_timer;
use bevy::window::PresentMode;
use bevy_gameplay_effects::prelude::*;

// Unfortunately effect systems are single threaded due to borrow issues
// but performance is still decent.

const ENTITIES_TO_SPAWN: usize = 150_000;

stats! (
    CharacterStats {
        Health,
        HealthRegen,
        Strength,
    }
);

#[derive(Resource)]
struct Tags {
    on_fire_tag: TagId,
    healing_tag: TagId,
}

fn main() {
    let mut app = App::new();
    let mut tag_registry: TagRegistry = TagRegistry::new();
    let on_fire_tag = tag_registry.register("Effect.Status.Burning");
    let healing_tag = tag_registry.register("Effect.Status.Healing");
    app.insert_resource(tag_registry);
    app.insert_resource(Tags{ on_fire_tag, healing_tag });

    let stacking_behaviors = StackingBehaviors::new()
        .stack(on_fire_tag, StackingPolicy::NoStackingResetDuration) 
        .stack(healing_tag, StackingPolicy::MultipleEffects(2)); // Can stack up to 2 healing effects

    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "No VSync".to_string(),
                resolution: (800u32, 600u32).into(),
                present_mode: PresentMode::Immediate, // <- disables VSync
                ..default()
            }),
            ..default()
        }),
        LogDiagnosticsPlugin::default(),
        FrameTimeDiagnosticsPlugin::default(),
        GameplayEffectsPlugin::<CharacterStats>::new(stacking_behaviors),
    ));

    app.add_systems(Startup, spawn_entities);
    app.add_systems(Update, (
        do_some_effects
            .run_if(on_timer(Duration::from_millis(500))),
        check_deaths,
    ));

    app.run();
}

fn spawn_entities(mut commands: Commands) {
    let active_effects = ActiveEffects::new([
        GameplayEffect::new(
            None,
            CharacterStats::Health,
            EffectMagnitude::Fixed(0.),
            EffectCalculation::LowerBound,
            EffectDuration::Persistent(None),
        ),
    ]);
    let stats = GameplayStats::new(
        |stat| {
            match stat {
                CharacterStats::Health => 100.,
                CharacterStats::HealthRegen => 1.,
                CharacterStats::Strength => 5.,
                CharacterStats::None =>  unreachable!() 
            }
        }
    );

    commands.spawn_batch((0..ENTITIES_TO_SPAWN).map(
        move |_| {
            (
                stats.clone(),
                active_effects.clone(),
            )
        })
    );
}

fn do_some_effects(
    mut commands: Commands,
    entities: Query<Entity, With<ActiveEffects<CharacterStats>>>,
) {
    let damage_effect = GameplayEffect::new(
        None,
        CharacterStats::Health,
        EffectMagnitude::LocalStat(CharacterStats::Strength, StatScalingParams{multiplier: -1.0, ..default()}),
        EffectCalculation::Additive,
        EffectDuration::Immediate,
    );

    for entity in entities {
        // Take some damage
        commands.trigger(AddEffect(AddEffectData::new(
            entity, damage_effect.clone(), None
        )));
    }
}

fn check_deaths(
    mut commands: Commands,
    mut events: MessageReader<OnBoundsBreached<CharacterStats>>,
    tags: Res<Tags>,
) {
    // Since all entities are receiving the same damage each frame they will all die
    // and fire these events at the same time.  This causes a big fps drop, down to 60 fps for me.
    // This is not a realistic in-game condition, but it shows that it can handle a good amount
    // of entities.

    // This will give 100 Health/s for 5 seconds
    let healing_effect = GameplayEffect::new(
        Some(tags.healing_tag),
        CharacterStats::Health,
        EffectMagnitude::Fixed(100.0),
        EffectCalculation::Additive,
        EffectDuration::Continuous(Some(5.0.into())),
    );
    for event in events.read() {
        if event.0.stat == CharacterStats::Health && event.0.bound == EffectCalculation::LowerBound {
            // Oh no entity died, let's heal him!
            commands.trigger(AddEffect(AddEffectData::new(event.target_entity, healing_effect.clone(), None)));
        }
    }
}