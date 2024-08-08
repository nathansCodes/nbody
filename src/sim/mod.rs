use crate::{assets::body, ui, utils, AppState};
use core::f32;
use std::collections::{HashMap, VecDeque};

use bevy::{
    ecs::system::SystemId,
    prelude::*,
    sprite::{MaterialMesh2dBundle, Mesh2dHandle},
};
use serde::Deserialize;

#[derive(Event)]
pub struct ClearTrajectories;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimState {
    Playing,
    Paused,
    Step,
}

#[derive(Resource)]
pub struct SimData {
    pub gravitational_const: f32,
    pub(super) trajectory_pos: usize,
}

impl Default for SimData {
    fn default() -> Self {
        Self {
            gravitational_const: 1.0,
            trajectory_pos: 1,
        }
    }
}

#[derive(Resource)]
struct OneShotSystems(HashMap<String, SystemId>);

#[derive(Component, Deserialize)]
pub struct Name(pub String);

#[derive(Component)]
pub struct Mass(pub f32);

#[derive(Component)]
pub struct Radius(pub f32);

#[derive(Clone, Copy)]
pub struct SimSnapshot {
    pub velocity: Vec2,
    pub position: Vec2,
}

#[derive(Component, Clone)]
pub(crate) struct Trajectory(VecDeque<SimSnapshot>);

#[derive(Component)]
pub struct TrajectoryVisibility(pub bool);

impl Trajectory {
    pub fn new(initial_pos: Vec2, initial_vel: Vec2) -> Self {
        Self(VecDeque::from([SimSnapshot {
            position: initial_pos,
            velocity: initial_vel,
        }]))
    }

    pub fn front(&self) -> Option<SimSnapshot> {
        self.0.front().cloned()
    }

    pub fn front_mut(&mut self) -> Option<&mut SimSnapshot> {
        self.0.front_mut()
    }

    pub fn pop_front(&mut self) -> Option<SimSnapshot> {
        self.0.pop_front()
    }

    fn push_back(&mut self, item: SimSnapshot) {
        self.0.push_back(item)
    }
}

#[derive(Component)]
pub(crate) struct Focused;

#[derive(Component)]
pub struct HoverIndicator;

#[derive(Bundle)]
struct CelestialBody {
    name: Name,
    mass: Mass,
    transform: Transform,
    radius: Radius,
    trajectory: Trajectory,
    trajectory_visibility: TrajectoryVisibility,
}

const TRAJECTORY_LEN: usize = 12000;
const TIME_STEP: f32 = 0.005;
// const G: f32 = 6.6743e-11;

pub fn recieve_asset_events(
    mut cmds: Commands,
    mut ev_asset: EventReader<AssetEvent<body::Body>>,
    assets: ResMut<Assets<body::Body>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for ev in ev_asset.read() {
        if let AssetEvent::LoadedWithDependencies { id } = ev {
            let body_asset = assets.get(*id).unwrap();

            let mesh = Mesh2dHandle(meshes.add(Circle {
                radius: body_asset.radius,
            }));

            let material = materials.add(body_asset.color);

            let transform =
                Transform::from_xyz(body_asset.initial_pos.x, body_asset.initial_pos.y, 0.0);

            let body = CelestialBody {
                mass: Mass(body_asset.mass),
                transform,
                radius: Radius(body_asset.radius),
                name: Name(body_asset.name.to_owned()),
                trajectory: Trajectory(VecDeque::from([SimSnapshot {
                    velocity: body_asset.velocity,
                    position: body_asset.initial_pos,
                }])),
                trajectory_visibility: TrajectoryVisibility(true),
            };

            cmds.spawn(MaterialMesh2dBundle {
                mesh,
                material,
                transform,
                ..default()
            })
            .insert(body);
        }
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimSystemSet;

fn simulate(mut sim: ResMut<SimData>, mut query: Query<(&mut Trajectory, &Mass, &Radius)>) {
    let mut query_items = query.iter_mut().collect::<Vec<_>>();

    if query_items.is_empty() {
        warn!("Nothing to simulate");
        return;
    }

    for i in sim.trajectory_pos - 1..TRAJECTORY_LEN - 1 {
        for j in 0..query_items.len() {
            let current_trajectory = &query_items[j].0;
            let _current_radius = &query_items[j].2 .0;
            let current = current_trajectory.0[i];

            let mut accel = Vec2::ZERO;

            for (k, (ref other_obj, Mass(other_mass), Radius(_other_radius))) in
                query_items.iter().enumerate()
            {
                let other_trajectory = &other_obj.0;
                let other = other_trajectory[i];
                if j == k {
                    continue;
                }

                let distance = other.position - current.position;

                let sqr_dist: f32 = distance.length_squared();
                let direction = distance.normalize();

                accel += direction * sim.gravitational_const * *other_mass / sqr_dist;
            }

            let velocity = current.velocity + accel * TIME_STEP;

            query_items[j].0.push_back(SimSnapshot {
                velocity,
                position: current.position + velocity * TIME_STEP,
            });
        }

        sim.trajectory_pos += 1;
    }
}

fn update_positions(
    mut sim: ResMut<SimData>,
    mut query: Query<(&mut Transform, &mut Trajectory, &Name)>,
) {
    if query.is_empty() {
        warn!("Nothing to update");
        return;
    }

    for (mut transform, mut trajectory, Name(_name)) in query.iter_mut() {
        if trajectory.0.is_empty() {
            warn!("Trajectory is empty");
            return;
        }
        let a = trajectory.pop_front().unwrap();
        // println!("{name} transform: {}", transform.translation);
        // println!("{name} velocity: {}", a.0);
        // println!();
        transform.translation = a.position.extend(0.0);
    }
    sim.trajectory_pos -= 1;
}

fn clear_trajectories_on_change(
    mut clear_ev: EventReader<ClearTrajectories>,
    mut trajectories: Query<&mut Trajectory>,
    mut sim: ResMut<SimData>,
) {
    for _ in clear_ev.read() {
        for mut traj in &mut trajectories {
            let current = traj.front().unwrap();

            traj.0.clear();

            traj.push_back(current);
        }
        sim.trajectory_pos = 1;
    }
}

fn draw_trajectories(
    mut gizmos: Gizmos,
    trajectories: Query<
        (&Trajectory, &TrajectoryVisibility, &Handle<ColorMaterial>),
        Without<Focused>,
    >,
    mats: Res<Assets<ColorMaterial>>,
    focused: Query<(Entity, &Trajectory), With<Focused>>,
) {
    for (Trajectory(traj), TrajectoryVisibility(vis), mat_handle) in trajectories.iter() {
        if !vis {
            continue;
        }
        let color = mats.get(mat_handle).unwrap().color;
        traj.iter()
            .zip(traj.iter().skip(1))
            .enumerate()
            .for_each(|(i, (a, b))| {
                let focused_pos = match focused.get_single() {
                    Ok(pos) => (
                        pos.1 .0.get(i).unwrap().position
                            - pos.1.front().expect("No front element").position,
                        pos.1 .0.get(i + 1).unwrap().position
                            - pos.1.front().expect("No front element").position,
                    ),
                    _ => (Vec2::ZERO, Vec2::ZERO),
                };

                gizmos.line_2d(
                    a.position - focused_pos.0,
                    b.position - focused_pos.1,
                    color.with_alpha(i as f32 / TRAJECTORY_LEN as f32 * -0.7 + 0.7),
                );
            });
    }
}

fn handle_input(
    state: Res<State<SimState>>,
    mut next_state: ResMut<NextState<SimState>>,
    kb: Res<ButtonInput<KeyCode>>,
    systems: Res<OneShotSystems>,
    mut cmds: Commands,
) {
    if kb.pressed(KeyCode::ArrowRight) {
        let id = systems.0["update_positions"];
        cmds.run_system(id);
    }

    if kb.just_pressed(KeyCode::Space) {
        let new_state = match state.get() {
            SimState::Playing => SimState::Paused,
            SimState::Paused => SimState::Playing,
            SimState::Step => SimState::Playing,
        };

        next_state.set(new_state);
    }
}

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        let mut one_shots = OneShotSystems(HashMap::new());

        one_shots.0.insert(
            "update_positions".into(),
            app.register_system(update_positions),
        );

        app.init_resource::<SimData>()
            .init_asset::<body::Body>()
            .init_asset_loader::<body::BodyLoader>()
            .insert_resource(one_shots)
            .insert_resource(Time::<Fixed>::from_hz(240.0))
            .insert_state(SimState::Paused)
            .add_event::<ClearTrajectories>()
            .configure_sets(Update, SimSystemSet.run_if(in_state(AppState::Simulating)))
            .configure_sets(
                FixedUpdate,
                SimSystemSet.run_if(in_state(AppState::Simulating)),
            )
            .add_systems(
                OnExit(AppState::Simulating),
                (utils::cleanup::<Trajectory>, crate::load_next_sim).chain(),
            )
            .add_systems(
                Update,
                (
                    draw_trajectories,
                    handle_input.run_if(not(ui::ui_is_hovered)),
                )
                    .in_set(SimSystemSet),
            )
            .add_systems(
                FixedUpdate,
                (
                    clear_trajectories_on_change,
                    simulate,
                    update_positions
                        .run_if(in_state(SimState::Playing).or_else(in_state(SimState::Step))),
                )
                    .in_set(SimSystemSet)
                    .chain(),
            )
            // only step once
            .add_systems(
                OnEnter(SimState::Step),
                |mut next_sim_state: ResMut<NextState<SimState>>| {
                    next_sim_state.set(SimState::Paused);
                },
            );
    }
}
