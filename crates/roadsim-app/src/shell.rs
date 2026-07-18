use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use egui_wgpu::{RendererOptions, WgpuConfiguration, winit::Painter};
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
            .filter(|frames| (1..=120).contains(frames));
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
    fatal_error: Option<ShellError>,
}

impl DesktopApp {
    const fn new(smoke: SmokeConfig) -> Self {
        Self {
            shell: None,
            smoke,
            rendered_frames: 0,
            fatal_error: None,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, message: impl Into<String>) {
        self.fatal_error = Some(ShellError::new(message));
        event_loop.exit();
    }

    fn rendered_frame(&mut self, event_loop: &ActiveEventLoop, scale_factor: f64) {
        self.rendered_frames += 1;
        if self
            .smoke
            .exit_after_frames
            .is_some_and(|limit| self.rendered_frames >= limit)
        {
            println!(
                "ROADSIM_SMOKE_OK frames={} scale_factor={scale_factor:.2}",
                self.rendered_frames
            );
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
        match WindowShell::new(window) {
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
                if shell.render() {
                    let scale_factor = shell.window.scale_factor();
                    shell.window.request_redraw();
                    self.rendered_frame(event_loop, scale_factor);
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
}

impl WindowShell {
    fn new(window: Arc<Window>) -> Result<Self, ShellError> {
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
        Ok(Self {
            window,
            egui_context,
            egui_state,
            painter,
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

    fn render(&mut self) -> bool {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return false;
        }

        let raw_input = self.egui_state.take_egui_input(&self.window);
        let scale_factor = self.window.scale_factor();
        let full_output = self.egui_context.run(raw_input, |context| {
            draw_shell(context, scale_factor);
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
        true
    }
}

fn draw_shell(context: &egui::Context, scale_factor: f64) {
    egui::TopBottomPanel::top("top_bar")
        .exact_height(42.0)
        .show(context, |ui| {
            ui.horizontal_centered(|ui| {
                ui.heading("RoadSim");
                ui.separator();
                ui.label("Проект без имени");
                ui.add_space(16.0);
                ui.colored_label(Color32::from_rgb(94, 211, 148), "● Готово");
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
                    let _ = ui.selectable_label(true, "▰ Corridor A");
                    ui.label("◇ Junction 1");
                    ui.label("↳ Signal program");
                });
            ui.add_space(12.0);
            ui.small("Статический preview; Design Model пока не изменяется из UI.");
        });

    egui::SidePanel::right("inspector")
        .default_width(260.0)
        .resizable(true)
        .show(context, |ui| {
            ui.heading("Инспектор");
            ui.separator();
            ui.label("Corridor A");
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
                    ui.label("Preview");
                    ui.end_row();
                });
            ui.add_space(16.0);
            ui.label("Симуляция");
            ui.horizontal(|ui| {
                let _ = ui.add_enabled(false, egui::Button::new("▶ Запустить"));
                let _ = ui.add_enabled(false, egui::Button::new("■ Стоп"));
            });
            ui.small("Backend contract будет подключён отдельным срезом.");
        });

    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(26.0)
        .show(context, |ui| {
            ui.horizontal_centered(|ui| {
                ui.small("Локальная CRS · metres");
                ui.separator();
                ui.small("120 FPS target");
                ui.separator();
                ui.small("x 0.0 m · y 0.0 m");
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::from_rgb(15, 21, 27)))
        .show(context, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter_at(rect);
            draw_grid(&painter, rect);
            draw_static_intersection(&painter, rect);
            painter.text(
                rect.left_top() + Vec2::new(16.0, 14.0),
                egui::Align2::LEFT_TOP,
                "STATIC ROAD VIEWPORT",
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

fn draw_static_intersection(painter: &egui::Painter, rect: Rect) {
    let center = rect.center();
    let road = Color32::from_rgb(55, 61, 66);
    let edge = Color32::from_rgb(194, 200, 203);
    let lane = Color32::from_rgb(224, 190, 78);
    let horizontal = [
        Pos2::new(rect.left() + 36.0, center.y),
        Pos2::new(rect.right() - 36.0, center.y),
    ];
    let vertical = [
        Pos2::new(center.x, rect.top() + 36.0),
        Pos2::new(center.x, rect.bottom() - 36.0),
    ];
    painter.line_segment(horizontal, Stroke::new(74.0, road));
    painter.line_segment(vertical, Stroke::new(74.0, road));
    for offset in [-37.0, 37.0] {
        painter.line_segment(
            [
                Pos2::new(horizontal[0].x, center.y + offset),
                Pos2::new(horizontal[1].x, center.y + offset),
            ],
            Stroke::new(2.0, edge),
        );
        painter.line_segment(
            [
                Pos2::new(center.x + offset, vertical[0].y),
                Pos2::new(center.x + offset, vertical[1].y),
            ],
            Stroke::new(2.0, edge),
        );
    }
    draw_dashed_line(painter, horizontal[0], horizontal[1], lane);
    draw_dashed_line(painter, vertical[0], vertical[1], lane);
    painter.circle_filled(center, 5.0, Color32::from_rgb(94, 211, 148));
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
    use super::SmokeConfig;

    #[test]
    fn smoke_frame_limit_is_bounded_and_explicit() {
        assert_eq!(SmokeConfig::from_value(None).exit_after_frames, None);
        assert_eq!(
            SmokeConfig::from_value(Some("2")).exit_after_frames,
            Some(2)
        );
        for invalid in ["", "0", "121", "not-a-number"] {
            assert_eq!(
                SmokeConfig::from_value(Some(invalid)).exit_after_frames,
                None
            );
        }
    }
}
