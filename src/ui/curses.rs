use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::{self, cmp, num, env};
use std::ffi::AsOsStr;
use core::str::StrExt;
use ncurses as nc;

use super::Action;
use game;
use game::area;
use actor::{self, Behavior, Slot};
use ui;
use item;

use hex2d::{Angle, IntegerSpacing, Coordinate, ToCoordinate, Position};

use game::tile;

use std::fmt;
use std::fmt::Write;

mod locale {
    use libc::{c_int, c_char};
    pub const LC_ALL: c_int = 6;
    extern "C" {
        pub fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;
    }
}

//        . . .
//       . . . .
//      . . . . .
//       . . . .
//        . . .
static SPACING: IntegerSpacing<i32> = IntegerSpacing::PointyTop(2, 1);

const NORMAL_DOT : &'static str = ".";
const UNICODE_DOT : &'static str = "·";

pub fn item_to_str(t : item::Type) -> &'static str {
    match t {
        item::Type::Weapon => ")",
        item::Type::Armor => "[",
    }
}

pub mod color {
    use std::collections::HashMap;
    use std::collections::hash_map::Entry;
    use ncurses as nc;

    pub const GRAY : [u8; 26] = [
        16, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243,
        244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255, 15
    ];
    pub const BLACK : u8 = GRAY[0];
    pub const WHITE : u8 = GRAY[25];

    pub const BACKGROUND_BG : u8 = GRAY[2];
    pub const MAP_BACKGROUND_BG : u8 = GRAY[2];

    pub const VISIBLE_FG : u8 = WHITE;

    // in light, shaded (barely visible), out of sight
    pub const EMPTY_FG : [u8; 3] = [GRAY[17], GRAY[12], GRAY[5]];
    pub const EMPTY_BG : [u8; 3] = [GRAY[24], GRAY[22], GRAY[6]];
    pub const WALL_FG : [u8; 3] = [BLACK, GRAY[1] , GRAY[2]];
    pub const WALL_BG : [u8; 3] = [GRAY[14], GRAY[8] , GRAY[4]];
    pub const CHAR_SELF_FG : [u8; 3] = [19, 18, 17];
    pub const CHAR_ALLY_FG : [u8; 3] = [28, 22, 23];
    pub const CHAR_ENEMY_FG : [u8; 3] = [124, 88, 52];
    pub const CHAR_GRAY_FG : u8= GRAY[17];
    pub const CHAR_BG : [u8; 3] = EMPTY_BG;
    pub const TREE_FG : [u8; 3] = CHAR_ALLY_FG;
    pub const TREE_BG : [u8; 3] = EMPTY_BG;

    pub const LABEL_FG: u8 = 94;
    pub const GREEN_FG: u8 = 34;
    pub const RED_FG:   u8 = 124;
    pub const TARGET_SELF_FG : u8 = 20;
    pub const TARGET_ENEMY_FG : u8 = 196;
    pub const LIGHTSOURCE : u8 = 227;
    pub const LOG_1_FG : u8 = GRAY[25];
    pub const LOG_2_FG : u8 = GRAY[21];
    pub const LOG_3_FG : u8 = GRAY[17];
    pub const LOG_4_FG : u8 = GRAY[13];
    pub const LOG_5_FG : u8 = GRAY[9];

    pub struct Allocator {
        map : HashMap<(u8, u8), i16>,
        cur : i16,
    }

    impl Allocator {
        pub fn new() -> Allocator {
            Allocator {
                cur: 1i16, /* 0 is reserved for defaults */
                map: HashMap::new(),
            }
        }

        pub fn get(&mut self, fg : u8, bg : u8) -> i16 {
            match self.map.entry((fg, bg)) {
                Entry::Occupied(i) => *i.get(),
                Entry::Vacant(i) => {
                    assert!((self.cur as i32) < nc::COLOR_PAIRS, "curses run out of color pairs!");
                    let ret = self.cur;
                    i.insert(self.cur);
                    nc::init_pair(ret, fg as i16, bg as i16);
                    self.cur += 1;
                    ret
                }
            }
        }
    }
}

pub struct Window {
    pub window : nc::WINDOW,
}

pub struct LogEntry {
    turn : u64,
    text : String,
}

impl Window {
    pub fn new(w : i32, h : i32, x : i32, y : i32) -> Window {
        Window {
            window : nc::subwin(nc::stdscr, h, w, y, x),
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        nc::delwin(self.window);
    }
}

pub struct CursesUI {
    calloc : RefCell<color::Allocator>,

    map_window : Option<Window>,
    log_window : Option<Window>,
    stats_window : Option<Window>,
    fs_window : Option<Window>,
    mode : Mode,
    log : VecDeque<LogEntry>,
    examine_pos : Option<Position>,
    dot : &'static str,

    label_color: u64,
    text_color: u64,
    text_gray_color: u64,
    red_color: u64,
    green_color: u64,
}

impl CursesUI {

    pub fn new() -> CursesUI {

        let term_ok = env::var_os("TERM").as_ref()
            .and_then(|s| s.as_os_str().to_str())
            .map_or(false, |s| s.ends_with("-256color"));

        let term_putty = env::var_os("TERM").as_ref()
            .and_then(|s| s.as_os_str().to_str())
            .map_or(false, |s| s.starts_with("putty"));

        if !term_ok {
            panic!("Your TERM environment variable must end with -256color, sorry, stranger from the past. It is curable. Google it, fix it, try again.");
        }

        if env::var_os("ESCDELAY").is_none() {
            env::set_var("ESCDELAY", "25");
        }

        unsafe {
            let _ = locale::setlocale(locale::LC_ALL, b"en_US.UTF-8\0".as_ptr() as *const i8);
        }


        nc::initscr();
        nc::start_color();
        nc::keypad(nc::stdscr, true);
        nc::noecho();
        nc::raw();
        nc::timeout(0);
        nc::flushinp();

        assert!(nc::has_colors());

        let mut calloc = color::Allocator::new();
        let label_color = nc::COLOR_PAIR(
            calloc.get(color::LABEL_FG, color::BACKGROUND_BG)
            );
        let text_color = nc::COLOR_PAIR(
            calloc.get(color::VISIBLE_FG, color::BACKGROUND_BG)
            );
        let text_gray_color = nc::COLOR_PAIR(
            calloc.get(color::GRAY[10], color::BACKGROUND_BG)
            );
        let green_color = nc::COLOR_PAIR(
            calloc.get(color::GREEN_FG, color::BACKGROUND_BG)
            );
        let red_color = nc::COLOR_PAIR(
            calloc.get(color::RED_FG, color::BACKGROUND_BG)
            );

        nc::doupdate();

        let mut ret = CursesUI {
            calloc: RefCell::new(calloc),
            map_window: None,
            stats_window: None,
            log_window: None,
            fs_window: None,
            mode : Mode::Normal,
            examine_pos : None,
            dot: if term_putty { NORMAL_DOT } else { UNICODE_DOT },
            log : VecDeque::new(),
            label_color: label_color,
            text_color: text_color,
            text_gray_color: text_gray_color,
            red_color: red_color,
            green_color: green_color,
        };

        ret.resize();
        ret
    }

    fn resize(&mut self) {

        let mut max_x = 0;
        let mut max_y = 0;
        nc::getmaxyx(nc::stdscr, &mut max_y, &mut max_x);

        let mid_x = max_x - 30;
        let mid_y = 12;

        let map_window = Window::new(
                mid_x, max_y, 0, 0
                );
        let stats_window = Window::new(
                max_x - mid_x, mid_y, mid_x, 0
                );
        let log_window = Window::new(
                max_x - mid_x, max_y - mid_y, mid_x, mid_y
                );
        let fs_window = Window::new(
                max_x, max_y, 0, 0
                );

        self.map_window = Some(map_window);
        self.stats_window = Some(stats_window);
        self.log_window = Some(log_window);
        self.fs_window = Some(fs_window);
    }

    pub fn log(&mut self, s : &str, gstate : &game::State) {
        self.log.push_front(LogEntry{
            text: s.to_string(), turn: gstate.turn
        });
    }

    pub fn display_intro(&mut self) {
        self.mode = Mode::FullScreen(FSMode::Intro);
    }

    fn draw_map(
        &mut self,
        astate : &actor::State, gstate : &game::State,
        )
    {
        let mut calloc = self.calloc.borrow_mut();

        let window = self.map_window.as_ref().unwrap().window;

        let actors_aheads : HashMap<Coordinate, Coordinate> =
            gstate.actors.iter().map(|(_, a)| (a.pos.coord + a.pos.dir, a.pos.coord)).collect();
        let astate_ahead = astate.pos.coord + astate.pos.dir;

        /* Get the screen bounds. */
        let mut max_x = 0;
        let mut max_y = 0;
        nc::getmaxyx(window, &mut max_y, &mut max_x);

        let mid_x = max_x / 2;
        let mid_y = max_y / 2;

        let cpair = nc::COLOR_PAIR(calloc.get(color::VISIBLE_FG, color::MAP_BACKGROUND_BG));
        nc::wbkgd(window, ' ' as nc::chtype | cpair as nc::chtype);
        nc::werase(window);

        let (center, head) = match self.mode {
            Mode::Examine => {
                match self.examine_pos {
                    None => {
                        self.examine_pos = Some(astate.pos);
                        (astate.pos.coord, astate.pos.coord + astate.pos.dir)
                    },
                    Some(pos) => {
                        (pos.coord, pos.coord + pos.dir)
                    },
                }
            },
            _ => {
                (astate.pos.coord, astate.pos.coord + astate.pos.dir)
            }
        };

        let (vpx, vpy) = center.to_pixel_integer(SPACING);

        for vy in range(0, max_y) {
            for vx in range(0, max_x) {
                let (rvx, rvy) = (vx - mid_x, vy - mid_y);

                let (cvx, cvy) = (rvx + vpx, rvy + vpy);

                let (c, off) = Coordinate::from_pixel_integer(SPACING, (cvx, cvy));

                let is_proper_coord = off == (0, 0);

                let (visible, mut draw, tt, t, light) = if is_proper_coord {

                    let t = gstate.map.get(&c);

                    let tt = match t {
                        Some(t) => t.type_,
                        None => tile::Wall,
                    };

                    (astate.sees(c) || astate.is_dead(), astate.knows(c), Some(tt), t, gstate.at(c).light())
                } else {
                    // Paint a glue characters between two real characters
                    let c1 = c;
                    let (c2, _) = Coordinate::from_pixel_integer(SPACING, (cvx + 1, cvy));

                    let knows = astate.knows(c1) && astate.knows(c2);

                    let (e1, e2) = (
                        gstate.at(c1).tile_map_or(tile::Wall, |t| t.type_).ascii_expand(),
                        gstate.at(c2).tile_map_or(tile::Wall, |t| t.type_).ascii_expand()
                        );

                    let c = Some(if e1 > e2 { c1 } else { c2 });

                    let tt = c.map_or(None, |c| gstate.at(c).tile_map_or(Some(tile::Wall), |t| Some(t.type_)));

                    let visible = (astate.sees(c1) && astate.sees(c2)) || astate.is_dead();

                    (visible, knows, tt, None, (gstate.at(c1).light() + gstate.at(c2).light()) / 2)
                };

                let mut bold = false;
                let occupied = gstate.at(c).is_occupied();
                let (fg, bg, mut glyph) =
                    if is_proper_coord && visible && occupied {
                        let fg = match gstate.at(c).actor_map_or(Behavior::Grue, |a| a.behavior) {
                            Behavior::Player => color::CHAR_SELF_FG,
                            Behavior::Pony => color::CHAR_ALLY_FG,
                            Behavior::Grue => color::CHAR_ENEMY_FG,
                        };
                        (fg, color::CHAR_BG, "@")
                    } else if is_proper_coord && visible && gstate.at(c).item().is_some() {
                        let item = gstate.at(c).item().unwrap();
                        (color::WALL_FG, color::EMPTY_BG, item_to_str(item.type_()))
                    } else {
                        match tt {
                            Some(tile::Empty) => {
                                (
                                    color::EMPTY_FG, color::EMPTY_BG,
                                    if is_proper_coord { self.dot } else { " " }
                                 )
                            },
                            Some(tile::Wall) => {
                                (color::WALL_FG, color::WALL_BG, "#")
                            },
                            Some(tile::Door(open)) => {
                                (color::WALL_FG, color::WALL_BG,
                                 if open { "_" } else { "+" })
                            },
                            Some(tile::Tree) => {
                                (color::TREE_FG, color::TREE_BG, "T")
                            },
                            None => {
                                (color::EMPTY_FG, color::EMPTY_BG, " ")
                            },
                        }
                    };


                let (mut fg, mut bg) = if !visible || light == 0 {
                    (fg[2], bg[2])
                } else if light < 3 {
                    (fg[1], bg[1])
                } else {
                    (fg[0], bg[0])
                };

                if let Some(t) = t {
                    if visible && t.light > 0 {
                        fg = color::LIGHTSOURCE;
                    }
                }

                if is_proper_coord && visible && gstate.at(c).actor_map_or(0, |a| a.light) > 0u32 {
                    bg = color::LIGHTSOURCE;
                }

                if self.mode == Mode::Examine {
                    if is_proper_coord && center == c {
                        glyph = "@";
                        fg = color::CHAR_GRAY_FG;
                        draw = true;
                    } else if is_proper_coord && c == head {
                        bold = true;
                        if astate.knows(c) {
                            fg = color::TARGET_SELF_FG;
                        } else {
                            draw = true;
                            glyph = " ";
                            bg = color::TARGET_SELF_FG;
                        }
                    }
                } else {
                    if is_proper_coord && actors_aheads.contains_key(&c) &&
                        astate.sees(*actors_aheads.get(&c).unwrap()) {
                        bold = true;
                        let color = if c == astate_ahead {
                            color::TARGET_SELF_FG
                        } else {
                            color::TARGET_ENEMY_FG
                        };

                        if astate.knows(c) {
                            if occupied {
                                bg = color;
                            } else {
                                fg = color;
                            }
                        } else {
                            draw = true;
                            glyph = " ";
                            bg = color;
                        }
                    }
                }

                if draw {
                    let cpair = nc::COLOR_PAIR(calloc.get(fg, bg));

                    if bold {
                        nc::wattron(window, nc::A_BOLD() as i32);
                    }

                    nc::wattron(window, cpair as i32);
                    nc::mvwaddstr(window, vy, vx, glyph);
                    nc::wattroff(window, cpair as i32);

                    if bold {
                        nc::wattroff(window, nc::A_BOLD() as i32);
                    }
                }

            }
        }

        nc::wnoutrefresh(window);
    }

    fn draw_stats_bar(&mut self, window : nc::WINDOW, name : &str,
                      cur : i32, prev : i32, max : i32) {

        let mut max_x = 0;
        let mut max_y = 0;
        nc::getmaxyx(window, &mut max_y, &mut max_x);

        let cur = cmp::max(cur, 0) as u32;
        let prev = cmp::max(prev, 0) as u32;
        let max = cmp::max(max, 1) as u32;

        nc::wattron(window, self.label_color as i32);
        nc::waddstr(window, &format!("{}: ", name));

        let width = max_x as u32 - 4 - name.char_len() as u32;
        let cur_w = cur * width / max;
        let prev_w = prev * width / max;

        nc::wattron(window, self.text_color as i32);
        nc::waddstr(window, "[");
        for i in range(0, width) {
            let (color, s) = match (i < cur_w, i < prev_w) {
                (true, true) => (self.text_color, "="),
                (false, true) => (self.red_color, "-"),
                (true, false) => (self.green_color, "+"),
                (false, false) => (self.text_color, " "),
            };
            nc::wattron(window, color as i32);
            nc::waddstr(window, s);
        }
        nc::wattron(window, self.text_color as i32);
        nc::waddstr(window, "]");
    }

    fn draw_turn<T>(&self, window : nc::WINDOW, label: &str, val: T)
        where T : num::Int+fmt::Display
    {
        nc::wattron(window, self.label_color as i32);
        nc::waddstr(window, &format!("{}: ", label));

        nc::wattron(window, self.text_color as i32);
        nc::waddstr(window, &format!("{:<8}", val));
    }

    fn draw_val<T>(&self, window : nc::WINDOW, label: &str, val: T)
        where T : num::Int+fmt::Display
    {
        nc::wattron(window, self.label_color as i32);
        nc::waddstr(window, &format!("{}:", label));

        nc::wattron(window, self.text_color as i32);
        nc::waddstr(window, &format!("{:>2} ", val));
    }

    fn draw_label(&self, window : nc::WINDOW, label: &str) {
        nc::wattron(window, self.label_color as i32);
        nc::waddstr(window, &format!("{}:", label));
    }

    fn draw_item(&self, window : nc::WINDOW, astate : &actor::State, label: &str, slot : actor::Slot) {
        self.draw_label(window, label);

        if slot == Slot::RHand && astate.attack_cooldown > 0 {
            nc::wattron(window, self.text_gray_color as i32);
        } else {
            nc::wattron(window, self.text_color as i32);
        }

        let item = if let Some(&(_, ref item)) = astate.equipped.get(&slot) {
            item.description().to_string()
        } else {
            if slot == Slot::RHand {
                "fist".to_string()
            } else {
                "-".to_string()
            }
        };

        let item = item.slice_chars(0, cmp::min(item.char_len(), 13));
        nc::waddstr(window, &format!("{:^13}", item));
    }

    fn draw_inventory(&mut self, astate : &actor::State, _gstate : &game::State) {
        let window = self.map_window.as_ref().unwrap().window;

        let cpair = self.text_color;
        nc::wbkgd(window, ' ' as nc::chtype | cpair as nc::chtype);

        nc::werase(window);
        nc::wmove(window, 0, 0);
        if astate.equipped.iter().any(|_| true) {
            nc::waddstr(window, &format!("Equipped: \n"));
            for (slot, &(ref ch, ref i)) in &astate.equipped {
                nc::waddstr(window, &format!(" {} - {} [{:?}]\n", ch, i.description(), slot));
            }
            nc::waddstr(window, &format!("\n"));
        }

        if astate.items.iter().any(|_| true) {
            nc::waddstr(window, &format!("Inventory: \n"));

            for (ch, i) in &astate.items {
                nc::waddstr(window, &format!(" {} - {}\n", ch, i.description()));
            }
            nc::waddstr(window, &format!("\n"));
        }

        nc::wnoutrefresh(window);
    }

    fn draw_stats(&mut self, astate : &actor::State, gstate : &game::State) {
        let window = self.stats_window.as_ref().unwrap().window;

        let cpair = self.text_color;
        nc::wbkgd(window, ' ' as nc::chtype | cpair as nc::chtype);

        nc::werase(window);
        nc::wmove(window, 0, 0);

        let mut max_x = 0;
        let mut max_y = 0;
        nc::getmaxyx(window, &mut max_y, &mut max_x);

        let mut y = 0;
        nc::wmove(window, y, 0);
        self.draw_val(window, "Str", astate.stats.str_);
        nc::wmove(window, y, 8);
        self.draw_val(window, "AC", 2);

        y += 1;
        nc::wmove(window, y, 0);
        self.draw_val(window, "Int", astate.stats.int);
        nc::wmove(window, y, 8);
        self.draw_val(window, "EV", 3);

        y += 1;
        nc::wmove(window, y, 0);
        self.draw_val(window, "Dex", astate.stats.dex);

        y += 1;
        nc::wmove(window, y, 0);

        self.draw_stats_bar(window, "HP",
                            astate.stats.hp, astate.prev_stats.hp,
                            astate.stats.max_hp);

        y += 1;
        nc::wmove(window, y, 0);
        self.draw_stats_bar(window, "MP",
                            astate.stats.mp, astate.prev_stats.mp,
                            astate.stats.max_mp);

        y += 1;
        nc::wmove(window, y, 0);
        self.draw_stats_bar(window, "XP", 50, 50, 100);

        let slots = [
            ("L", Slot::LHand),
            ("R", Slot::RHand),
            ("Q", Slot::Quick),
        ];

        for &(string, slot) in &slots {
            y += 1;
            nc::wmove(window, y, 0);
            self.draw_item(window, astate, string, slot);
        }

        y += 1;
        nc::wmove(window, y, 0);

        let pos = if self.mode == Mode::Examine {
            self.examine_pos.unwrap()
        } else {
            astate.pos
        };

        let head = pos.coord + pos.dir;
        let descr = self.tile_description(head, astate, gstate);
        self.draw_label(window, "In front");
        nc::wattron(window, self.text_color as i32);
        nc::waddstr(window, &format!(" {}", descr));

        y += 1;
        nc::wmove(window, y, 0);
        self.draw_turn(window, "Turn", gstate.turn);

        nc::wnoutrefresh(window);
    }

    // TODO: Consider the distance to the Item to print something
    // like "you see x in the distance", "you find yourself in x".
    fn format_areas<I>(&self, mut i : I) -> Option<String>
        where I : Iterator, <I as Iterator>::Item : fmt::Display
        {
            if let Some(descr) = i.next() {
                let mut s = String::new();
                write!(&mut s, "{}", "You see: ").unwrap();
                write!(&mut s, "{}", descr).unwrap();

                for ref descr in i {
                    write!(&mut s, ", ").unwrap();
                    write!(&mut s, "{}", descr).unwrap();
                }

                write!(&mut s, ".").unwrap();
                Some(s)
            } else {
                None
            }
        }

    fn turn_to_color(
        &self, turn : u64, calloc : &RefCell<color::Allocator>,
        gstate : &game::State) -> Option<i16>
    {
        let mut calloc = calloc.borrow_mut();

        let dturn = gstate.turn - turn;

        let fg = if dturn < 1 {
            Some(color::LOG_1_FG)
        } else if dturn < 4 {
            Some(color::LOG_2_FG)
        } else if dturn < 16 {
            Some(color::LOG_3_FG)
        } else if dturn < 32 {
            Some(color::LOG_4_FG)
        } else if dturn < 64 {
            Some(color::LOG_5_FG)
        } else {
            None
        };

        fg.map(|fg| calloc.get(fg, color::BACKGROUND_BG))
    }

    fn tile_description(&self, coord : Coordinate,
                        astate : &actor::State, gstate : &game::State
                        ) -> String
    {
        if !astate.knows(coord) {
            return "Unknown".to_string();
        }

        let tile_type = gstate.at(coord).tile_map_or(tile::Wall, |t| t.type_);
        let tile = gstate.at(coord).tile_map_or(None, |t| Some(t.clone()));
        let item = gstate.at(coord).item_map_or(None, |i| Some(i.description().to_string()));

        let actor =
            if astate.sees(coord) || astate.is_dead() {
                gstate.at(coord).actor_map_or(None, |a| Some(match a.behavior {
                    Behavior::Pony => "A pony",
                    Behavior::Grue => "Toothless Grue",
                    Behavior::Player => "Yourself",
                }.to_string())
                )
            } else {
                None
            };

        match (tile_type, actor, item) {
            (tile::Wall, _, _) => {
                "a wall".to_string()
            },
            (tile::Door(_), _, _) => {
                "door".to_string()
            },
            (tile::Empty, None, None) => {
                match tile.and_then(|t| t.area).and_then(|a| Some(a.type_)) {
                    Some(area::Room(_)) => "room".to_string(),
                    None => "nothing".to_string()
                }
            },
            (tile::Empty, Some(descr), _) => {
                descr
            },
            (tile::Empty, None, Some(item)) => {
                item
            },
            _ => {
                "Indescribable".to_string()
            },
        }
    }

    fn draw_log(&mut self, _ : &actor::State, gstate : &game::State) {
        let window = self.log_window.as_ref().unwrap().window;
       
        let cpair = nc::COLOR_PAIR(self.calloc.borrow_mut().get(color::VISIBLE_FG, color::BACKGROUND_BG));
        nc::wbkgd(window, ' ' as nc::chtype | cpair as nc::chtype);
        nc::werase(window);
        nc::wmove(window, 0, 0);
        for i in &self.log {

            if nc::getcury(window) == nc::getmaxy(window) - 1 {
                break;
            }

            if let Some(color) = self.turn_to_color(i.turn, &self.calloc, gstate) {
                let cpair = nc::COLOR_PAIR(color);
                nc::wattron(window, cpair as i32);
                nc::waddstr(window, &format!(
                        "{:>2} {}\n", gstate.turn - i.turn, i.text.as_slice()
                        ));
            }
        }

        nc::wnoutrefresh(window);
    }

    fn draw_intro(&mut self)
    {
        let window = self.fs_window.as_ref().unwrap().window;
        let mut calloc = self.calloc.borrow_mut();
        let cpair = nc::COLOR_PAIR(calloc.get(color::VISIBLE_FG, color::BACKGROUND_BG));
        nc::wbkgd(window, ' ' as nc::chtype | cpair as nc::chtype);
        nc::werase(window);
        nc::wmove(window, 0, 0);

        nc::waddstr(window, "Long, long ago in a galaxy far, far away...\n\n");
        nc::waddstr(window, "You can press '?' in the game for help.\n\n");
        nc::waddstr(window, "Press anything to start.");
        nc::wnoutrefresh(window);
    }

    fn draw_help( &mut self) {
        let window = self.fs_window.as_ref().unwrap().window;
        let mut calloc = self.calloc.borrow_mut();
        let cpair = nc::COLOR_PAIR(calloc.get(color::VISIBLE_FG, color::BACKGROUND_BG));
        nc::wbkgd(window, ' ' as nc::chtype | cpair as nc::chtype);
        nc::werase(window);
        nc::wmove(window, 0, 0);

        nc::waddstr(window, "This game have no point (yet) and is incomplete. Sorry for that.\n\n");
        nc::waddstr(window, "= Implemented commands = \n\n");
        nc::waddstr(window, "Move: hjklui\n");
        nc::waddstr(window, "Wait: .\n");
        nc::waddstr(window, "Autoexplore: O\n");
        nc::waddstr(window, "Examine: x\n");
        nc::waddstr(window, "Equip: E\n");
        nc::waddstr(window, "Inventory: I\n");
        nc::wnoutrefresh(window);
    }

    fn draw_quit( &mut self) {
        let window = self.fs_window.as_ref().unwrap().window;
        let mut calloc = self.calloc.borrow_mut();
        let cpair = nc::COLOR_PAIR(
            calloc.get(color::VISIBLE_FG, color::BACKGROUND_BG)
            );

        let mut max_x = 0;
        let mut max_y = 0;
        nc::getmaxyx(nc::stdscr, &mut max_y, &mut max_x);
        let text = "Quit. Are you sure?";

        nc::wbkgd(window, ' ' as nc::chtype | cpair as nc::chtype);
        nc::werase(window);
        nc::wmove(window, max_y / 2, (max_x  - text.char_len() as i32) / 2);

        nc::waddstr(window, text);
        nc::wnoutrefresh(window);
    }

}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum InvMode {
    View,
    Equip,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum FSMode {
    Help,
    Intro,
    Quit,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Mode {
    Normal,
    Examine,
    FullScreen(FSMode),
    Inventory(InvMode),
}

impl ui::UiFrontend for CursesUI {

    fn update(&mut self, astate : &actor::State, gstate : &game::State) {
        let discoviered_areas = astate.discovered_areas.iter()
            .filter_map(|coord| gstate.at(*coord).tile_map_or(None, |t| t.area))
            ;

        if let Some(s) = self.format_areas(discoviered_areas.map(|area| area.type_)) {
            self.log(&s, gstate);
        }

        if astate.were_hit {
            self.log("X hit you", gstate);
        }

        if astate.did_hit {
            self.log("You hit X", gstate);
        }
    }

    fn draw(&mut self, astate : &actor::State, gstate : &game::State) {
        let mut max_x = 0;
        let mut max_y = 0;
        nc::getmaxyx(nc::stdscr, &mut max_y, &mut max_x);

        match self.mode {
            Mode::Normal|Mode::Examine|Mode::Inventory(_) => {
                if let Mode::Inventory(_) = self.mode {
                    self.draw_inventory(astate, gstate);
                } else {
                    self.draw_map(astate, gstate);
                }

                self.draw_log(astate, gstate);

                self.draw_stats(astate, gstate);
            },
            Mode::FullScreen(fs_mode) => match fs_mode {
                FSMode::Help => {
                    self.draw_help();
                },
                FSMode::Quit => {
                    self.draw_quit();
                },
                FSMode::Intro => {
                    self.draw_intro();
                },
            },
        }

        nc::mv(max_y - 1, max_x - 1);
        std::old_io::stdio::flush();
    }

    fn input(&mut self) -> Option<Action> {
        loop {
            let ch = nc::getch();
            if ch == nc::KEY_RESIZE {
                self.resize();
                return Some(Action::Redraw);
            }
            if ch == -1 {
                return None;
            }
            match self.mode {
                Mode::FullScreen(fs_mode) => match fs_mode {
                    FSMode::Quit => match ch as u8 as char {
                        'y'|'Y' => return Some(Action::Exit),
                        _ => {
                            self.mode = Mode::Normal;
                            return Some(Action::Redraw);
                        },
                    },
                    _ => {
                        match ch {
                            -1 =>
                                return None,
                            _ => {
                                self.mode = Mode::Normal;
                                return Some(Action::Redraw);
                            }
                        }
                    },
                },
                Mode::Normal => {
                    return Some(match (ch as u8) as char {
                        'h' => Action::Game(game::Action::Turn(Angle::Left)),
                        'l' => Action::Game(game::Action::Turn(Angle::Right)),
                        'k' => Action::Game(game::Action::Move(Angle::Forward)),
                        'u' => Action::Game(game::Action::Spin(Angle::Left)),
                        'i' => Action::Game(game::Action::Spin(Angle::Right)),
                        'H' => Action::Game(game::Action::Move(Angle::Left)),
                        'L' => Action::Game(game::Action::Move(Angle::Right)),
                        'j' => Action::Game(game::Action::Move(Angle::Back)),
                        '.' => Action::Game(game::Action::Wait),
                        ',' => Action::Game(game::Action::Pick),
                        'o' => Action::AutoExplore,
                        'q' => {
                            self.mode = Mode::FullScreen(FSMode::Quit);
                            return Some(Action::Redraw);
                        },
                        'I' =>  {
                            self.mode = Mode::Inventory(InvMode::View);
                            return Some(Action::Redraw);
                        },
                        'E' =>  {
                            self.mode = Mode::Inventory(InvMode::Equip);
                            return Some(Action::Redraw);
                        },
                        'x' =>  {
                            self.examine_pos = None;
                            self.mode = Mode::Examine;
                            return Some(Action::Redraw);
                        },
                        '?' => {
                            self.mode = Mode::FullScreen(FSMode::Help);
                            return Some(Action::Redraw);
                        },
                        _ => { return None }
                    })
                },
                Mode::Inventory(InvMode::Equip) => match ch {
                    -1 => return None,
                    ch => match ch as u8 as char {
                        'a'...'z'|'A'...'Z' => {
                            return Some(Action::Game(game::Action::Equip(ch as u8 as char)))
                        },
                        '\x1b' => {
                            self.mode = Mode::Normal;
                            return Some(Action::Redraw);
                        },
                        _ => {},
                    }
                },
                Mode::Inventory(InvMode::View) => match ch {
                    -1 => return None,
                    ch => match ch as u8 as char {
                        'a'...'z'|'A'...'Z' => {
                        },
                        '\x1b' => {
                            self.mode = Mode::Normal;
                            return Some(Action::Redraw);
                        },
                        _ => {},
                    }
                },
                Mode::Examine => {
                    if ch == -1 {
                        return None;
                    }

                    let pos = self.examine_pos.unwrap();

                    match ch as u8 as char {
                        'x' | 'q' => {
                            self.examine_pos = None;
                            self.mode = Mode::Normal;
                        },
                        'h' => {
                            self.examine_pos = Some(pos + Angle::Left);
                        },
                        'l' => {
                            self.examine_pos = Some(pos + Angle::Right);
                        },
                        'j' => {
                            self.examine_pos = Some(pos + (pos.dir + Angle::Back).to_coordinate());
                        },
                        'k' => {
                            self.examine_pos = Some(pos + pos.dir.to_coordinate());
                        },
                        'K' => {
                            self.examine_pos = Some(pos + pos.dir.to_coordinate().scale(5));
                        },
                        'J' => {
                            self.examine_pos = Some(pos + (pos.dir + Angle::Back).to_coordinate().scale(5));
                        },
                        _ => {
                            return None;
                        }
                    }
                    return Some(Action::Redraw);
                }
            }
        }
    }

    fn event(&mut self, event : ui::Event, gstate : &game::State) {
        match event {
            ui::Event::Log(logev) => match logev {
                ui::LogEvent::AutoExploreDone => self.log("Nothing else to explore.", gstate),
            }
        }
    }
}

impl Drop for CursesUI {
    fn drop(&mut self) {
        nc::clear();
        nc::refresh();
        nc::endwin();
    }
}
