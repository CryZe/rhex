use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc};

use hex2d::{Coordinate, Direction, Angle, Position};
use actor::{self};
use generate;
use hex2dext::algo;
use item::Item;

use self::tile::{Tile};
use hex2dext::algo::bfs;

pub mod area;
pub mod tile;
pub mod controller;

pub use self::controller::Controller;

pub type Map = HashMap<Coordinate, tile::Tile>;
pub type Actors = HashMap<Coordinate, Arc<actor::State>>;
pub type Items = HashMap<Coordinate, Box<Item>>;
pub type LightMap = HashMap<Coordinate, u32>;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Action {
    Wait,
    Turn(Angle),
    Move(Angle),
    Spin(Angle),
    Equip(char),
    Pick,
}

#[derive(Clone, Debug)]
pub struct State {
    pub actors: Actors,
    pub actors_orig: HashMap<Coordinate, Coordinate>, // from -> to
    pub actors_dead: Vec<Arc<actor::State>>,
    pub map : Arc<Map>,
    pub items: Arc<Items>,
    pub light_map: LightMap,
    pub turn : u64,
}

impl State {
    pub fn new() -> State {

        let cp = Coordinate::new(0, 0);
        let (map, actors, _items) = generate::DungeonGenerator.generate_map(cp, 400);

        State {
            actors: actors,
            actors_orig: HashMap::new(),
            actors_dead: Vec::new(),
            items: Arc::new(HashMap::new()),
            map: Arc::new(map),
            turn: 0,
            light_map: HashMap::new(),
        }
    }

    pub fn recalculate_noise(&mut self) {

        let mut actors = self.actors.clone();

        for (&source_coord, a) in self.actors.iter() {
            if a.noise_emision > 0 {
                source_coord.for_each_in_range(a.noise_emision, |coord| {
                    if let Some(mut actor) = actors.remove(&coord) {
                        let mut actor = actor.make_unique().clone();
                        actor.noise_hears(source_coord);
                        actors.insert(coord, Arc::new(actor));
                    }
                });
            }
        }
        self.actors = actors;
    }

    pub fn recalculate_light_map(&mut self) {
        let mut light_map : HashMap<Coordinate, u32> = HashMap::new();

        for (pos, tile) in &*self.map {
            let light = tile.light;
            if light > 0 {
                algo::los::los(
                    &|coord| {
                        if coord == *pos {
                            0
                        } else {
                            self.at(coord).tile_map_or(light, |tile| tile.opaqueness())
                        }
                    },
                    &mut |coord, light| {
                        match light_map.entry(coord) {
                            Entry::Occupied(mut entry) => {
                                let val = entry.get_mut();
                                if light as u32 > *val {
                                    *val = light as u32;
                                }
                            },
                            Entry::Vacant(entry) => {
                                entry.insert(light as u32);
                            },
                        }
                    },
                    light, *pos, Direction::all()
                    );
            }
        }

        for (pos, astate) in &self.actors {
            if astate.light_emision > 0 {
                algo::los::los(
                    &|coord| {
                        if coord == *pos {
                            0
                        } else {
                            self.at(coord).tile_map_or(astate.light_emision as i32, |tile| tile.opaqueness())
                        }
                    },
                    &mut |coord, light| {
                        match light_map.entry(coord) {
                            Entry::Occupied(mut entry) => {
                                let val = entry.get_mut();
                                if light as u32 > *val {
                                    *val = light as u32;
                                }
                            },
                            Entry::Vacant(entry) => {
                                entry.insert(light as u32);
                            },
                        }
                    },
                    astate.light_emision as i32, *pos, Direction::all()
                );
            }
        }

        self.light_map = light_map;
    }

    pub fn spawn(&self, coord : Coordinate,
                 behavior : actor::Behavior, light : u32) -> State {

        let mut actors = self.actors.clone();

        let pos = Position::new(coord, Direction::XY);

        let mut actor = actor::State::new(behavior, pos);
        actor.add_light(light);

        actors.insert(pos.coord, Arc::new(actor));

        State {
            actors: actors,
            actors_orig: self.actors_orig.clone(),
            actors_dead: self.actors_dead.clone(),
            items: self.items.clone(),
            map: self.map.clone(),
            turn: self.turn,
            light_map: self.light_map.clone(),
        }
    }

    pub fn spawn_player(&self) -> State {
        self.spawn(Coordinate::new(0, 0), actor::Behavior::Player, 0)
    }

    pub fn spawn_pony(&self, pos : Coordinate) -> State {
        self.spawn(pos, actor::Behavior::Pony, 7)
    }

    pub fn act(&mut self, acoord : Coordinate, action : Action) {

        let astate = self.actors[acoord].clone();

        if !astate.can_perform_action() {
            return;
        }

        let new_pos = astate.pos_after_action(action);

        if astate.pos == new_pos {
            // no movement
            match action {
                Action::Pick => {
                    let item = self.at_mut(
                        astate.pos.coord + astate.pos.dir
                        ).pick_item();

                    match item {
                        Some(item) => {
                            let mut astate = self.actors.remove(&astate.pos.coord).unwrap().make_unique().clone();
                            astate.add_item(item);
                            self.actors.insert(astate.pos.coord, Arc::new(astate));
                        },
                        None => {},
                    }
                },
                Action::Equip(ch) => {
                    let mut astate = self.actors.remove(&astate.pos.coord).unwrap().make_unique().clone();
                    astate.equip_switch(ch);
                    self.actors.insert(astate.pos.coord, Arc::new(astate));
                },
                _ => {}
            }
        } else if astate.pos.coord != new_pos.coord &&
            self.actors_orig.contains_key(&new_pos.coord)
            {
            // that is an attack!
            if !astate.can_attack() {
                return;
            }

            let dir = match action {
                Action::Move(dir) => astate.pos.dir + dir,
                _ => astate.pos.dir,
            };

            let mut astate = self.actors.remove(&astate.pos.coord).unwrap().make_unique().clone();
            let target_pos = self.actors_orig[new_pos.coord];
            let mut target = self.actors.remove(&target_pos).map(|mut t| t.make_unique().clone());

            astate.attacks(dir, target.as_mut());

            if let Some(target) = target {
                self.actors.insert(target.pos.coord,
                                   Arc::new(target)
                                   );
            }

            let coord = astate.pos.coord;
            self.actors.insert(coord, Arc::new(astate));
        } else if self.at(new_pos.coord).tile_map_or(
            false, |t| t.type_ == tile::Door(false)
            ) {

            let mut map = self.map.clone().make_unique().clone();
            map.insert(new_pos.coord, Tile::new(tile::Door(true)));
            self.map = Arc::new(map);

        } else if astate.pos.coord == new_pos.coord || self.at(new_pos.coord).is_passable() {
            // we've moved
            self.actors_orig.insert(astate.pos.coord, new_pos.coord);
            let mut astate = self.actors.remove(&astate.pos.coord).unwrap().make_unique().clone();
            astate.moved(new_pos);
            self.actors.insert(astate.pos.coord, Arc::new(astate));
        } else {
            // we hit the wall or something
        }
    }

    pub fn pre_tick(&mut self) {
        let mut actors = HashMap::new();
        let mut actors_orig = HashMap::new();
        for (coord, ref mut a) in self.actors.iter() {
            let mut a = a.clone().make_unique().clone();
            a.pre_tick(self);
            actors.insert(*coord, Arc::new(a));
            actors_orig.insert(*coord, *coord);
        }

        self.actors = actors;
        self.actors_orig = actors_orig;
    }

    /// Advance one turn (increase the turn counter) and do some maintenance
    pub fn post_tick(&mut self) {
        // filter out the dead
        let mut left_actors = HashMap::new();
        let mut new_dead_actors = Vec::new();

        for (&coord, a) in self.actors.iter() {
            if a.is_dead() {
                if a.behavior == actor::Behavior::Player {
                    new_dead_actors.push(a.clone());
                }
            } else {
                left_actors.insert(coord, a.clone());
            }
        }

        self.actors = left_actors;

        for a in new_dead_actors.iter() {
            self.actors_dead.push(a.clone());
        }

        self.recalculate_light_map();
        self.recalculate_noise();

        let mut actors = HashMap::new();

        for (&coord, a) in self.actors.iter() {
            let mut a = a.clone().make_unique().clone();
            a.post_tick(self);
            actors.insert(coord, Arc::new(a));
        }

        self.actors = actors;
        self.turn += 1;
    }

    pub fn at(&self, coord: Coordinate) -> At {
        At {
            coord: coord,
            state: self
        }
    }

    pub fn at_mut(&mut self, coord: Coordinate) -> AtMut {
        AtMut {
            coord: coord,
            state: self
        }
    }
}

pub struct At<'a> {
    coord : Coordinate,
    state : &'a State,
}

impl<'a> At<'a> {
    pub fn tile(&self) -> Option<&'a tile::Tile> {
        self.state.map.get(&self.coord)
    }

    pub fn tile_map_or<R, F>(&self, def: R, f : F) -> R
        where F : Fn(&tile::Tile) -> R
    {
        self.state.map.get(&self.coord).map_or(def, |a| f(a))
    }

    pub fn actor_map_or<R, F : Fn(&actor::State) -> R>
        (&self, def: R, cond : F) -> R
    {
        self.state.actors.get(&self.coord).map_or(def, |a| cond(a))
    }

    pub fn item_map_or<R, F : Fn(&Box<Item>) -> R>
        (&self, def: R, cond : F) -> R
    {
        self.state.items.get(&self.coord).map_or(def, |i| cond(i))
    }

    pub fn is_occupied(&self) -> bool {
        self.state.actors.contains_key(&self.coord)
    }

    pub fn is_passable(&self) -> bool {
        !self.is_occupied() && self.tile_map_or(false, |t| t.is_passable())
    }

    pub fn light(&self) -> u32 {
        self.state.light_map.get(&self.coord).map_or(0, |l| *l)
    }

    pub fn item(&self) -> Option<&'a Item> {
        self.state.items.get(&self.coord).map(|i| &**i)
    }
}

pub struct AtMut<'a> {
    coord : Coordinate,
    state : &'a mut State,
}

impl<'a> AtMut<'a> {
    /*
    pub fn to_at(&'a self) -> At<'a> {
        At {
            coord: self.coord,
            state: self.state
        }
    }*/

    pub fn drop_item(&mut self, item : Box<Item>) {
        let coord = {
            let mut bfs = bfs::Traverser::new(
                |coord| self.state.at(coord).tile_map_or(false, |t| t.is_passable()),
                |coord| self.state.items.get(&coord).is_none(),
                self.coord
                );

            bfs.find()
        };

        match coord {
            None => { /* destroy the item :/ */ },
            Some(coord) => {
                let mut items = self.state.items.clone().make_unique().clone();
                items.insert(coord, item);
                self.state.items = Arc::new(items);
            }
        }
    }

    pub fn pick_item(&mut self) -> Option<Box<Item>> {
        if self.state.items.get(&self.coord).is_some() {
            let mut items = self.state.items.clone().make_unique().clone();
            let item = items.remove(&self.coord);
            self.state.items = Arc::new(items);
            item
        } else {
            None
        }
    }
}
