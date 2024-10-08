use bevy::{
    ecs::system::SystemId,
    input::mouse::{MouseScrollUnit, MouseWheel},
    prelude::*,
    sprite::{MaterialMesh2dBundle, Mesh2dHandle},
    utils::hashbrown::HashMap,
    window::PrimaryWindow,
};

use crate::{
    sim::{self, Follow, Hover, Mass, Name, Radius, SimSnapshot, Trajectory, TrajectoryVisibility},
    ui::{self, Inspect},
    utils, AppState,
};

#[derive(Resource, Default)]
struct ControlState {
    cam_origin: Vec2,
    frame_delta: Vec2,
}

#[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
enum ControlMode {
    #[default]
    Normal,
    Spawn,
}

#[derive(Component)]
pub struct SimCamera;

#[derive(Component)]
struct PreSpawn;

// This is used for zooming into the cursor instead of the cursor location.
// The cursor's world position cannot be calculated immediately after updating the
// projection's scale because the camera only gets updated in
// `bevy::render::camera::camera_system`, not immediately after changing it.
// This is why I update a fake camera that doesn't render, and only update the real camera's scale
// and position in the very next update cycle.
#[derive(Component)]
struct FakeCam;

fn setup(mut cmds: Commands) {
    cmds.spawn(Camera2dBundle::default())
        .insert(SimCamera)
        .insert(IsDefaultUiCamera);
    cmds.spawn((Camera::default(), OrthographicProjection::default()))
        .insert(FakeCam);
}

fn setup_cam_zoom(mut query: Query<&mut OrthographicProjection, With<SimCamera>>) {
    query.single_mut().scale = 0.05_f32.exp();
}

fn spawn_fake_body(
    q_windows: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform, &OrthographicProjection), With<SimCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut cmds: Commands,
) {
    let (cam, cam_global_transform, projection) = q_camera.single();

    let mouse_position = cam
        .viewport_to_world_2d(
            cam_global_transform,
            q_windows.single().cursor_position().unwrap_or(Vec2::ZERO),
        )
        .unwrap_or(Vec2::ZERO);
    let radius = (projection.area.min.y - projection.area.max.y).abs() * 0.01;

    let color = bevy::color::palettes::tailwind::RED_600;

    let mat_mesh_2d = MaterialMesh2dBundle {
        mesh: Mesh2dHandle(meshes.add(Circle { radius })),
        material: materials.add(Color::Srgba(color)),
        transform: Transform::from_xyz(mouse_position.x, mouse_position.y, 0.0),
        ..default()
    };

    let name = Name("New Body".to_string());
    let mass = Mass(radius * 100.0);
    let radius = Radius(radius);

    cmds.spawn(mat_mesh_2d)
        .insert((radius, name, mass, PreSpawn));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn cam_controller_core(
    kb: Res<ButtonInput<KeyCode>>,
    mut q_camera: Query<(&Camera, &GlobalTransform, &mut Transform), With<SimCamera>>,
    q_focused: Query<(Entity, &Transform), (With<sim::Follow>, Without<SimCamera>)>,
    q_bodies: Query<(Entity, &Trajectory, &Radius)>,
    q_windows: Query<&Window, With<PrimaryWindow>>,
    q_already_followed: Query<Entity, With<Follow>>,
    q_already_inspected: Query<Entity, With<Inspect>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut control_state: ResMut<ControlState>,
    mut next_ctrl_mode: ResMut<NextState<ControlMode>>,
    one_shots: Res<OneShotSystems>,
    mut cmds: Commands,
) {
    let focused = q_focused.get_single().ok();
    let (cam, cam_global_transform, mut cam_transform) = q_camera.single_mut();

    control_state.frame_delta = Vec2::ZERO;

    if let Some((e, transform)) = focused {
        cam_transform.translation -= control_state.cam_origin.extend(0.0);
        control_state.cam_origin = transform.translation.xy();
        if kb.pressed(KeyCode::Escape) {
            cmds.entity(e).remove::<sim::Follow>();
        }
    } else {
        control_state.cam_origin = Vec2::ZERO;
    }

    if let Some(cursor_pos) = q_windows.single().cursor_position() {
        for (entity_id, trajectory, Radius(radius)) in q_bodies.iter() {
            let SimSnapshot {
                velocity: _,
                position,
            } = trajectory.front().unwrap();
            // convert to world space
            let cursor_pos = cam
                .viewport_to_world_2d(cam_global_transform, cursor_pos)
                .unwrap();

            if cursor_pos.x > position.x - radius
                && cursor_pos.x < position.x + radius
                && cursor_pos.y > position.y - radius
                && cursor_pos.y < position.y + radius
            {
                cmds.entity(entity_id).try_insert(Hover);
                if mouse.just_pressed(MouseButton::Middle) {
                    cmds.entity(entity_id).insert(Follow);

                    for entity_id in q_already_followed.iter() {
                        cmds.entity(entity_id).remove::<Follow>();
                    }
                }

                if mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Middle)
                {
                    cmds.entity(entity_id).insert(Inspect);

                    for entity_id in q_already_inspected.iter() {
                        cmds.entity(entity_id).remove::<Inspect>();
                    }
                }
            } else {
                cmds.entity(entity_id).remove::<Hover>();
            }
        }
    }

    if kb.pressed(KeyCode::ControlLeft) && kb.just_pressed(KeyCode::KeyN) {
        next_ctrl_mode.set(ControlMode::Spawn);
        cmds.run_system(one_shots.0["spawn_fake_body"]);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn cam_controller_normal(
    mut q_camera: Query<
        (&mut OrthographicProjection, &Camera, &GlobalTransform),
        (With<Camera2d>, With<SimCamera>),
    >,
    mut q_fake_camera: Query<
        (&mut OrthographicProjection, &Camera),
        (With<FakeCam>, Without<SimCamera>),
    >,
    q_windows: Query<&Window, With<PrimaryWindow>>,
    mut wheel: EventReader<MouseWheel>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor_moved: EventReader<CursorMoved>,
    mut control_state: ResMut<ControlState>,
    mut zoom_diff: Local<Option<Vec2>>,
    time: Res<Time>,
) {
    let dt = time.delta_seconds();

    let (mut projection, cam, global_transform) = q_camera.single_mut();
    let (mut fake_projection, fake_cam) = q_fake_camera.single_mut();

    let mut log_scale = projection.scale.ln();

    if let Some(prev_cursor_pos) = *zoom_diff {
        let current_cursor_pos = fake_cam
            .viewport_to_world_2d(
                global_transform,
                q_windows.single().cursor_position().unwrap(),
            )
            .unwrap();

        projection.scale = fake_projection.scale;
        control_state.frame_delta += prev_cursor_pos - current_cursor_pos;

        *zoom_diff = None;
    }

    for ev in wheel.read() {
        log_scale -= ev.y
            * dt
            * match ev.unit {
                MouseScrollUnit::Line => 10.0,
                MouseScrollUnit::Pixel => 7.0,
            };
        fake_projection.scale = log_scale.exp();
        *zoom_diff = Some(
            cam.viewport_to_world_2d(
                global_transform,
                q_windows.single().cursor_position().unwrap(),
            )
            .unwrap(),
        );
    }

    if mouse.pressed(MouseButton::Left) {
        for ev in cursor_moved.read() {
            if let Some(delta) = ev.delta {
                control_state.frame_delta += Vec2::new(-delta.x, delta.y) * log_scale.exp();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cam_controller_spawn(
    kb: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut wheel: EventReader<MouseWheel>,
    q_windows: Query<&Window, With<PrimaryWindow>>,
    mut q_pre_spawn: Query<(Entity, &mut Transform, &mut Radius), With<PreSpawn>>,
    mut q_camera: Query<(&Camera, &GlobalTransform, &OrthographicProjection), With<SimCamera>>,
    mut next_ctrl_mode: ResMut<NextState<ControlMode>>,
    mut control_state: ResMut<ControlState>,
    mut clear_traj_evw: EventWriter<sim::ClearTrajectories>,
    q_focused: Query<&sim::Trajectory, With<sim::Follow>>,
    mut gizmos: Gizmos,
    mut cmds: Commands,
) {
    let (cam, cam_global_transform, _projection) = q_camera.single_mut();
    control_state.frame_delta = Vec2::ZERO;

    let mouse_position = cam
        .viewport_to_world_2d(
            cam_global_transform,
            q_windows.single().cursor_position().unwrap_or(Vec2::ZERO),
        )
        .unwrap_or(Vec2::ZERO);

    let pre_spawn = q_pre_spawn.get_single_mut().ok();

    if pre_spawn.is_none() {
        return;
    }

    let (entity, mut transform, mut radius) = pre_spawn.unwrap();

    if mouse.just_released(MouseButton::Left) {
        cmds.entity(entity).remove::<PreSpawn>().insert((
            sim::Trajectory::new(
                transform.translation.xy(),
                transform.translation.xy() - mouse_position
                    + q_focused
                        .get_single()
                        .map(|q| q.front().unwrap().velocity)
                        .unwrap_or(Vec2::ZERO),
            ),
            TrajectoryVisibility(true),
        ));

        clear_traj_evw.send(sim::ClearTrajectories);
        next_ctrl_mode.set(ControlMode::Normal);
        return;
    }

    if mouse.pressed(MouseButton::Left) {
        let transform_2d = transform.translation.xy();
        gizmos.arrow_2d(
            transform_2d,
            transform_2d + (transform_2d - mouse_position),
            Color::WHITE,
        );
    } else {
        transform.translation = mouse_position.extend(0.0);
    }

    for ev in wheel.read() {
        radius.0 += ev.y;
    }

    if kb.pressed(KeyCode::Escape) {
        next_ctrl_mode.set(ControlMode::Normal);
        cmds.entity(entity).despawn();
    }
}

fn cam_controller_wasd(
    q_projection: Query<&OrthographicProjection, (With<Camera2d>, With<SimCamera>)>,
    mut control_state: ResMut<ControlState>,
    time: Res<Time>,
    kb: Res<ButtonInput<KeyCode>>,
) {
    let projection = q_projection.single();

    let dt = time.delta_seconds();

    let cam_speed: f32 = 300.0 * projection.scale;
    let dist = cam_speed * dt;

    if kb.pressed(KeyCode::KeyW) {
        control_state.frame_delta.y += dist;
    }
    if kb.pressed(KeyCode::KeyA) {
        control_state.frame_delta.x -= dist;
    }
    if kb.pressed(KeyCode::KeyS) {
        control_state.frame_delta.y -= dist;
    }
    if kb.pressed(KeyCode::KeyD) {
        control_state.frame_delta.x += dist;
    }
}

fn cam_controller_apply(
    mut cam_transform: Query<&mut Transform, (With<Camera2d>, With<SimCamera>)>,
    control_state: Res<ControlState>,
) {
    let mut transform = cam_transform.single_mut();
    transform.translation +=
        control_state.frame_delta.extend(0.0) + control_state.cam_origin.extend(0.0);
}

#[derive(Resource)]
struct OneShotSystems(HashMap<String, SystemId>);

#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub struct ControlSystemSet;

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        let mut one_shots = OneShotSystems(HashMap::new());

        one_shots.0.insert(
            "spawn_fake_body".into(),
            app.register_system(spawn_fake_body),
        );

        app.insert_resource(ClearColor(Color::BLACK))
            .insert_resource(ControlState::default())
            .insert_resource(one_shots)
            .insert_state(ControlMode::Normal)
            .configure_sets(
                PostUpdate,
                ControlSystemSet
                    .run_if(utils::primary_window_exists)
                    .run_if(in_state(AppState::Simulating))
                    .after(bevy::render::camera::CameraUpdateSystem)
                    .after(TransformSystem::TransformPropagate),
            )
            .add_systems(Startup, (setup, setup_cam_zoom).chain())
            .add_systems(
                PostUpdate,
                (
                    cam_controller_core,
                    (
                        cam_controller_normal.run_if(in_state(ControlMode::Normal)),
                        cam_controller_spawn.run_if(in_state(ControlMode::Spawn)),
                        cam_controller_wasd,
                    )
                        .chain()
                        .run_if(not(ui::ui_is_active)),
                    cam_controller_apply,
                )
                    .in_set(ControlSystemSet)
                    .chain(),
            );
    }
}
