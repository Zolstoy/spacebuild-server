extern crate tokio;
use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{Event, EventStream, KeyCode, MouseEventKind},
    ExecutableCommand,
};
use futures::StreamExt;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Position, Rect},
    style::{Color, Style},
    text::Text,
    widgets::{
        canvas::{Canvas, Circle, Points},
        Block, Borders, Cell, HighlightSpacing, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState,
    },
    DefaultTerminal, Frame,
};
use spacebuild::{
    client::Client,
    network::tls::ClientPki,
    protocol::{BodyInfo, GameInfo},
    Id,
};
use std::{collections::HashMap, time::Duration};

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
struct Args {
    #[arg(value_name = "HOST", default_value = "localhost")]
    host: String,

    #[arg(value_name = "PORT", default_value_t = 2567)]
    port: u16,

    #[arg(short,
        long,
        value_name = "CA_CERT_PATH",
        num_args(0..=1)
    )]
    tls: Option<Option<String>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let pki = if let Some(tls) = args.tls {
        if let Some(ca_cert_path) = tls {
            Some(ClientPki::Path { cert: ca_cert_path })
        } else {
            Some(ClientPki::WebPki)
        }
    } else {
        None
    };

    println!("Connecting to {}:{}", args.host, args.port);
    let mut client = Client::connect(format!("{}:{}", args.host, args.port).as_str(), pki).await?;

    println!("Logging in as observer");
    client.login("observer").await?;

    print!("Running app");
    let terminal = ratatui::init();
    let app_result = App::default().run(terminal, client).await;
    ratatui::restore();
    app_result
}

#[derive(Debug, Default)]
struct App {
    should_quit: bool,
    cursor: (u16, u16),
    celestials: HashMap<spacebuild::Id, spacebuild::protocol::BodyInfo>,
    star: BodyInfo,
    list_scroll: usize,
    list_area: Rect,
    draw_zoom: f64,
    draw_area: Rect,
    offset: (f64, f64),
    list_clicked: bool,
    nb_row_clicked: u16,
    left_button_down: bool,
}

impl App {
    const FRAMES_PER_SECOND: f32 = 60.0;

    pub async fn run(mut self, mut terminal: DefaultTerminal, mut client: Client) -> Result<()> {
        let period = Duration::from_secs_f32(1.0 / Self::FRAMES_PER_SECOND);
        let mut interval = tokio::time::interval(period);
        let mut events = EventStream::new();
        self.draw_zoom = 500.0;
        std::io::stdout()
            .execute(crossterm::event::EnableMouseCapture)
            .unwrap();

        while !self.should_quit {
            tokio::select! {
                _ = interval.tick() => {
                    terminal.draw(|frame| self.draw(frame))?;
                },
                Some(Ok(event)) = events.next() => {
                    self.handle_event(&event);
                },
                Ok(game_info) = client.next_game_info() => {
                    match game_info {
                        GameInfo::Player(_player_info) => {

                        },
                        GameInfo::BodiesInSystem(bodies) => {
                            for body in bodies {
                                if body.element_type == "Star" {
                                    self.star = body.clone();
                                }
                                self.celestials.insert(body.id, body);
                            }
                        },
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn draw(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(2, 10), Constraint::Min(0)])
            .split(f.area());

        let bandeau = Block::default().title("Server info").borders(Borders::ALL);
        f.render_widget(bandeau, chunks[0]);

        // let info = Paragraph::new(Text::from(
        //     format!("Celestials: {}", self.celestials.len(),),
        // ))
        // // .block(Block::default().borders(Borders::ALL).title("Info"))
        // .style(Style::new().fg(Color::White).bg(Color::Black));
        // f.render_widget(
        //     info,
        //     chunks[0].inner(Margin {
        //         vertical: 1,
        //         horizontal: 1,
        //     }),
        // );

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Ratio(4, 10)])
            .split(chunks[1]);

        self.list_area = main_chunks[1];
        self.draw_area = main_chunks[0];

        let header = ["Id", "Type", "X", "Y", "Z"]
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .height(1);

        let rows = self.celestials.iter().enumerate().map(|(i, data)| {
            let color = match i % 2 {
                0 => Color::Black,
                _ => Color::DarkGray,
            };
            let mut cells = vec![];
            cells.push(Cell::from(Text::from(format!("{}", data.0))));
            cells.push(Cell::from(Text::from(data.1.element_type.clone())));
            cells.push(Cell::from(Text::from(format!(
                "{}",
                data.1.coords[0] as i32
            ))));
            cells.push(Cell::from(Text::from(format!(
                "{}",
                data.1.coords[1] as i32
            ))));
            cells.push(Cell::from(Text::from(format!(
                "{}",
                data.1.coords[2] as i32
            ))));
            if self.list_clicked && self.nb_row_clicked as usize + self.list_scroll as usize == i {
                Row::new(cells).style(Style::new().fg(Color::Black).bg(Color::White))
            } else {
                Row::new(cells).style(Style::new().fg(Color::White).bg(color))
            }
        });

        let bar = " █ ";
        let celestials_table = Table::new(
            rows,
            [
                Constraint::Min(5),
                Constraint::Min(12),
                Constraint::Min(8),
                Constraint::Min(8),
                Constraint::Min(8),
            ],
        )
        .block(
            Block::default()
                .title("Celestials")
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::White)),
        )
        .header(header)
        .highlight_symbol(Text::from(vec![
            "".into(),
            bar.into(),
            bar.into(),
            "".into(),
        ]))
        .highlight_spacing(HighlightSpacing::Always);
        f.render_stateful_widget(
            celestials_table,
            self.list_area,
            &mut TableState::default().with_selected(self.list_scroll),
        );

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state =
            ScrollbarState::new(self.celestials.len()).position(self.list_scroll);

        f.render_stateful_widget(
            scrollbar,
            main_chunks[1].inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );

        if self.star.id != Id::default() {
            let cln = self.celestials.clone();
            let mut x_bounds = [
                -(self.draw_area.width as f64) / 2.,
                self.draw_area.width as f64 / 2.,
            ];
            let mut y_bounds = [
                -(self.draw_area.height as f64),
                self.draw_area.height as f64,
            ];

            x_bounds[0] += self.offset.0;
            x_bounds[1] += self.offset.0;
            y_bounds[0] += self.offset.1;
            y_bounds[1] += self.offset.1;

            x_bounds[0] *= self.draw_zoom;
            x_bounds[1] *= self.draw_zoom;
            y_bounds[0] *= self.draw_zoom;
            y_bounds[1] *= self.draw_zoom;

            let system_canvas = Canvas::default()
                .block(Block::default().title("System").borders(Borders::ALL))
                .x_bounds(x_bounds)
                .y_bounds(y_bounds)
                .paint(move |ctx| {
                    for celestials in cln.values() {
                        let mut coords = celestials.coords;
                        coords[0] -= self.star.coords[0];
                        coords[2] -= self.star.coords[2];

                        match celestials.element_type.as_str() {
                            "Star" => {
                                ctx.layer();
                                ctx.draw(&Circle {
                                    x: coords[0],
                                    y: coords[2],
                                    radius: 100.,
                                    color: Color::White,
                                });
                            }
                            "Planet" => {
                                ctx.layer();
                                ctx.draw(&Circle {
                                    x: coords[0],
                                    y: coords[2],
                                    radius: 40.,
                                    color: Color::Blue,
                                });
                            }
                            "Moon" => {
                                ctx.layer();
                                ctx.draw(&Circle {
                                    x: coords[0],
                                    y: coords[2],
                                    radius: 10.,
                                    color: Color::Yellow,
                                });
                            }
                            "Asteroid" => {
                                ctx.draw(&Points {
                                    coords: &vec![(coords[0], coords[2])],
                                    color: Color::Red,
                                });
                            }
                            "Player" => {
                                ctx.layer();
                                ctx.draw(&Circle {
                                    x: coords[0],
                                    y: coords[2],
                                    radius: 2.,
                                    color: Color::Green,
                                });
                            }
                            _ => {}
                        }
                    }
                });

            f.render_widget(system_canvas, main_chunks[0]);
        }
    }

    fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Key(key_event) => match key_event.code {
                KeyCode::Char('q') => {
                    self.should_quit = true;
                }
                // KeyCode::Char('j') => {
                //     self.list_scroll += 1;
                // }
                // KeyCode::Char('k') => {
                //     self.list_scroll = self.list_scroll.saturating_sub(1);
                // }
                _ => {}
            },

            Event::Mouse(event) => match event.kind {
                MouseEventKind::Down(button) => {
                    if button == crossterm::event::MouseButton::Left {
                        if self
                            .list_area
                            .inner(Margin {
                                vertical: 1,
                                horizontal: 1,
                            })
                            .contains(Position::new(event.column, event.row))
                        {
                            self.list_clicked = true;
                            self.nb_row_clicked =
                                event.row - self.list_area.y - 1 + self.list_scroll as u16;
                        }
                        self.left_button_down = true;
                    }
                }
                MouseEventKind::Up(button) => {
                    if button == crossterm::event::MouseButton::Left {
                        self.left_button_down = false;
                    }
                }
                MouseEventKind::Drag(_) => {
                    if self.left_button_down
                        && self
                            .draw_area
                            .inner(Margin {
                                vertical: 1,
                                horizontal: 1,
                            })
                            .contains(Position::new(event.column, event.row))
                    {
                        self.offset = (
                            self.offset.0 + (event.column as f64 - self.cursor.1 as f64),
                            self.offset.1 - (event.row as f64 - self.cursor.0 as f64),
                        );

                        self.cursor = (event.row, event.column);
                    }
                }
                MouseEventKind::Moved => {
                    self.cursor = (event.row, event.column);
                }
                MouseEventKind::ScrollUp => {
                    if self
                        .draw_area
                        .inner(Margin {
                            vertical: 1,
                            horizontal: 1,
                        })
                        .contains(Position::new(event.column, event.row))
                    {
                        if self.draw_zoom >= 10. {
                            self.draw_zoom -= 10.;
                        }
                    }

                    if self
                        .list_area
                        .inner(Margin {
                            vertical: 1,
                            horizontal: 1,
                        })
                        .contains(Position::new(event.column, event.row))
                    {
                        self.list_scroll = self.list_scroll.saturating_sub(1);
                    }
                }
                MouseEventKind::ScrollDown => {
                    if self
                        .draw_area
                        .contains(Position::new(event.column, event.row))
                    {
                        if self.draw_zoom <= 1000. {
                            self.draw_zoom += 10.;
                        }
                    }
                    if self
                        .list_area
                        .contains(Position::new(event.column, event.row))
                    {
                        self.list_scroll += 1;
                        if self.list_scroll >= self.celestials.len() {
                            self.list_scroll = self.celestials.len().saturating_sub(1);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}
