use crate::simulation::{SimulationController, SimulationObservation, UiSimulationState};
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use egui_wgpu::{RendererOptions, WgpuConfiguration, winit::Painter};
use roadsim_application::RunRequest;
use roadsim_backend_api::FrameBatch;
use roadsim_compiled_network::{CompiledNetwork, CompiledPoint};
use std::{error::Error, fmt, num::NonZeroU32, sync::Arc};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const WINDOW_TITLE: &str = "RoadSim — Desktop Preview";
const SMOKE_FRAMES_ENV: &str = "ROADSIM_SMOKE_EXIT_AFTER_FRAMES";

#[derive(Debug)]
pub struct ShellError(String);

impl ShellError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ShellError {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SmokeConfig {
    exit_after_frames: Option<u32>,
}

impl SmokeConfig {
    fn from_env() -> Self {
        Self::from_value(std::env::var(SMOKE_FRAMES_ENV).ok().as_deref())
    }

    fn from_value(value: Option<&str>) -> Self {
        let exit_after_frames = value
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|frames| (2..=120).contains(frames));
        Self { exit_after_frames }
    }
}

pub fn run() -> Result<(), ShellError> {
    let event_loop = EventLoop::new().map_err(|error| ShellError::new(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = DesktopApp::new(SmokeConfig::from_env());
    event_loop
        .run_app(&mut app)
        .map_err(|error| ShellError::new(error.to_string()))?;
    app.fatal_error.map_or(Ok(()), Err)
}

struct DesktopApp {
    shell: Option<WindowShell>,
    smoke: SmokeConfig,
    rendered_frames: u32,
    seen_simulation_frame: bool,
    smoke_reported: bool,
    fatal_error: Option<ShellError>,
}

impl DesktopApp {
    const fn new(smoke: SmokeConfig) -> Self {
        Self {
            shell: None,
            smoke,
            rendered_frames: 0,
            seen_simulation_frame: false,
            smoke_reported: false,
            fatal_error: None,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, message: impl Into<String>) {
        self.fatal_error = Some(ShellError::new(message));
        event_loop.exit();
    }

    fn rendered_frame(
        &mut self,
        event_loop: &ActiveEventLoop,
        scale_factor: f64,
        observation: SimulationObservation,
    ) {
        if self.smoke_reported {
            return;
        }
        self.rendered_frames += 1;
        self.seen_simulation_frame |= observation.agent_count > 0;
        if self
            .smoke
            .exit_after_frames
            .is_some_and(|limit| self.rendered_frames >= limit)
        {
            if !self.seen_simulation_frame {
                self.fail(event_loop, "simulation smoke produced no agent frame");
                return;
            }
            println!(
                "ROADSIM_SMOKE_OK frames={} scale_factor={scale_factor:.2} \
                 simulation_state={} tick={} agents={}",
                self.rendered_frames,
                observation.state.as_str(),
                observation.tick.get(),
                observation.agent_count,
            );
            self.smoke_reported = true;
            event_loop.exit();
        }
    }
}

impl ApplicationHandler for DesktopApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(shell) = &mut self.shell {
            if let Err(error) = shell.resume_surface() {
                self.fail(event_loop, error.to_string());
            } else {
                shell.window.request_redraw();
            }
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_min_inner_size(LogicalSize::new(800.0, 520.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.fail(event_loop, format!("window creation failed: {error}"));
                return;
            }
        };
        match WindowShell::new(window, self.smoke.exit_after_frames.is_some()) {
            Ok(shell) => {
                shell.window.request_redraw();
                self.shell = Some(shell);
            }
            Err(error) => self.fail(event_loop, error.to_string()),
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(shell) = &mut self.shell
            && let Err(error) = shell.suspend_surface()
        {
            self.fail(event_loop, error.to_string());
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(shell) = &mut self.shell else {
            return;
        };
        if shell.window.id() != window_id {
            return;
        }

        let response = shell.egui_state.on_window_event(&shell.window, &event);
        if response.repaint {
            shell.window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                shell.resize(size.width, size.height);
                shell.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                eprintln!("RoadSim DPI scale factor changed to {scale_factor:.2}");
                shell.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Some(observation) = shell.render() {
                    let scale_factor = shell.window.scale_factor();
                    shell.window.request_redraw();
                    self.rendered_frame(event_loop, scale_factor, observation);
                }
            }
            _ => {}
        }
    }
}

struct WindowShell {
    window: Arc<Window>,
    egui_context: egui::Context,
    egui_state: egui_winit::State,
    painter: Painter,
    simulation: SimulationController,
}

impl WindowShell {
    fn new(window: Arc<Window>, auto_start: bool) -> Result<Self, ShellError> {
        let egui_context = egui::Context::default();
        egui_context.set_zoom_factor(1.0);
        let egui_state = egui_winit::State::new(
            egui_context.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );
        let mut painter = pollster::block_on(Painter::new(
            egui_context.clone(),
            WgpuConfiguration::default(),
            false,
            RendererOptions::default(),
        ));
        pollster::block_on(painter.set_window(egui::ViewportId::ROOT, Some(window.clone())))
            .map_err(|error| ShellError::new(format!("GPU initialization failed: {error}")))?;
        let simulation = SimulationController::new(auto_start).map_err(|error| {
            ShellError::new(format!("simulation initialization failed: {error}"))
        })?;
        Ok(Self {
            window,
            egui_context,
            egui_state,
            painter,
            simulation,
        })
    }

    fn resume_surface(&mut self) -> Result<(), ShellError> {
        pollster::block_on(
            self.painter
                .set_window(egui::ViewportId::ROOT, Some(self.window.clone())),
        )
        .map_err(|error| ShellError::new(format!("GPU surface resume failed: {error}")))
    }

    fn suspend_surface(&mut self) -> Result<(), ShellError> {
        pollster::block_on(self.painter.set_window(egui::ViewportId::ROOT, None))
            .map_err(|error| ShellError::new(format!("GPU surface suspend failed: {error}")))
    }

    fn resize(&mut self, width: u32, height: u32) {
        let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            return;
        };
        self.painter
            .on_window_resized(egui::ViewportId::ROOT, width, height);
    }

    fn render(&mut self) -> Option<SimulationObservation> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return None;
        }

        self.simulation.advance();
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let scale_factor = self.window.scale_factor();
        let simulation = &mut self.simulation;
        let full_output = self.egui_context.run(raw_input, |context| {
            draw_shell(context, scale_factor, simulation);
        });
        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);
        let pixels_per_point = self.egui_context.pixels_per_point();
        let primitives = self
            .egui_context
            .tessellate(full_output.shapes, pixels_per_point);
        self.painter.paint_and_update_textures(
            egui::ViewportId::ROOT,
            pixels_per_point,
            [0.035, 0.047, 0.059, 1.0],
            &primitives,
            &full_output.textures_delta,
            Vec::new(),
        );
        Some(self.simulation.observation())
    }
}

fn draw_shell(context: &egui::Context, scale_factor: f64, simulation: &mut SimulationController) {
    egui::TopBottomPanel::top("top_bar")
        .exact_height(42.0)
        .show(context, |ui| {
            ui.horizontal_centered(|ui| {
                ui.heading("RoadSim");
                ui.separator();
                ui.label("Встроенный straight-road demo");
                ui.add_space(16.0);
                let status_color = match simulation.state() {
                    UiSimulationState::Running => Color32::from_rgb(94, 211, 148),
                    UiSimulationState::Paused => Color32::from_rgb(224, 190, 78),
                    UiSimulationState::Failed => Color32::from_rgb(235, 99, 99),
                    _ => Color32::from_gray(180),
                };
                ui.colored_label(status_color, format!("● {}", simulation.state().label()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("GPU viewport · DPI {scale_factor:.2}×"));
                });
            });
        });

    egui::SidePanel::left("project_tree")
        .default_width(220.0)
        .resizable(true)
        .show(context, |ui| {
            ui.heading("Объекты");
            ui.separator();
            egui::CollapsingHeader::new("Проект")
                .default_open(true)
                .show(ui, |ui| {
                    let _ = ui.selectable_label(true, "▰ Demo corridor");
                    ui.label("↳ Lane 0101 · reverse");
                    ui.label("↳ Lane 0102 · forward");
                });
            ui.add_space(12.0);
            ui.small("Валидный Design Model → immutable CSN. UI не изменяет модель.");
        });

    egui::SidePanel::right("inspector")
        .default_width(260.0)
        .resizable(true)
        .show(context, |ui| {
            ui.heading("Инспектор");
            ui.separator();
            ui.label("Demo corridor");
            egui::Grid::new("properties")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Длина");
                    ui.label("120.0 m");
                    ui.end_row();
                    ui.label("Полосы");
                    ui.label("2");
                    ui.end_row();
                    ui.label("Режим");
                    ui.label("Compiled CSN");
                    ui.end_row();
                });
            ui.add_space(16.0);
            ui.label("Симуляция");
            ui.small("roadsim.fake.v1 · seed 20260718");
            ui.label(format!("Tick: {}", simulation.tick().get()));
            ui.horizontal(|ui| {
                // Enablement comes from the orchestrator, so a button can
                // never offer a transition the run would refuse.
                let can_start =
                    simulation.accepts(RunRequest::Start) || simulation.accepts(RunRequest::Reset);
                if ui
                    .add_enabled(can_start, egui::Button::new("▶ Запустить"))
                    .clicked()
                {
                    simulation.start();
                }
                if simulation.accepts(RunRequest::Resume) {
                    if ui.button("▶ Продолжить").clicked() {
                        simulation.resume();
                    }
                } else if ui
                    .add_enabled(
                        simulation.accepts(RunRequest::Pause),
                        egui::Button::new("Ⅱ Пауза"),
                    )
                    .clicked()
                {
                    simulation.pause();
                }
                let can_stop = simulation.accepts(RunRequest::Cancel);
                if ui
                    .add_enabled(can_stop, egui::Button::new("■ Стоп"))
                    .clicked()
                {
                    simulation.stop();
                }
            });
            if let Some(code) = simulation.error_code() {
                ui.colored_label(Color32::from_rgb(235, 99, 99), code);
            } else {
                ui.small("Test backend; SUMO worker не подключён.");
            }
        });

    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(26.0)
        .show(context, |ui| {
            ui.horizontal_centered(|ui| {
                ui.small("Локальная CRS · metres");
                ui.separator();
                ui.small(format!("tick {}", simulation.tick().get()));
                ui.separator();
                ui.small(format!(
                    "agents {}",
                    simulation.frame().map_or(0, |frame| frame.agents().len())
                ));
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::from_rgb(15, 21, 27)))
        .show(context, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter_at(rect);
            draw_grid(&painter, rect);
            draw_network(&painter, rect, simulation.network(), simulation.frame());
            painter.text(
                rect.left_top() + Vec2::new(16.0, 14.0),
                egui::Align2::LEFT_TOP,
                "CSN VIEWPORT · DETERMINISTIC FAKE BACKEND",
                egui::FontId::monospace(12.0),
                Color32::from_gray(145),
            );
        });
}

fn draw_grid(painter: &egui::Painter, rect: Rect) {
    let spacing = 32.0;
    let color = Color32::from_rgba_unmultiplied(85, 104, 119, 35);
    let mut x = rect.left();
    while x <= rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, color),
        );
        x += spacing;
    }
    let mut y = rect.top();
    while y <= rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0, color),
        );
        y += spacing;
    }
}

fn draw_network(
    painter: &egui::Painter,
    rect: Rect,
    network: &CompiledNetwork,
    frame: Option<&FrameBatch>,
) {
    let Some(transform) = ViewTransform::for_network(network, rect) else {
        return;
    };
    for lane in network.lanes().iter() {
        let start = transform.map(lane.start());
        let end = transform.map(lane.end());
        let road_width = (lane.width_m() as f32 * transform.scale).clamp(8.0, 30.0);
        painter.line_segment(
            [start, end],
            Stroke::new(road_width + 2.0, Color32::from_rgb(188, 194, 198)),
        );
        painter.line_segment(
            [start, end],
            Stroke::new(road_width, Color32::from_rgb(55, 61, 66)),
        );
        draw_dashed_line(painter, start, end, Color32::from_rgb(224, 190, 78));
    }

    if let Some(frame) = frame {
        for agent in frame.agents() {
            let position = transform.map_xy(agent.x_m(), agent.y_m());
            let heading = agent.heading_rad() as f32;
            let footprint = agent.footprint();
            let length_px = footprint.length_m() as f32 * transform.scale;
            let width_px = footprint.width_m() as f32 * transform.scale;
            let corners = vehicle_corners(position, heading, length_px, width_px);
            painter.add(egui::Shape::convex_polygon(
                corners.to_vec(),
                Color32::from_rgb(68, 170, 235),
                Stroke::new(1.0, Color32::from_rgb(224, 242, 255)),
            ));
            let nose =
                position + Vec2::new(heading.cos(), -heading.sin()) * (length_px * 0.5).max(2.0);
            painter.line_segment(
                [position, nose],
                Stroke::new(2.0, Color32::from_rgb(224, 242, 255)),
            );
        }
    }

    let hash = network.header().content_hash().to_string();
    painter.text(
        rect.left_bottom() + Vec2::new(16.0, -14.0),
        egui::Align2::LEFT_BOTTOM,
        format!("CSN {} · {} lanes", &hash[..8], network.lanes().len()),
        egui::FontId::monospace(11.0),
        Color32::from_gray(135),
    );
}

fn vehicle_corners(center: Pos2, heading_rad: f32, length_px: f32, width_px: f32) -> [Pos2; 4] {
    let forward = Vec2::new(heading_rad.cos(), -heading_rad.sin());
    let right = Vec2::new(heading_rad.sin(), heading_rad.cos());
    let half_length = forward * (length_px * 0.5);
    let half_width = right * (width_px * 0.5);
    [
        center + half_length + half_width,
        center + half_length - half_width,
        center - half_length - half_width,
        center - half_length + half_width,
    ]
}

struct ViewTransform {
    center_x: f64,
    center_y: f64,
    screen_center: Pos2,
    scale: f32,
}

impl ViewTransform {
    fn for_network(network: &CompiledNetwork, rect: Rect) -> Option<Self> {
        let mut points = network
            .lanes()
            .iter()
            .flat_map(|lane| [lane.start(), lane.end()]);
        let first = points.next()?;
        let (mut min_x, mut max_x) = (first.x_m(), first.x_m());
        let (mut min_y, mut max_y) = (first.y_m(), first.y_m());
        for point in points {
            min_x = min_x.min(point.x_m());
            max_x = max_x.max(point.x_m());
            min_y = min_y.min(point.y_m());
            max_y = max_y.max(point.y_m());
        }
        let width_m = (max_x - min_x + 20.0).max(20.0);
        let height_m = (max_y - min_y + 20.0).max(20.0);
        let available = rect.shrink(48.0);
        let scale = (available.width() / width_m as f32)
            .min(available.height() / height_m as f32)
            .max(0.01);
        Some(Self {
            center_x: (min_x + max_x) * 0.5,
            center_y: (min_y + max_y) * 0.5,
            screen_center: rect.center(),
            scale,
        })
    }

    fn map(&self, point: CompiledPoint) -> Pos2 {
        self.map_xy(point.x_m(), point.y_m())
    }

    fn map_xy(&self, x_m: f64, y_m: f64) -> Pos2 {
        Pos2::new(
            self.screen_center.x + (x_m - self.center_x) as f32 * self.scale,
            self.screen_center.y - (y_m - self.center_y) as f32 * self.scale,
        )
    }
}

fn draw_dashed_line(painter: &egui::Painter, start: Pos2, end: Pos2, color: Color32) {
    let delta = end - start;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }
    let direction = delta / length;
    let mut offset = 0.0;
    while offset < length {
        let segment_end = (offset + 12.0).min(length);
        painter.line_segment(
            [start + direction * offset, start + direction * segment_end],
            Stroke::new(1.5, color),
        );
        offset += 22.0;
    }
}

#[cfg(test)]
mod tests {
    use super::{SmokeConfig, vehicle_corners};
    use egui::Pos2;

    #[test]
    fn smoke_frame_limit_is_bounded_and_explicit() {
        assert_eq!(SmokeConfig::from_value(None).exit_after_frames, None);
        assert_eq!(
            SmokeConfig::from_value(Some("2")).exit_after_frames,
            Some(2)
        );
        for invalid in ["", "0", "1", "121", "not-a-number"] {
            assert_eq!(
                SmokeConfig::from_value(Some(invalid)).exit_after_frames,
                None
            );
        }
    }

    #[test]
    fn vehicle_rectangle_preserves_metric_aspect_and_heading() {
        let horizontal = vehicle_corners(Pos2::new(10.0, 20.0), 0.0, 45.0, 18.0);
        assert_eq!(horizontal[0], Pos2::new(32.5, 29.0));
        assert_eq!(horizontal[2], Pos2::new(-12.5, 11.0));

        let vertical = vehicle_corners(
            Pos2::new(10.0, 20.0),
            std::f32::consts::FRAC_PI_2,
            45.0,
            18.0,
        );
        assert!((vertical[0].x - 19.0).abs() < 1.0e-5);
        assert!((vertical[0].y + 2.5).abs() < 1.0e-5);
    }
}
