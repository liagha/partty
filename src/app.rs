use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::window::{Window, WindowId};

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
}

impl App {
    pub fn run() {
        let event = winit::event_loop::EventLoop::new().unwrap();
        event.set_control_flow(ControlFlow::Wait);
        let mut app = Self::default();
        app.wake = Some(event.create_proxy());
        event.run_app(&mut app).unwrap();
    }

    fn feed(&mut self) -> bool {
        let mut fresh = false;
        loop {
            match self.inbox.as_ref().map(|inbox| inbox.try_recv()) {
                Some(Ok(bytes)) => {
                    self.parse.advance(&mut self.grid, &bytes);
                    fresh = true;
                }
                _ => break,
            }
        }
        if fresh {
            self.show();
        }
        fresh
    }

    fn show(&mut self) {
        if !self.grid.take_dirty() {
            return;
        }
        let styled = self.grid.spans();
        if let Some(view) = self.view.as_mut() {
            view.show(&styled);
        }
    }

    fn fit(&mut self) {
        let window = match self.window.as_ref() {
            Some(window) => window.clone(),
            None => return,
        };
        let size = window.inner_size();
        let (rows, cols) = Text::cells(size.width, size.height, window.scale_factor() as f32);
        self.grid.resize(rows, cols);
        if let Some(shell) = self.shell.as_mut() {
            shell.resize(rows, cols);
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
        let config = Config::load();
        self.view = Some(View::open(window.clone(), loop_, config.bg));
        let size = window.inner_size();
        let (rows, cols) = Text::cells(size.width, size.height, window.scale_factor() as f32);
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
                let bytes = Shell::key(
                    event.state,
                    event.repeat,
                    &event.logical_key,
                    event.text.as_deref(),
                );
                if let Some(bytes) = bytes {
                    if let Some(shell) = self.shell.as_mut() {
                        shell.write(&bytes);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as f32,
                };
                self.grid.wheel(lines);
                self.show();
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Some(view) = self.view.as_mut() {
                    view.draw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _loop_: &ActiveEventLoop) {
        if self.feed() {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }
}
