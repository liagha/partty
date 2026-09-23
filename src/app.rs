use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::clip::Clip;
use crate::config::Config;
use crate::grid::Grid;
use crate::shell::Shell;
use crate::text::Text;
use crate::view::View;

#[derive(Default)]
pub struct App {
    window: Option<Arc<Window>>,
    view: Option<View>,
    shell: Option<Shell>,
    inbox: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
    grid: Grid,
    parse: vte::Parser,
    wake: Option<EventLoopProxy<()>>,
    mods: ModifiersState,
    at: (f32, f32),
    drag: bool,
    held: u8,
    font: f32,
    next: Option<std::time::Instant>,
}

impl App {
    pub fn run() {
        let event = winit::event_loop::EventLoop::new().unwrap();
        event.set_control_flow(ControlFlow::Wait);
        let mut app = Self::default();
        app.wake = Some(event.create_proxy());
        event.run_app(&mut app).unwrap();
    }

    fn flush(&mut self) {
        let reply = self.grid.take_reply();
        if !reply.is_empty() {
            if let Some(shell) = self.shell.as_mut() {
                shell.write(&reply);
            }
        }
    }

    fn feed(&mut self) -> bool {
        let mut fresh = false;
        loop {
            match self.inbox.as_ref().map(|inbox| inbox.try_recv()) {
                Some(Ok(bytes)) => {
                    let was = self.grid.cursor();
                    let (clean, payloads) = self.grid.split(&bytes);
                    for p in &payloads {
                        self.grid.apc(p);
                    }
                    self.parse.advance(&mut self.grid, &clean);
                    self.grid.moved(was);
                    self.grid.lit();
                    self.flush();
                    fresh = true;
                }
                _ => break,
            }
        }
        if fresh {
            self.show();
            if let Some(title) = self.grid.take_title() {
                if let Some(window) = self.window.as_ref() {
                    window.set_title(&title);
                }
            }
        }
        fresh
    }

    fn show(&mut self) {
        if !self.grid.take_dirty() {
            return;
        }
        let styled = self.grid.spans();
        if let Some(view) = self.view.as_mut() {
            let geo = view.show(&styled);
            self.grid.set_px(geo.adv, geo.step);
            let pics = self.grid.pics();
            view.pics(&pics, &|id| self.grid.entry(id));
        }
    }

    fn press(&mut self, button: MouseButton, state: ElementState) {
        let btn = match button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            _ => return,
        };
        let Some((row, col)) = self.spot() else {
            return;
        };
        match state {
            ElementState::Pressed => {
                self.held = btn;
                self.drag = true;
                self.grid.click(btn, row, col, true);
            }
            ElementState::Released => {
                self.drag = false;
                self.grid.click(btn, row, col, false);
            }
        }
    }

    fn spot(&self) -> Option<(usize, usize)> {
        let view = self.view.as_ref()?;
        view.spot(self.at.0, self.at.1)
    }

    fn paste(&mut self, primary: bool) {
        let text = Clip::paste(primary);
        if !text.is_empty() {
            if let Some(shell) = self.shell.as_mut() {
                shell.write(&Clip::wrap(&text, self.grid.pastes()));
            }
        }
    }

    fn fit(&mut self) {
        let window = match self.window.as_ref() {
            Some(window) => window.clone(),
            None => return,
        };
        let size = window.inner_size();
        let (rows, cols) = Text::cells(
            size.width,
            size.height,
            window.scale_factor() as f32,
            self.font,
        );
        self.grid.resize(rows, cols);
        if let Some(shell) = self.shell.as_mut() {
            let _ = shell.resize(rows, cols);
        }
        self.show();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, loop_: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("partty");
        let window = Arc::new(loop_.create_window(attrs).unwrap());
        let (config, source) = Config::load();
        self.font = config.size;
        self.view = Some(View::open(
            window.clone(),
            loop_,
            config.bg,
            config.size,
            &config.face,
            &config.files,
        ));
        let size = window.inner_size();
        let (rows, cols) = Text::cells(
            size.width,
            size.height,
            window.scale_factor() as f32,
            config.size,
        );
        let origin = source
            .map(|s| s.display().to_string())
            .unwrap_or("<defaults>".into());
        eprintln!("partty: {rows}x{cols} font={} config={origin}", config.size);
        self.grid = Grid::new(rows, cols, config.below.unwrap_or(rows), config.inks);
        let wake = self.wake.clone().expect("proxy");
        let (shell, inbox) = Shell::open(rows, cols, wake);
        self.shell = Some(shell);
        self.inbox = Some(inbox);
        self.window = Some(window);
    }

    fn window_event(&mut self, loop_: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let window = match self.window.as_ref() {
            Some(window) if window.id() == id => window.clone(),
            _ => return,
        };
        match event {
            WindowEvent::CloseRequested => loop_.exit(),
            WindowEvent::Resized(size) => {
                if let Some(view) = self.view.as_mut() {
                    view.resize(size.width, size.height, window.scale_factor() as f32);
                }
                self.fit();
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(view) = self.view.as_mut() {
                    let size = window.inner_size();
                    view.resize(size.width, size.height, scale_factor as f32);
                }
                self.fit();
                window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let hot = self.mods.control_key() && self.mods.shift_key();
                let press = event.state == ElementState::Pressed && !event.repeat;
                let vee = event.physical_key == PhysicalKey::Code(KeyCode::KeyV);
                let insert = event.physical_key == PhysicalKey::Code(KeyCode::Insert);
                match &event.logical_key {
                    Key::Character(c) if hot && (c == "v" || c == "V" || vee) => {
                        if press {
                            self.paste(false)
                        }
                    }
                    _ if hot && vee => {
                        if press {
                            self.paste(false)
                        }
                    }
                    _ if press && insert && self.mods.shift_key() => self.paste(false),
                    _ => {
                        let bytes = Shell::key(
                            event.state,
                            &event.logical_key,
                            event.text.as_deref(),
                            self.mods.control_key(),
                            self.mods.alt_key(),
                        );
                        if let Some(bytes) = bytes {
                            if let Some(shell) = self.shell.as_mut() {
                                shell.write(&bytes);
                            }
                        }
                    }
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.mods = mods.state();
            }
            WindowEvent::Focused(inside) => {
                if !inside {
                    self.drag = false;
                }
                if self.grid.focused() {
                    self.grid.focus(inside);
                    self.flush();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.at = (position.x as f32, position.y as f32);
                if !self.grid.mouse() {
                    return;
                }
                if self.drag && self.grid.motion() {
                    if let Some((row, col)) = self.spot() {
                        self.grid.click(self.held, row, col, true);
                    }
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if self.grid.mouse() {
                    self.press(button, state);
                } else if let (MouseButton::Middle, ElementState::Pressed) = (button, state) {
                    self.paste(true);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as f32,
                };
                if self.grid.mouse() {
                    let n = lines.trunc() as i32;
                    if n != 0 {
                        if let Some((row, col)) = self.spot() {
                            for _ in 0..n.abs() {
                                self.grid.roll(row, col, n > 0);
                            }
                        }
                    }
                } else {
                    self.grid.wheel(lines);
                    self.show();
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(view) = self.view.as_mut() {
                    view.draw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, loop_: &ActiveEventLoop) {
        if self.feed() {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        if self.grid.blinked() {
            let now = std::time::Instant::now();
            match self.next {
                Some(due) if now < due => {
                    loop_.set_control_flow(ControlFlow::WaitUntil(due));
                }
                _ => {
                    self.grid.flip();
                    self.show();
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    let due = now + std::time::Duration::from_millis(530);
                    self.next = Some(due);
                    loop_.set_control_flow(ControlFlow::WaitUntil(due));
                }
            }
        } else if self.next.take().is_some() {
            loop_.set_control_flow(ControlFlow::Wait);
        }
    }
}
